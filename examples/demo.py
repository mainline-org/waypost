#!/usr/bin/env python3
"""Two reference assistants, private messaging, portable memory and recoverable consumption."""
import argparse
import hashlib
import json
import os
import uuid
import zipfile
from pathlib import Path
from consumer import Client, Consumer, export_memory


def setup(url, admin):
    control = Client(url, admin)
    prefix = str(uuid.uuid4())
    space = control.request('POST', '/admin/v0/spaces', {'request_id': prefix + '-space', 'display_name': 'Waypost complete trial'})
    sid = space['space_id']
    root = '/admin/v0/spaces/' + sid

    def assistant(name):
        actor = control.request('POST', root + '/actors', {'request_id': prefix + '-' + name, 'kind': 'agent', 'display_name': name + ' (reference demo)'})
        consumer = control.request('POST', root + '/consumers', {'request_id': prefix + '-' + name + '-consumer', 'actor_id': actor['actor_id'], 'view': 'inbox', 'filter': {}})
        return actor, consumer

    muse, muse_consumer = assistant('Muse')
    instict, instict_consumer = assistant('Instict')
    collection = control.request('POST', root + '/collections', {'request_id': prefix + '-collection', 'source_actor_id': muse['actor_id'], 'display_name': 'Portable decisions', 'scope': 'project/trial'})
    cid = collection['collection_id']

    def credential(key, actor, consumer, scopes, grants):
        token = control.request('POST', root + '/tokens', {'request_id': prefix + '-' + key, 'actor_id': actor, 'consumer_id': consumer, 'scopes': scopes, 'resource_grants': grants})
        return Client(url, token['token'])

    scopes = ['message.send', 'message.read', 'receipt.write', 'memory.read']
    source = credential('muse-token', muse['actor_id'], muse_consumer['consumer_id'], scopes + ['memory.write', 'blob.write'], {})
    target = credential('instict-token', instict['actor_id'], instict_consumer['consumer_id'], scopes, {'memory_read_collection_ids': [cid]})
    owner = credential('owner-token', space['default_actor_id'], space['default_consumer_id'], scopes, {'recipient_ids': [muse['actor_id'], instict['actor_id']], 'memory_read_collection_ids': [cid]})
    control.request('PUT', root + '/privacy', {'expected_revision': 0, 'enabled': True, 'mode': 'redact', 'all_agents': True, 'actor_ids': [], 'allow_uninspected_attachments': False, 'rules': [{'kind': 'email'}, {'kind': 'credential'}, {'kind': 'metadata_key', 'value': 'password'}]})
    return {'prefix': prefix, 'space': space, 'muse': muse, 'instict': instict, 'collection': collection, 'source': source, 'target': target, 'owner': owner}


def upload(client, key, content, filename='decision.md'):
    prepared = client.request('POST', '/v0/uploads', {'request_id': key, 'content_type': 'text/plain', 'size_bytes': len(content), 'sha256': hashlib.sha256(content).hexdigest(), 'filename': filename})
    client.request('PUT', '/v0/uploads/' + prepared['upload_id'] + '/content', content=content, content_type='text/plain')
    return client.request('POST', '/v0/uploads/' + prepared['upload_id'] + '/commit', {'request_id': 'seal-' + key})['blob_id']


def durable_send(consumer, body):
    with consumer.db:
        key = consumer.enqueue('message.send', '/v0/messages', body)
    return consumer.deliver('message.send', key)


def run(url, admin, directory):
    trial = setup(url, admin)
    prefix, cid = trial['prefix'], trial['collection']['collection_id']
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    source, target, owner = trial['source'], trial['target'], trial['owner']
    first_blob = upload(source, prefix + '-first', b'# Decision\n\nUse asynchronous durable mailboxes.\n')
    obsolete_blob = upload(source, prefix + '-obsolete', b'This item will be deleted explicitly.\n', 'obsolete.md')
    publish = f'/v0/memory/collections/{cid}/revisions'
    snapshot = {'request_id': prefix + '-snapshot', 'collection_id': cid, 'expected_revision': 0, 'mode': 'snapshot', 'items': [{'item_id': 'decision', 'blob_id': first_blob, 'path': 'decisions/first.md', 'metadata': {'origin': 'user_explicit'}}, {'item_id': 'obsolete', 'blob_id': obsolete_blob, 'path': 'obsolete.md'}], 'delete_ids': []}
    revision = source.request('POST', publish, snapshot)
    second_blob = upload(source, prefix + '-second', b'# Decision\n\nUse SQLite for durable consumer queues.\n')
    changed = source.request('POST', publish, {'request_id': prefix + '-delta', 'expected_revision': 1, 'mode': 'delta', 'items': [{'item_id': 'decision', 'blob_id': second_blob, 'path': 'decisions/first.md', 'metadata': {'origin': 'user_explicit'}}], 'delete_ids': ['obsolete']})
    assert changed['revision'] == 2
    assert source.request('POST', publish, snapshot) == revision
    source.request('POST', '/v0/messages', {'request_id': prefix + '-report', 'kind': 'result', 'subject': 'Research complete', 'body': {'text': 'The decision is ready. Contact alice@example.org.'}, 'metadata': {'password': 'demo-private-value'}})

    owner_path = directory / 'owner-consumer.sqlite'
    owner_consumer = Consumer(owner_path, owner)
    target_consumer = Consumer(directory / 'instict-consumer.sqlite', target)
    try:
        owner_consumer.sync()
        message = next(row for row in owner_consumer.db.execute("SELECT content FROM inbox WHERE binding=? AND item_type='message'", (owner_consumer.binding,)))
        text = message['content'].decode()
        assert 'alice@example.org' not in text and 'demo-private-value' not in text and '[REDACTED]' in text
        task = durable_send(owner_consumer, {'request_id': prefix + '-task', 'recipient_ids': [trial['instict']['actor_id']], 'kind': 'question', 'subject': 'Inspect the decision', 'body': {'text': 'Read the authorized portable decision and reply.'}})
        target_consumer.sync()
        saved = next(item for item in target_consumer.pending() if item['resource_id'] == task['message_id'])
        durable_send(target_consumer, {'request_id': prefix + '-reply', 'recipient_ids': [trial['space']['default_actor_id']], 'in_reply_to': task['message_id'], 'kind': 'result', 'subject': 'Decision inspected', 'body': {'text': 'Revision 2 was downloaded and durably saved; deleted items remain Tombstones.'}})
        target_consumer.processed(saved['item_id'])
        owner_consumer.sync()
        pending_before = owner_consumer.pending()
        owner_consumer.close()
        owner_consumer = Consumer(owner_path, owner)
        assert owner_consumer.pending() == pending_before
        exported = export_memory(owner, cid, directory / 'portable-memory.zip')
        with zipfile.ZipFile(exported['archive']) as bundle:
            assert bundle.read('items/decisions/first.md') == b'# Decision\n\nUse SQLite for durable consumer queues.\n'
            assert 'items/obsolete.md' not in bundle.namelist()
            assert 'memory.sqlite' in bundle.namelist()
        return {'result': 'passed', 'space_id': trial['space']['space_id'], 'dashboard': url.rstrip('/') + '/dashboard', 'memory_revision': 2, 'privacy': 'filtered', 'consumer_restart': 'pending inputs preserved', 'export': exported['archive'], 'note': 'Muse and Instict are reference demo identities; this does not install hooks into those products.'}
    finally:
        owner_consumer.close()
        target_consumer.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', default=os.environ.get('WAYPOST_URL', 'http://127.0.0.1:8787'))
    parser.add_argument('--output', default='var/demo')
    args = parser.parse_args()
    secret = os.environ.get('WAYPOST_ADMIN_SECRET')
    if not secret:
        secret_path = Path('var/admin-secret')
        if secret_path.exists():
            secret = secret_path.read_text().strip()
    if not secret:
        parser.error('Set WAYPOST_ADMIN_SECRET or initialize var/admin-secret.')
    print(json.dumps(run(args.url, secret, args.output), ensure_ascii=False, indent=2))

if __name__ == '__main__':
    main()
