#!/usr/bin/env python3
"""Read-only Cloudflare backup plus local restore verification.

Reads production without writing to it: `wrangler d1 export` (read), R2
object listing and downloads (GET only). Then restores the snapshot into a
fresh temporary native data directory and serves it locally, verifying that
every space, message, attachment and memory revision reads back with keys
from the operator's own env file. Temporary material is removed afterwards;
only the encrypted archive, its backup key and the JSON report stay in
--archive-dir. Secret values are never printed or logged.
"""
import argparse
import hashlib
import http.cookiejar
import json
import os
import shutil
import socket
import sqlite3
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
API = 'https://api.cloudflare.com/client/v4/accounts/{account}'


def fail(reason):
    print(json.dumps({'result': 'failed', 'reason': reason}), flush=True)
    return 1


def ensure_node22(env):
    candidates = [Path.home() / '.nvm/versions/node/v22.23.1/bin']
    current = shutil.which('node', path=env.get('PATH', ''))
    if current:
        try:
            major = int(subprocess.run(
                [current, '--version'], capture_output=True, text=True, timeout=15
            ).stdout.strip().lstrip('v').split('.')[0])
            if major >= 22:
                return env
        except (ValueError, subprocess.SubprocessError):
            pass
    for directory in candidates:
        node = directory / 'node'
        if node.is_file():
            env = dict(env)
            env['PATH'] = str(directory) + os.pathsep + env.get('PATH', '')
            return env
    raise RuntimeError('Node.js 22+ is required for wrangler and was not found.')


def cf_get(account, token, path):
    request = urllib.request.Request(
        API.format(account=account) + path,
        headers={'Authorization': 'Bearer ' + token, 'User-Agent': 'Waypost/0.1.0'})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read(), dict(response.headers)


class Admin:
    """Minimal operator client mirroring dashboard/app.js login and inspect calls."""

    def __init__(self, origin, admin_secret):
        self.origin = origin
        self.admin_secret = admin_secret
        self.jar = http.cookiejar.CookieJar()
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.jar))
        self.csrf = None

    def call(self, method, path, body=None):
        data = json.dumps(body or {}).encode()
        request = urllib.request.Request(self.origin + path, data=data, method=method)
        request.add_header('Content-Type', 'application/json')
        if self.csrf:
            request.add_header('x-waypost-csrf', self.csrf)
        try:
            with self.opener.open(request, timeout=30) as response:
                return response.status, json.loads(response.read())
        except urllib.error.HTTPError as error:
            return error.code, json.loads(error.read())

    def login(self):
        status, result = self.call('POST', '/admin/session', {'admin_secret': self.admin_secret})
        if status != 200 or not result.get('ok'):
            raise RuntimeError(f'admin login failed with HTTP {status}')
        self.csrf = result['data']['csrf_token']

    def pages(self, path, body):
        cursor = None
        while True:
            status, result = self.call('POST', path, {**body, 'cursor': cursor})
            if status != 200 or not result.get('ok'):
                raise RuntimeError(f'{path} {body.get("kind")} failed with HTTP {status}')
            data = result['data']
            yield from data.get('items', [])
            if not data.get('has_more'):
                return
            cursor = data.get('next_cursor')


def free_port():
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        return listener.getsockname()[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', default='wrangler.production.toml')
    parser.add_argument('--env-file', default='.env.production')
    parser.add_argument('--archive-dir', required=True, type=Path,
                        help='Fresh directory (created 0700) keeping the encrypted archive, key and report.')
    parser.add_argument('--work-dir', type=Path, default=None)
    parser.add_argument('--keep-work-dir', action='store_true')
    parser.add_argument('--binary', type=Path, default=None)
    args = parser.parse_args()

    archive_dir = args.archive_dir.resolve()
    if archive_dir.exists():
        return fail('archive directory already exists; refuse to mix snapshots')
    keep_work = args.work_dir.resolve() if args.work_dir else Path(
        tempfile.mkdtemp(prefix='waypost-cf-verify-'))
    work = keep_work
    binary = (args.binary.resolve() if args.binary
              else (ROOT / 'target/debug/waypost').resolve())
    if not binary.is_file():
        return fail('native test binary is missing')

    config_text = (ROOT / args.config).read_text()
    try:
        import tomllib
    except ImportError:
        import tomli as tomllib
    config = tomllib.loads(config_text)
    account = config['account_id']
    bucket = config['r2_buckets'][0]['bucket_name']
    database = config['d1_databases'][0]['database_name']
    # Restored reads must match production's plaintext policy, or legacy rows
    # fail with "plaintext data requires migration".
    allow_legacy = str(config.get('vars', {}).get('ALLOW_LEGACY_PLAINTEXT', '')) == '1'

    secrets = {}
    for line in (ROOT / args.env_file).read_text().splitlines():
        name, _, value = line.partition('=')
        if name in ('WAYPOST_ADMIN_SECRET', 'WAYPOST_ENCRYPTION_KEYS') and value:
            secrets[name] = value
    if set(secrets) != {'WAYPOST_ADMIN_SECRET', 'WAYPOST_ENCRYPTION_KEYS'}:
        return fail('env file must provide both operator credential values')

    try:
        env = {'PATH': os.environ.get('PATH', ''), 'HOME': os.environ.get('HOME', ''),
               'WRANGLER_SEND_METRICS': 'false', 'CI': 'true'}
        env = ensure_node22(env)
    except RuntimeError as error:
        return fail(str(error))

    work.mkdir(mode=0o700, parents=True, exist_ok=True)
    server = None
    try:
        # Phase 1: read-only backup.
        dump_path = work / 'metadata.sql'
        with (work / 'export.log').open('w') as log:
            os.chmod(work / 'export.log', 0o600)
            subprocess.run(
                ['wrangler', 'd1', 'export', database, '--remote',
                 '--config', str(ROOT / args.config), '--output', str(dump_path)],
                env=env, cwd=str(ROOT), stdout=log, stderr=subprocess.STDOUT,
                check=True, timeout=180)
        os.chmod(dump_path, 0o600)
        dump_text = dump_path.read_text()

        token = json.loads(subprocess.run(
            ['wrangler', 'auth', 'token', '--json'], env=env, cwd=str(ROOT),
            capture_output=True, text=True, check=True, timeout=30).stdout)['token']

        objects, cursor = [], None
        while True:
            path = f'/r2/buckets/{bucket}/objects' + (
                '?cursor=' + urllib.parse.quote(cursor, safe='') if cursor else '')
            response = json.loads(cf_get(account, token, path)[0])
            if not response.get('success'):
                return fail('R2 listing was not successful')
            result = response['result']
            entries = result if isinstance(result, list) else result.get('objects', [])
            objects.extend(entries)
            cursor = ((response.get('result_info') or {}).get('cursor')
                      or (result.get('cursor') if isinstance(result, dict) else None))
            if not cursor:
                break

        object_dir = work / 'objects'
        object_dir.mkdir(mode=0o700)
        index = []
        for position, entry in enumerate(objects):
            key = entry.get('key') or entry.get('name')
            if not key:
                return fail('R2 listing entry without a key')
            payload, headers = cf_get(
                account, token, f'/r2/buckets/{bucket}/objects/{urllib.parse.quote(key, safe="")}')
            if len(payload) != int(entry.get('size', len(payload))):
                return fail(f'R2 download size mismatch for entry {position}')
            target = object_dir / f'{position}.bin'
            target.write_bytes(payload)
            os.chmod(target, 0o600)
            digest = hashlib.sha256(payload).hexdigest()
            content_type = (headers.get('Content-Type') or headers.get('content-type')
                            or 'application/octet-stream')
            index.append({'key': key, 'file': f'objects/{position}.bin',
                          'size_bytes': len(payload), 'sha256': digest,
                          'content_type': content_type})

        database_probe = sqlite3.connect(':memory:')
        try:
            database_probe.executescript(dump_text)
        except sqlite3.Error as error:
            return fail(f'D1 export is not importable SQLite: {error}')
        blob_objects = {row[0]: row[1] for row in database_probe.execute(
            'SELECT blob_id, object_key FROM blobs')}
        references = set()
        for table, column in (('messages', 'body_ref'), ('uploads', 'object_key'),
                              ('blobs', 'object_key')):
            references.update(row[0] for row in
                              database_probe.execute(f'SELECT {column} FROM {table}') if row[0])
        for (blob_id,) in database_probe.execute(
                'SELECT DISTINCT blob_id FROM memory_revision_items'):
            if blob_id in blob_objects:
                references.add(blob_objects[blob_id])
        downloaded = {item['key'] for item in index}
        if not references <= downloaded:
            return fail('snapshot references objects missing from R2 downloads')
        # Blob rows additionally record their own checksum: bytes must match it.
        db_checksums = {}
        for blob_id, object_key, size, sha in database_probe.execute(
                'SELECT blob_id, object_key, size_bytes, sha256 FROM blobs'):
            sha_hex = sha.hex() if isinstance(sha, bytes) else str(sha)
            db_checksums[blob_id] = (object_key, size, sha_hex)
        by_key = {}
        for blob_id, (object_key, size, sha_hex) in db_checksums.items():
            by_key.setdefault(object_key, []).append((blob_id, size, sha_hex))
        for item in index:
            for blob_id, size, sha_hex in by_key.get(item['key'], []):
                if item['size_bytes'] != size or item['sha256'] != sha_hex:
                    return fail(f'R2 bytes differ from the recorded blob {blob_id}')
        staging_refs = {row[0] for row in
                        database_probe.execute('SELECT object_key FROM object_writes') if row[0]}
        staging_dangling = sorted(staging_refs - downloaded)
        counts = {table: database_probe.execute(f'SELECT count(*) FROM {table}').fetchone()[0]
                  for table in ('spaces', 'messages', 'blobs', 'uploads',
                                'memory_collections', 'memory_revisions',
                                'memory_revision_items')}
        database_probe.close()

        manifest = {'format': 'waypost-cloudflare-readonly-v1',
                    'snapshot_boundary': 'D1 export first, then a complete R2 listing and download pass; no global write freeze',
                    'production_writes': False,
                    'objects': index, 'counts': counts,
                    'referenced_keys': len(references),
                    'staging_dangling': staging_dangling}
        (work / 'manifest.json').write_text(json.dumps(manifest, indent=2))
        os.chmod(work / 'manifest.json', 0o600)

        # Phase 2: restore into a fresh native layout (keys only via environment).
        restored = work / 'restored'
        restored.mkdir(mode=0o700)
        restored_db = restored / 'metadata.sqlite'
        import_db = sqlite3.connect(str(restored_db))
        try:
            import_db.executescript(dump_text)
        except sqlite3.Error as error:
            return fail(f'restored metadata is not usable SQLite: {error}')
        # The dump carries Cloudflare's own d1_migrations ledger, not the
        # native schema_migrations ledger. Without version alignment the
        # native runner would re-apply every migration, and non-idempotent
        # statements (CREATE INDEX/TRIGGER without IF NOT EXISTS, table
        # rebuilds, backfills) fail or duplicate rows on already-migrated
        # data. Translate only when both sides applied the same files.
        local_order = sorted(
            name for name in os.listdir(ROOT / 'migrations') if name.endswith('.sql'))
        applied = [row[0] for row in import_db.execute(
            'SELECT name FROM d1_migrations ORDER BY id')]
        if applied != local_order:
            return fail('dump migration set differs from local migrations; refusing restore')
        import_db.execute(
            'CREATE TABLE IF NOT EXISTS schema_migrations(version INTEGER PRIMARY KEY)')
        import_db.executemany(
            'INSERT OR IGNORE INTO schema_migrations(version) VALUES(?)',
            [(version,) for version in range(1, len(local_order) + 1)])
        import_db.commit()
        import_db.close()
        os.chmod(restored_db, 0o600)
        native_objects = restored / 'objects'
        native_objects.mkdir(mode=0o700)
        native_index = sqlite3.connect(str(native_objects / 'objects.sqlite'))
        native_index.execute('CREATE TABLE IF NOT EXISTS objects(object_key TEXT PRIMARY KEY,size INTEGER NOT NULL,sha256 BLOB NOT NULL,content_type TEXT NOT NULL)')
        for item in index:
            data = (object_dir / Path(item['file']).name).read_bytes()
            if hashlib.sha256(data).hexdigest() != item['sha256']:
                return fail('restored object bytes differ from the manifest')
            target = native_objects / item['key']
            target.write_bytes(data)
            os.chmod(target, 0o600)
            native_index.execute('INSERT INTO objects(object_key,size,sha256,content_type) VALUES(?,?,?,?)',
                                 (item['key'], item['size_bytes'],
                                  bytes.fromhex(item['sha256']), item['content_type']))
        native_index.commit()
        native_index.close()
        os.chmod(native_objects / 'objects.sqlite', 0o600)

        # Phase 3: serve the restored layout locally and verify every record reads.
        port = free_port()
        server_env = {'PATH': os.environ.get('PATH', ''), 'HOME': os.environ.get('HOME', ''),
                      'WAYPOST_PUBLIC_ORIGIN': f'http://127.0.0.1:{port}',
                      'WAYPOST_ADMIN_SECRET': secrets['WAYPOST_ADMIN_SECRET'],
                      'WAYPOST_ENCRYPTION_KEYS': secrets['WAYPOST_ENCRYPTION_KEYS']}
        if allow_legacy:
            server_env['WAYPOST_ALLOW_LEGACY_PLAINTEXT'] = '1'
        with (work / 'server.log').open('w') as log:
            os.chmod(work / 'server.log', 0o600)
            server = subprocess.Popen(
                [str(binary), 'serve', '--data', str(restored),
                 '--listen', f'127.0.0.1:{port}'],
                env=server_env, stdout=log, stderr=subprocess.STDOUT)
        admin = Admin(f'http://127.0.0.1:{port}', secrets['WAYPOST_ADMIN_SECRET'])
        for _ in range(500):
            if server.poll() is not None:
                return fail('restored service exited before becoming ready')
            try:
                admin.call('GET', '/health')
                break
            except OSError:
                time.sleep(0.02)
        else:
            return fail('restored service did not start')
        admin.login()
        spaces = [item['space_id'] for item in
                  admin.pages('/admin/v0/registry', {'kind': 'spaces'})]
        if len(spaces) != counts['spaces']:
            return fail('restored space count differs from the manifest')
        seen_messages = seen_attachments = seen_items = 0
        verified_attachment_bytes = binary_attachments = 0
        for space_id in spaces:
            messages = list(admin.pages(f'/admin/v0/spaces/{space_id}/inspect',
                                        {'kind': 'messages', 'limit': 100}))
            seen_messages += len(messages)
            for summary in messages:
                status, result = admin.call(
                    'POST', f'/admin/v0/spaces/{space_id}/inspect',
                    {'kind': 'message', 'resource_id': summary['message_id']})
                if status != 200 or not result.get('ok'):
                    return fail('a restored message failed to decrypt and read')
                message = result['data']
                if not message.get('subject') or message.get('body', {}).get('text') is None:
                    return fail('a restored message is missing readable content')
                for blob_id in message.get('attachment_ids', []):
                    seen_attachments += 1
                    status, result = admin.call(
                        'POST', f'/admin/v0/spaces/{space_id}/inspect',
                        {'kind': 'blob_content', 'resource_id': blob_id})
                    if status != 200 or not result.get('ok'):
                        return fail('a restored attachment failed to read')
                    text = result['data'].get('text')
                    if text is None:
                        binary_attachments += 1
                        continue
                    digest = hashlib.sha256(text.encode()).hexdigest()
                    object_key, _, sha_hex = db_checksums[blob_id]
                    if digest != sha_hex:
                        return fail('a restored attachment differs from its recorded checksum')
                    verified_attachment_bytes += 1
            collections = list(admin.pages(f'/admin/v0/spaces/{space_id}/inspect',
                                           {'kind': 'collections', 'limit': 100}))
            for collection in collections:
                status, result = admin.call(
                    'POST', f'/admin/v0/spaces/{space_id}/inspect',
                    {'kind': 'memory_revision', 'resource_id': collection['collection_id']})
                if status != 200 or not result.get('ok'):
                    return fail('a restored memory history failed to read')
                for revision in result['data'].get('revisions', []):
                    status, state = admin.call(
                        'POST', f'/admin/v0/spaces/{space_id}/inspect',
                        {'kind': 'memory', 'resource_id': collection['collection_id'],
                         'revision': revision['revision']})
                    if status != 200 or not state.get('ok'):
                        return fail('a restored memory revision failed to read')
                    for item in state['data'].get('items', []):
                        if not item.get('path'):
                            return fail('a restored memory item is missing its path')
                        seen_items += 1
                        status, content = admin.call(
                            'POST', f'/admin/v0/spaces/{space_id}/inspect',
                            {'kind': 'blob_content', 'resource_id': item['blob_id']})
                        if status != 200 or not content.get('ok'):
                            return fail('a restored memory blob failed to read')
        if seen_messages != counts['messages']:
            return fail('restored message reads differ from the manifest count')
        if seen_items != counts['memory_revision_items']:
            return fail('restored memory item reads differ from the manifest count')

        # Phase 4: seal the kept backup (encrypted) and report.
        archive_dir.mkdir(mode=0o700, parents=True)
        key_path = archive_dir / 'backup-key'
        key_path.write_bytes(os.urandom(32))
        os.chmod(key_path, 0o600)
        key = key_path.read_bytes()
        staging = work / 'stage'
        staging.mkdir(mode=0o700)
        shutil.copy2(dump_path, staging / 'metadata.sql')
        shutil.copy2(work / 'manifest.json', staging / 'manifest.json')
        shutil.copytree(object_dir, staging / 'objects')
        archive = staging / 'snapshot.tar'
        with tarfile.open(archive, 'w') as tar:
            for name in ('metadata.sql', 'manifest.json', 'objects'):
                tar.add(staging / name, arcname=name)
        node_js = ("const fs=require('node:fs'),c=require('node:crypto');"
                   "const a=process.argv.slice(1),key=fs.readFileSync(a[1]),"
                   "iv=c.randomBytes(12),cipher=c.createCipheriv('aes-256-gcm',key,iv);"
                   "cipher.setAAD(Buffer.from('WAYPOST-BACKUP-1'));"
                   "const out=Buffer.concat([Buffer.from('WPBACKUP1'),iv,"
                   "cipher.update(fs.readFileSync(a[0])),cipher.final(),cipher.getAuthTag()]);"
                   "fs.writeFileSync(a[2],out,{mode:0o600,flag:'wx'});")
        node = shutil.which('node') or shutil.which('node', path=env.get('PATH', ''))
        target = archive_dir / 'snapshot.tar.aesgcm'
        subprocess.run([node, '-e', node_js, str(archive), str(key_path), str(target)],
                       check=True, timeout=120)
        report = {'result': 'verified', 'production_writes': False,
                  'snapshot_boundary': manifest['snapshot_boundary'],
                  'allow_legacy_plaintext': allow_legacy,
                  'objects': len(index),
                  'object_bytes': sum(item['size_bytes'] for item in index),
                  'counts': counts, 'staging_dangling': staging_dangling,
                  'messages_read': seen_messages,
                  'attachments_verified': verified_attachment_bytes,
                  'attachments_binary_skipped': binary_attachments,
                  'memory_items_read': seen_items,
                  'reference_closure_verified': True,
                  'encrypted_archive_sha256': hashlib.sha256(target.read_bytes()).hexdigest()}
        (archive_dir / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        os.chmod(archive_dir / 'report.json', 0o600)
        print(json.dumps(report), flush=True)
        return 0
    finally:
        if server is not None and server.poll() is None:
            server.terminate()
            try:
                server.wait(timeout=10)
            except subprocess.TimeoutExpired:
                server.kill()
        if not args.keep_work_dir and work.exists():
            shutil.rmtree(work, ignore_errors=True)


if __name__ == '__main__':
    try:
        sys.exit(main())
    except subprocess.CalledProcessError as error:
        print(json.dumps({'result': 'failed', 'reason': f'subprocess exited {error.returncode}'}),
              flush=True)
        sys.exit(1)
    except Exception as error:
        print(json.dumps({'result': 'failed', 'reason': f'{type(error).__name__}: {error}'}),
              flush=True)
        sys.exit(1)
