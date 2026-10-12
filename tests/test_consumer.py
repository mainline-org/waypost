import importlib.util
import json
import os
import socket
import sqlite3
import subprocess
import tempfile
import time
import unittest
import zipfile
from pathlib import Path
import sys
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'examples'))
from consumer import Client, Consumer, export_memory
from demo import setup, upload
ADMIN = 'test-only-admin-secret-32-bytes-long'

class LostAck(Client):
    def __init__(self, original):
        super().__init__(original.origin, original.token)
        self.lose = True
    def request(self, method, path, value=None, **kwargs):
        result = super().request(method, path, value, **kwargs)
        if self.lose and path == '/v0/receipts':
            self.lose = False
            raise OSError('Simulated lost committed response.')
        return result

class ConsumerContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.runtime = tempfile.TemporaryDirectory()
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        cls.url = f'http://127.0.0.1:{port}'
        env = dict(os.environ, WAYPOST_ADMIN_SECRET=ADMIN, WAYPOST_PUBLIC_ORIGIN=cls.url)
        cls.server = subprocess.Popen([str(ROOT / 'target/debug/waypost'), 'serve', '--data', cls.runtime.name, '--listen', f'127.0.0.1:{port}'], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(100):
            try:
                Client(cls.url, ADMIN).request('GET', '/health')
                return
            except OSError:
                time.sleep(.02)
        raise RuntimeError('Native test server did not start.')
    @classmethod
    def tearDownClass(cls):
        cls.server.terminate()
        cls.server.wait(timeout=5)
        cls.runtime.cleanup()

    def test_committed_ack_response_loss_preserves_pending_inputs_across_restart(self):
        trial = setup(self.url, ADMIN)
        source, owner = trial['source'], trial['owner']
        source.request('POST', '/v0/messages', {'request_id': trial['prefix'] + '-input', 'kind': 'info', 'subject': 'Durable input', 'body': {'text': 'Exact bytes of an unprocessed input.'}})
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'consumer.sqlite'
            consumer = Consumer(path, LostAck(owner))
            with self.assertRaises(OSError):
                consumer.sync()
            pending = consumer.pending()
            self.assertEqual(len(pending), 1)
            consumer.close()
            reopened = Consumer(path, owner)
            reopened.recover()
            self.assertEqual(reopened.pending(), pending)
            self.assertEqual(reopened.sync()['saved'], 0)
            self.assertEqual(reopened.pending(), pending)
            reopened.processed(pending[0]['item_id'])
            self.assertEqual(reopened.pending(), [])
            reopened.close()
        identity = source.request('GET', '/v0/describe')
        receipts = source.request('POST', '/v0/inbox/read', {'consumer_id': identity['consumer_id']})['items']
        self.assertEqual([r['data']['stage'] for r in receipts], ['received', 'processed'])

    def test_actor_outbox_survives_current_permission_loss_and_consumer_rotation(self):
        from consumer import ApiError
        trial=setup(self.url, ADMIN)
        owner=trial['owner'];control=Client(self.url,ADMIN);sid=trial['space']['space_id'];aid=trial['space']['default_actor_id']
        root='/admin/v0/spaces/'+sid
        read_only=control.request('POST',root+'/tokens',{'request_id':trial['prefix']+'-read-only','actor_id':aid,'consumer_id':trial['space']['default_consumer_id'],'scopes':['message.read'],'resource_grants':{}})
        next_consumer=control.request('POST',root+'/consumers',{'request_id':trial['prefix']+'-next-consumer','actor_id':aid,'view':'inbox','filter':{}})
        rotated=control.request('POST',root+'/tokens',{'request_id':trial['prefix']+'-rotated','actor_id':aid,'consumer_id':next_consumer['consumer_id'],'scopes':['message.send','message.read'],'resource_grants':{'recipient_ids':[trial['instict']['actor_id']]}})
        body={'request_id':trial['prefix']+'-queued','recipient_ids':[trial['instict']['actor_id']],'kind':'info','subject':'Resume original','body':{'text':'An actor-stable queued write.'}}
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'producer.sqlite'
            original=Consumer(path,owner)
            with original.db: original.enqueue('message.send','/v0/messages',body)
            original.close()
            limited=Consumer(path,Client(self.url,read_only['token']))
            with self.assertRaises(ApiError) as rejected: limited.deliver('message.send',body['request_id'])
            self.assertEqual(rejected.exception.code,'FORBIDDEN');limited.close()
            resumed=Consumer(path,Client(self.url,rotated['token']));resumed.recover();resumed.recover();resumed.close()
        target=trial['target'];identity=target.request('GET','/v0/describe')
        feed=target.request('POST','/v0/inbox/read',{'consumer_id':identity['consumer_id']})
        self.assertEqual(len(feed['items']),1)
        message=target.request('GET','/v0/messages/'+feed['items'][0]['resource_id'])
        self.assertEqual(message['body']['text'],body['body']['text'])

    def test_outbound_parameters_are_relational_and_replay_exactly_after_restart(self):
        trial = setup(self.url, ADMIN)
        body = {'request_id': trial['prefix'] + '-relational', 'kind': 'question', 'subject': 'A durable question', 'recipient_ids': [trial['instict']['actor_id']], 'body': {'text': 'Choose an option.', 'options': ['one', 'two']}, 'source': {'native_thread_id': None, 'url': 'https://example.org/context'}, 'attachment_ids': [], 'metadata': {'origin': 'explicit'}}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'consumer.sqlite'
            consumer = Consumer(path, trial['owner'])
            with consumer.db:
                consumer.enqueue('message.send', '/v0/messages', body)
            consumer.close()
            reopened = Consumer(path, trial['owner'])
            row = reopened.db.execute('SELECT * FROM outbound_requests').fetchone()
            self.assertEqual(row['body_text'], 'Choose an option.')
            self.assertNotIn('body', row.keys())
            recipients = reopened.db.execute("SELECT value FROM outbound_values WHERE field='recipient_ids' ORDER BY position").fetchall()
            self.assertEqual([r['value'] for r in recipients], body['recipient_ids'])
            self.assertEqual(reopened.outbound_body(row), body)
            with self.assertRaises(ValueError):
                with reopened.db:
                    reopened.enqueue('message.send', '/v0/messages', dict(body, subject='Different'))
            reopened.recover()
            self.assertEqual(reopened.db.execute('SELECT state FROM outbound_requests').fetchone()[0], 'committed')
            reopened.close()

    def test_memory_and_ack_links_are_queryable_without_decoding_an_archive(self):
        trial = setup(self.url, ADMIN)
        cid = trial['collection']['collection_id']
        blob = upload(trial['source'], trial['prefix'] + '-relational-note', b'A source note.')
        body = {'request_id': trial['prefix'] + '-relational-memory', 'expected_revision': 0, 'mode': 'snapshot', 'items': [{'item_id': 'note', 'blob_id': blob, 'path': 'notes/source.md', 'metadata': {'origin': 'explicit'}}], 'delete_ids': []}
        with tempfile.TemporaryDirectory() as directory:
            producer_path = Path(directory) / 'producer.sqlite'
            producer = Consumer(producer_path, trial['source'])
            with producer.db:
                producer.enqueue('memory.publish', '/v0/memory/collections/' + cid + '/revisions', body)
            producer.close()
            producer = Consumer(producer_path, trial['source'])
            row = producer.db.execute('SELECT * FROM outbound_requests').fetchone()
            self.assertEqual(producer.outbound_body(row), body)
            producer.recover()
            owner = Consumer(Path(directory) / 'owner.sqlite', trial['owner'])
            self.assertEqual(owner.sync()['saved'], 1)
            self.assertEqual(owner.db.execute('SELECT blob_id,path FROM local_memory_items').fetchone()['blob_id'], blob)
            self.assertEqual(owner.db.execute('SELECT revision FROM local_memory_versions').fetchone()[0], 1)
            self.assertEqual(owner.db.execute("SELECT stage FROM outbound_values WHERE field='receipt.items'").fetchone()[0], 'received')
            producer.sync()
            self.assertEqual(producer.db.execute('SELECT stage,version FROM local_receipt_updates').fetchone()['stage'], 'received')
            owner.close()
            producer.close()

    def test_pre_release_json_queue_migration_preserves_the_original_pending_request(self):
        trial = setup(self.url, ADMIN)
        body = {'request_id': trial['prefix'] + '-legacy', 'kind': 'info', 'subject': 'Queued before upgrade', 'body': {'text': 'Resume the original request.'}}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'consumer.sqlite'
            before = Consumer(path, trial['source'])
            binding = before.producer_binding
            before.close()
            with sqlite3.connect(path) as db:
                db.execute('CREATE TABLE outbox(binding TEXT,operation TEXT,request_id TEXT,path TEXT,body BLOB,state TEXT,result BLOB)')
                db.execute('INSERT INTO outbox VALUES(?,?,?,?,?,?,NULL)', (binding, 'message.send', body['request_id'], '/v0/messages', json.dumps(body).encode(), 'unknown'))
            upgraded = Consumer(path, trial['source'])
            row = upgraded.db.execute('SELECT * FROM outbound_requests').fetchone()
            self.assertEqual(upgraded.outbound_body(row), body)
            self.assertEqual(row['state'], 'unknown')
            upgraded.recover()
            self.assertEqual(upgraded.db.execute('SELECT state FROM outbound_requests').fetchone()[0], 'committed')
            self.assertIsNone(upgraded.db.execute("SELECT name FROM sqlite_master WHERE name='outbox'").fetchone())
            upgraded.close()

    def test_export_downloads_exact_historical_bytes_and_excludes_connection_credentials(self):
        trial = setup(self.url, ADMIN)
        source, owner = trial['source'], trial['owner']
        cid = trial['collection']['collection_id']
        content = b'# Original decision\n\nPreserve exact source bytes.\n'
        blob = upload(source, trial['prefix'] + '-note', content)
        path = '/v0/memory/collections/' + cid + '/revisions'
        source.request('POST', path, {'request_id': trial['prefix'] + '-snapshot', 'expected_revision': 0, 'mode': 'snapshot', 'items': [{'item_id': 'note', 'blob_id': blob, 'path': 'notes/decision.md', 'metadata': {'origin': 'user_explicit', 'access_token': 'transport-secret-do-not-export'}}]})
        source.request('POST', path, {'request_id': trial['prefix'] + '-delete', 'expected_revision': 1, 'mode': 'delta', 'delete_ids': ['note']})
        with tempfile.TemporaryDirectory() as directory:
            result = export_memory(owner, cid, Path(directory) / 'memory.zip', revision=1)
            self.assertEqual(result['revision'], 1)
            with zipfile.ZipFile(result['archive']) as bundle:
                self.assertEqual(bundle.read('items/notes/decision.md'), content)
                database = Path(directory) / 'memory.sqlite'
                database.write_bytes(bundle.read('memory.sqlite'))
            with sqlite3.connect(database) as db:
                self.assertEqual(db.execute('SELECT revision FROM collection').fetchone()[0], 1)
                self.assertEqual(json.loads(db.execute('SELECT metadata FROM items').fetchone()[0]), {'origin': 'user_explicit'})
                self.assertEqual(db.execute('SELECT content FROM blobs').fetchone()[0], content)

if __name__ == '__main__':
    unittest.main()
