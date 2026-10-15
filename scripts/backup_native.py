#!/usr/bin/env python3
"""Create a stopped native Waypost snapshot, keeping credentials in a separate directory."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import sys


def overlaps(first, second):
    return first == second or first in second.parents or second in first.parents


def copy_private(source, destination):
    if not stat.S_ISREG(source.lstat().st_mode):
        raise ValueError('Backup inputs must be regular files, without symbolic links.')
    destination.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    with source.open('rb') as incoming, destination.open('xb') as outgoing:
        os.chmod(destination, 0o600)
        shutil.copyfileobj(incoming, outgoing)
        outgoing.flush()
        os.fsync(outgoing.fileno())


def backup(args):
    data = args.data.resolve(strict=True)
    output = args.output.resolve()
    secrets_output = args.secrets_output.resolve()
    if any(overlaps(first, second) for first, second in
           ((data, output), (data, secrets_output), (output, secrets_output))):
        raise ValueError('Data, backup and secret directories must be separate, non-nested paths.')
    if output.exists() or secrets_output.exists():
        raise ValueError('Backup and secret destinations must not already exist.')
    secrets = {}
    for name, option, environment in (
            ('encryption-keys', args.encryption_key_file, 'WAYPOST_ENCRYPTION_KEYS'),
            ('admin-secret', args.admin_secret_file, 'WAYPOST_ADMIN_SECRET'),
            ('cursor-secret', args.cursor_secret_file, 'WAYPOST_CURSOR_SECRET')):
        if environment in os.environ and option is None:
            raise ValueError(f'{environment} is configured; supply its current value as a private '
                             'file with the corresponding --*-file option.')
        source = option if option is not None else data / name
        if name == 'cursor-secret' and option is None:
            continue
        if not source.is_file():
            raise ValueError(f'Missing {name}; provide the current private file explicitly.')
        secrets[name] = source

    created = []
    with (data / 'runtime-lock').open('a+b') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise ValueError('Stop Waypost before taking a backup; the data directory is in use.') from error
        if (data / 'objects').is_symlink():
            raise ValueError('Object backup does not accept symbolic links.')
        if not (data / 'metadata.sqlite').is_file() or not (data / 'objects/objects.sqlite').is_file():
            raise ValueError('Expected an initialized native data directory with both SQLite stores.')
        files = [data / 'metadata.sqlite'] + sorted(data.glob('metadata.sqlite-*'))
        for path in sorted((data / 'objects').rglob('*')):
            if path.is_symlink():
                raise ValueError('Object backup does not accept symbolic links.')
            if not path.is_dir():
                files.append(path)
        try:
            for directory in (output, secrets_output):
                directory.mkdir(mode=0o700, parents=True)
                created.append(directory)
            manifest = {'format': 'waypost-native-offline-v1', 'files': {}}
            for source in files:
                relative = source.relative_to(data)
                target = output / 'data' / relative
                copy_private(source, target)
                digest = hashlib.sha256()
                with target.open('rb') as file:
                    for chunk in iter(lambda: file.read(1024 * 1024), b''):
                        digest.update(chunk)
                manifest['files'][relative.as_posix()] = {
                    'size_bytes': target.stat().st_size, 'sha256': digest.hexdigest()}
            for name, source in secrets.items():
                copy_private(source, secrets_output / name)
            manifest_path = output / 'manifest.json'
            with manifest_path.open('x') as file:
                os.chmod(manifest_path, 0o600)
                json.dump(manifest, file, indent=2)
                file.write('\n')
                file.flush()
                os.fsync(file.fileno())
            for directory in (output / 'data/objects', output / 'data', output, secrets_output):
                descriptor = os.open(directory, os.O_RDONLY)
                try:
                    os.fsync(descriptor)
                finally:
                    os.close(descriptor)
        except Exception:
            for directory in reversed(created):
                shutil.rmtree(directory)
            raise
    return {'ok': True, 'files': len(files), 'secrets': sorted(secrets),
            'note': 'Keep both directories separately. Test recovery before relying on this backup.'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--secrets-output', required=True, type=Path)
    parser.add_argument('--encryption-key-file', type=Path,
                        help='Current keyring when provided outside the data directory.')
    parser.add_argument('--admin-secret-file', type=Path,
                        help='Current administrator credential when provided externally.')
    parser.add_argument('--cursor-secret-file', type=Path,
                        help='Current independent cursor secret, if configured.')
    args = parser.parse_args()
    try:
        result = backup(args)
    except (OSError, ValueError) as error:
        print(f'Backup failed: {error}', file=sys.stderr)
        return 1
    print(json.dumps(result))
    return 0


if __name__ == '__main__':
    sys.exit(main())
