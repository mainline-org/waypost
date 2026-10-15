"""Offline backup recovery verified through the native CLI and public HTTP API."""
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'examples'))
from consumer import ApiError, Client
from demo import setup, upload


class NativeService:
    def __init__(self, data):
        self.data = Path(data)
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', 0))
            port = listener.getsockname()[1]
        self.origin = f'http://127.0.0.1:{port}'
        target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
        self.binary = Path(os.environ.get('WAYPOST_TEST_BINARY', target / 'debug/waypost'))
        self.env = {key: value for key, value in os.environ.items()
                    if not key.startswith('WAYPOST_')}
        self.env['WAYPOST_PUBLIC_ORIGIN'] = self.origin
        self.command = [str(self.binary), 'serve', '--data', str(self.data),
                        '--listen', f'127.0.0.1:{port}']
        self.process = None

    def start(self):
        self.process = subprocess.Popen(self.command, env=self.env,
                                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(200):
            if self.process.poll() is not None:
                raise RuntimeError('Native recovery service exited before becoming ready.')
            try:
                Client(self.origin, '').request('GET', '/health')
                return self
            except OSError:
                time.sleep(.02)
        self.stop()
        raise RuntimeError('Native recovery service did not start.')

    def stop(self):
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()
            self.process.wait(timeout=5)


class OfflineBackupRecovery(unittest.TestCase):
    def backup(self, data, destination, secrets):
        return subprocess.run([sys.executable, str(ROOT / 'scripts/backup_native.py'),
                               '--data', str(data), '--output', str(destination),
                               '--secrets-output', str(secrets)],
                              env={key: value for key, value in os.environ.items()
                                   if not key.startswith('WAYPOST_')},
                              capture_output=True, text=True, timeout=15)

    def test_restore_preserves_messages_attachments_and_historical_memory_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = NativeService(root / 'original').start()
            self.addCleanup(original.stop)
            admin = (original.data / 'admin-secret').read_text().strip()
            trial = setup(original.origin, admin)
            source, owner = trial['source'], trial['owner']
            attachment_bytes = '报告附件：已确认餐厅预订。\n'.encode()
            attachment = upload(source, trial['prefix'] + '-attachment', attachment_bytes,
                                'booking.txt')
            sent = source.request('POST', '/v0/messages', {
                'request_id': trial['prefix'] + '-message', 'kind': 'result',
                'subject': '恢复后仍能阅读的报告', 'body': {'text': '预订已确认。'},
                'attachment_ids': [attachment]})
            message_path = '/v0/messages/' + sent['message_id']
            expected_message = owner.request('GET', message_path)
            collection = trial['collection']['collection_id']
            revision_path = f'/v0/memory/collections/{collection}/revisions'
            versions = [b'# Preference\nQuiet table.\n', b'# Preference\nWindow table.\n']
            for revision, content in enumerate(versions, 1):
                blob = upload(source, trial['prefix'] + f'-memory-{revision}', content)
                source.request('POST', revision_path, {
                    'request_id': trial['prefix'] + f'-revision-{revision}',
                    'expected_revision': revision - 1, 'mode': 'snapshot',
                    'items': [{'item_id': 'preference', 'blob_id': blob,
                               'path': 'preferences/table.md',
                               'metadata': {'origin': 'user_explicit'}}], 'delete_ids': []})
            rejected_live = self.backup(original.data, root / 'live-backup', root / 'live-secrets')
            self.assertNotEqual(rejected_live.returncode, 0)
            self.assertIn('Stop Waypost', rejected_live.stderr)
            self.assertFalse((root / 'live-backup').exists())
            self.assertFalse((root / 'live-secrets').exists())
            original.stop()

            backup, secrets = root / 'backup', root / 'separate-secrets'
            result = self.backup(original.data, backup, secrets)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse((backup / 'data/encryption-keys').exists())
            self.assertFalse((backup / 'data/admin-secret').exists())
            if os.name == 'posix':
                self.assertEqual(secrets.stat().st_mode & 0o777, 0o700)
                for path in secrets.iterdir():
                    self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            restored_data = root / 'different-location/restored'
            shutil.copytree(backup / 'data', restored_data)
            for filename in ('encryption-keys', 'admin-secret'):
                shutil.copy2(secrets / filename, restored_data / filename)
            restored = NativeService(restored_data).start()
            self.addCleanup(restored.stop)
            recovered_owner = Client(restored.origin, owner.token)
            self.assertEqual(recovered_owner.request('GET', message_path), expected_message)
            actual_attachment = recovered_owner.blob(expected_message['attachment_ids'][0])[1]
            self.assertEqual(actual_attachment, attachment_bytes)
            for revision, content in enumerate(versions, 1):
                page, items, blobs = recovered_owner.memory(collection, revision)
                self.assertEqual(page['revision'], revision)
                self.assertEqual(items[0]['path'], 'preferences/table.md')
                self.assertEqual(blobs[items[0]['blob_id']][1], content)
            restored.stop()

            (restored_data / 'encryption-keys').unlink()
            missing = NativeService(restored_data)
            rejected = subprocess.run(missing.command, env=missing.env, capture_output=True,
                                      text=True, timeout=5)
            self.assertNotEqual(rejected.returncode, 0)
            self.assertIn('Encryption keyring is missing', rejected.stderr)
            keyring = json.loads((secrets / 'encryption-keys').read_text())
            keyring['keys'] = {key: '22' * 32 for key in keyring['keys']}
            (restored_data / 'encryption-keys').write_text(json.dumps(keyring))
            (restored_data / 'encryption-keys').chmod(0o600)
            wrong_key = NativeService(restored_data).start()
            self.addCleanup(wrong_key.stop)
            with self.assertRaises(ApiError) as unreadable:
                Client(wrong_key.origin, owner.token).request('GET', message_path)
            self.assertEqual(unreadable.exception.code, 'INTERNAL_ERROR')
            with self.assertRaises(ApiError) as unreadable:
                Client(wrong_key.origin, owner.token).blob(expected_message['attachment_ids'][0])
            self.assertEqual(unreadable.exception.code, 'INTERNAL_ERROR')
            with self.assertRaises(ApiError) as unreadable:
                Client(wrong_key.origin, owner.token).memory(collection, 1)
            self.assertEqual(unreadable.exception.code, 'INTERNAL_ERROR')

    def test_backup_rejects_nested_secret_storage_and_preserves_existing_destinations(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = NativeService(root / 'original').start()
            original.stop()
            self.addCleanup(original.stop)
            backup = root / 'backup'
            rejected = self.backup(original.data, backup, backup / 'secrets')
            self.assertNotEqual(rejected.returncode, 0)
            self.assertIn('non-nested', rejected.stderr)
            self.assertFalse(backup.exists())
            backup.mkdir()
            marker = backup / 'keep.txt'
            marker.write_text('A previous backup must survive.')
            rejected = self.backup(original.data, backup, root / 'secrets')
            self.assertNotEqual(rejected.returncode, 0)
            self.assertEqual(marker.read_text(), 'A previous backup must survive.')
            self.assertFalse((root / 'secrets').exists())


if __name__ == '__main__':
    unittest.main()
