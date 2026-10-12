#!/usr/bin/env python3
"""Reference consumer: SQLite first, ACK second. No model or scheduler is implied."""
import argparse
import hashlib
import io
import json
import os
import re
import sqlite3
import tempfile
import urllib.error
import urllib.request
import uuid
import zipfile
from pathlib import Path
from local_store import create_tables, save_outbound, outbound_body, save_inbound


class ApiError(Exception):
    def __init__(self, value):
        self.value = value
        self.code = value.get('code', 'UNKNOWN')
        super().__init__(value.get('message', self.code))


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


class Client:
    def __init__(self, origin, token):
        if not origin.startswith(('http://', 'https://')):
            raise ValueError('Expected an HTTP(S) origin.')
        self.origin = origin.rstrip('/')
        self.token = token
        self.opener = urllib.request.build_opener(NoRedirect())

    def request(self, method, path, value=None, content=None, content_type='application/json'):
        if not path.startswith('/') or path.startswith('//'):
            raise ValueError('Only service-relative paths are accepted.')
        data = content if content is not None else (json.dumps(value).encode() if value is not None else None)
        req = urllib.request.Request(self.origin + path, data=data, method=method,
            headers={'Authorization': 'Bearer ' + self.token, 'Content-Type': content_type})
        try:
            with self.opener.open(req, timeout=20) as response:
                raw = response.read()
        except urllib.error.HTTPError as error:
            try:
                value = json.loads(error.read())['error']
            except (ValueError, KeyError):
                value = {'code': 'HTTP_ERROR', 'message': f'HTTP {error.code}'}
            error.close()
            raise ApiError(value) from error
        if path.endswith('/content') and method == 'GET':
            return raw
        result = json.loads(raw)
        if not result['ok']:
            raise ApiError(result['error'])
        return result['data']

    def blob(self, blob_id):
        check_id(blob_id)
        metadata = self.request('GET', '/v0/blobs/' + blob_id)
        content = self.request('GET', '/v0/blobs/' + blob_id + '/content')
        if len(content) != metadata['size_bytes'] or hashlib.sha256(content).hexdigest() != metadata['sha256']:
            raise ValueError('Downloaded Blob does not match its sealed metadata.')
        return metadata, content

    def memory(self, collection_id, revision=None):
        check_id(collection_id)
        cursor, items, blobs = None, [], {}
        actual = revision
        while True:
            page = self.request('POST', f'/v0/memory/collections/{collection_id}/read',
                {'revision': actual, 'cursor': cursor, 'include_tombstones': True, 'limit': 10})
            if actual is None:
                actual = page['revision']
            if page['revision'] != actual:
                raise ValueError('Memory pagination mixed revisions.')
            items.extend(page['items'])
            for item in page['items']:
                safe_path(item['path'])
                if item['blob_id'] not in blobs:
                    blobs[item['blob_id']] = self.blob(item['blob_id'])
            if not page['has_more']:
                return page, items, blobs
            cursor = page['next_cursor']


def check_id(value):
    if not isinstance(value, str) or not re.fullmatch(r'[A-Za-z0-9_-]{1,96}', value):
        raise ValueError('Invalid resource ID.')


def safe_path(value):
    if not value or '\\' in value or ':' in value or any(c in value for c in '\x00\r\n') or any(p in ('', '.', '..') for p in value.split('/')):
        raise ValueError('Unsafe export path.')
    return value


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()


class Consumer:
    def __init__(self, path, client):
        self.client = client
        path = Path(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        self.db = sqlite3.connect(path)
        self.db.row_factory = sqlite3.Row
        self.db.execute('PRAGMA synchronous=FULL')
        self.db.execute('PRAGMA foreign_keys=ON')
        self.db.executescript('''
            CREATE TABLE IF NOT EXISTS cursors(binding TEXT PRIMARY KEY,cursor TEXT);
            CREATE TABLE IF NOT EXISTS inbox(binding TEXT NOT NULL,item_id TEXT NOT NULL,item_type TEXT NOT NULL,resource_id TEXT NOT NULL,resource_version INTEGER NOT NULL,content BLOB NOT NULL,stage TEXT NOT NULL DEFAULT 'received',PRIMARY KEY(binding,item_id));
            CREATE TABLE IF NOT EXISTS blobs(blob_id TEXT PRIMARY KEY,content_type TEXT NOT NULL,sha256 TEXT NOT NULL,content BLOB NOT NULL);
        ''')
        create_tables(self.db)
        identity = client.request('GET', '/v0/describe')
        self.identity = identity
        self.binding = hashlib.sha256(encoded({'origin': client.origin, 'space': identity['space_id'], 'actor': identity['actor_id'], 'consumer': identity['consumer_id'], 'view': identity['view_revision'], 'scopes': identity['scopes'], 'grants': identity['resource_grants']})).hexdigest()
        self.producer_binding = hashlib.sha256(encoded({'origin': client.origin, 'space': identity['space_id'], 'actor': identity['actor_id']})).hexdigest()

    def close(self):
        self.db.close()

    def enqueue(self, operation, path, body):
        binding = self.binding if operation == 'receipt.ack' else self.producer_binding
        return save_outbound(self.db, binding, operation, path, body)

    def outbound_body(self, row):
        return outbound_body(self.db, row)

    def deliver(self, operation, key):
        binding = self.binding if operation == 'receipt.ack' else self.producer_binding
        row = self.db.execute('SELECT * FROM outbound_requests WHERE binding=? AND operation=? AND request_id=?', (binding, operation, key)).fetchone()
        if row['state'] == 'committed':
            return json.loads(row['result'])
        try:
            result = self.client.request('POST', row['path'], self.outbound_body(row))
        except ApiError as error:
            state = 'unknown' if error.value.get('retryable') or error.code in ('UNAUTHENTICATED', 'FORBIDDEN') else 'failed'
            with self.db:
                self.db.execute('UPDATE outbound_requests SET state=?,result=? WHERE binding=? AND operation=? AND request_id=?', (state, encoded(error.value), binding, operation, key))
            raise
        except (OSError, ValueError):
            with self.db:
                self.db.execute("UPDATE outbound_requests SET state='unknown' WHERE binding=? AND operation=? AND request_id=?", (binding, operation, key))
            raise
        with self.db:
            self.db.execute("UPDATE outbound_requests SET state='committed',result=? WHERE binding=? AND operation=? AND request_id=?", (encoded(result), binding, operation, key))
        return result

    def recover(self):
        rows = self.db.execute("SELECT * FROM outbound_requests WHERE binding IN(?,?) AND state IN('queued','unknown') ORDER BY rowid", (self.binding, self.producer_binding)).fetchall()
        for row in rows:
            try:
                status = self.client.request('POST', '/v0/submissions/status', {'operation': row['operation'], 'request_id': row['request_id']})
                if status['submission_state'] in ('committed', 'failed'):
                    with self.db:
                        self.db.execute('UPDATE outbound_requests SET state=?,result=? WHERE binding=? AND operation=? AND request_id=?', (status['submission_state'], encoded(status['result'] if status['submission_state'] == 'committed' else status['error']), row['binding'], row['operation'], row['request_id']))
                    continue
            except ApiError as error:
                if error.code != 'NOT_FOUND':
                    raise
            self.deliver(row['operation'], row['request_id'])

    def sync(self):
        self.recover()
        row = self.db.execute('SELECT cursor FROM cursors WHERE binding=?', (self.binding,)).fetchone()
        page = self.client.request('POST', '/v0/inbox/read', {'consumer_id': self.identity['consumer_id'], 'cursor': row['cursor'] if row else None, 'limit': min(20, self.identity['limits']['max_page_size'])})
        contents, blobs = [], {}
        for item in page['items']:
            if item['item_type'] == 'message':
                content = self.client.request('GET', '/v0/messages/' + item['resource_id'])
                for blob_id in content['attachment_ids']:
                    blobs[blob_id] = self.client.blob(blob_id)
            elif item['item_type'] == 'memory_revision':
                memory, items, memory_blobs = self.client.memory(item['resource_id'], item['resource_version'])
                content = {'collection': memory, 'items': items}
                blobs.update(memory_blobs)
            elif item['item_type'] == 'receipt':
                content = item['data']
            else:
                raise ValueError('Unknown Feed type; checkpoint was not advanced.')
            contents.append((item, content))
        # The complete page, exact content/bytes, and pending ACK are one local commit.
        with self.db:
            for item, content in contents:
                self.db.execute('INSERT OR IGNORE INTO inbox(binding,item_id,item_type,resource_id,resource_version,content) VALUES(?,?,?,?,?,?)', (self.binding, item['item_id'], item['item_type'], item['resource_id'], item['resource_version'], encoded(content)))
                save_inbound(self.db, self.binding, item, content)
            for blob_id, (metadata, content) in blobs.items():
                self.db.execute('INSERT OR IGNORE INTO blobs(blob_id,content_type,sha256,content) VALUES(?,?,?,?)', (blob_id, metadata['content_type'], metadata['sha256'], content))
            self.db.execute('INSERT INTO cursors(binding,cursor) VALUES(?,?) ON CONFLICT(binding) DO UPDATE SET cursor=excluded.cursor', (self.binding, page['next_cursor']))
            key = self.enqueue('receipt.ack', '/v0/receipts', {'request_id': 'ack-' + str(uuid.uuid4()), 'consumer_id': self.identity['consumer_id'], 'items': [{'item_id': item['item_id'], 'stage': 'received'} for item, _ in contents], 'checkpoint_cursor': page['next_cursor']})
        self.deliver('receipt.ack', key)
        return {'saved': len(contents), 'has_more': page['has_more']}

    def pending(self):
        return [dict(row) for row in self.db.execute("SELECT item_id,item_type,resource_id,resource_version,stage FROM inbox WHERE binding=? AND stage<>'processed' ORDER BY rowid", (self.binding,))]

    def processed(self, item_id):
        check_id(item_id)
        with self.db:
            row = self.db.execute('SELECT 1 FROM inbox WHERE binding=? AND item_id=?', (self.binding, item_id)).fetchone()
            if not row:
                raise ValueError('Input was not durably saved by this Consumer.')
            self.db.execute("UPDATE inbox SET stage='processed' WHERE binding=? AND item_id=?", (self.binding, item_id))
            key = self.enqueue('receipt.ack', '/v0/receipts', {'request_id': 'processed-' + str(uuid.uuid4()), 'consumer_id': self.identity['consumer_id'], 'items': [{'item_id': item_id, 'stage': 'processed'}]})
        return self.deliver('receipt.ack', key)


SECRET_KEYS = {'authorization', 'access_token', 'refresh_token', 'api_key', 'password', 'secret', 'token', 'admin_secret'}

def export_metadata(value):
    if isinstance(value, dict):
        return {key: export_metadata(item) for key, item in value.items() if key.lower() not in SECRET_KEYS}
    if isinstance(value, list):
        return [export_metadata(item) for item in value]
    return value


def export_memory(client, collection_id, destination, revision=None):
    page, items, blobs = client.memory(collection_id, revision)
    destination = Path(destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as temporary:
        database = Path(temporary) / 'memory.sqlite'
        db = sqlite3.connect(database)
        db.executescript('CREATE TABLE collection(collection_id TEXT,source_actor_id TEXT,scope TEXT,revision INTEGER);CREATE TABLE items(item_id TEXT PRIMARY KEY,blob_id TEXT,path TEXT,metadata TEXT,deleted_in_revision INTEGER);CREATE TABLE blobs(blob_id TEXT PRIMARY KEY,content_type TEXT,sha256 TEXT,content BLOB);')
        with db:
            db.execute('INSERT INTO collection VALUES(?,?,?,?)', (collection_id, page['source_actor_id'], page['scope'], page['revision']))
            for item in items:
                db.execute('INSERT INTO items VALUES(?,?,?,?,?)', (item['item_id'], item['blob_id'], safe_path(item['path']), json.dumps(export_metadata(item['metadata']), ensure_ascii=False), item.get('deleted_in_revision')))
            for blob_id, (metadata, content) in blobs.items():
                db.execute('INSERT INTO blobs VALUES(?,?,?,?)', (blob_id, metadata['content_type'], metadata['sha256'], content))
        db.close()
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, 'w', zipfile.ZIP_DEFLATED) as bundle:
            bundle.write(database, 'memory.sqlite')
            bundle.writestr('README.md', f'# Portable memory\n\nCollection: {collection_id}\nSource: {page["source_actor_id"]}\nScope: {page["scope"]}\nRevision: {page["revision"]}\n\nActive item files are in `items/`. Complete items, Tombstones, provenance metadata and sealed bytes are in `memory.sqlite`. Connection credentials are excluded.\n')
            for item in items:
                if item.get('deleted_in_revision') is None:
                    bundle.writestr('items/' + safe_path(item['path']), blobs[item['blob_id']][1])
        temporary_output = destination.with_name(destination.name + '.' + uuid.uuid4().hex + '.tmp')
        with open(temporary_output, 'xb') as output:
            output.write(buffer.getvalue())
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary_output, destination)
    return {'collection_id': collection_id, 'revision': page['revision'], 'items': len(items), 'archive': str(destination)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['sync', 'pending', 'processed', 'export'])
    parser.add_argument('--url', default=os.environ.get('WAYPOST_URL', 'http://127.0.0.1:8787'))
    parser.add_argument('--db', default='var/consumer.sqlite')
    parser.add_argument('--item')
    parser.add_argument('--collection')
    parser.add_argument('--revision', type=int)
    parser.add_argument('--output', default='var/memory.zip')
    args = parser.parse_args()
    token = os.environ.get('WAYPOST_TOKEN')
    if not token:
        parser.error('Set WAYPOST_TOKEN to a private Agent/Consumer credential.')
    client = Client(args.url, token)
    if args.command == 'export':
        if not args.collection:
            parser.error('--collection is required')
        result = export_memory(client, args.collection, args.output, args.revision)
    else:
        consumer = Consumer(args.db, client)
        try:
            result = consumer.sync() if args.command == 'sync' else consumer.pending() if args.command == 'pending' else consumer.processed(args.item)
        finally:
            consumer.close()
    print(json.dumps(result, ensure_ascii=False, indent=2))

if __name__ == '__main__':
    main()
