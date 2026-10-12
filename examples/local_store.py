"""Relational replay parameters and inspectable local inputs. No network calls."""
import hashlib
import json

SCALARS = ('kind', 'subject', 'thread_id', 'in_reply_to', 'consumer_id',
           'checkpoint_cursor', 'collection_id', 'expected_revision', 'mode',
           'manifest_blob_id', 'content_type', 'size_bytes', 'sha256', 'filename')
FIELDS = SCALARS + ('recipient_ids', 'attachment_ids', 'body', 'metadata', 'source', 'items', 'delete_ids')
SOURCE_FIELDS = ('native_thread_id', 'native_message_id', 'url')
KEY = 'binding,operation,request_id'
OPERATIONS = {
    'message.send': {'request_id', 'kind', 'subject', 'thread_id', 'in_reply_to', 'recipient_ids', 'attachment_ids', 'body', 'metadata', 'source'},
    'receipt.ack': {'request_id', 'consumer_id', 'items', 'checkpoint_cursor'},
    'memory.publish': {'request_id', 'collection_id', 'expected_revision', 'mode', 'items', 'delete_ids', 'manifest_blob_id'},
    'blob.prepare': {'request_id', 'content_type', 'size_bytes', 'sha256', 'filename'},
    'blob.commit': {'request_id'},
}


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()


def mask(fields, value):
    return sum(1 << i for i, field in enumerate(fields) if field in value)


def present(bits, fields):
    return [field for i, field in enumerate(fields) if bits & (1 << i)]


def create_tables(db):
    db.executescript('''
        CREATE TABLE IF NOT EXISTS outbound_requests(
            binding TEXT NOT NULL,operation TEXT NOT NULL,request_id TEXT NOT NULL,path TEXT NOT NULL,
            fingerprint BLOB NOT NULL,fields INTEGER NOT NULL,null_fields INTEGER NOT NULL,
            kind TEXT,subject TEXT,thread_id TEXT,in_reply_to TEXT,consumer_id TEXT,checkpoint_cursor TEXT,
            collection_id TEXT,expected_revision INTEGER,mode TEXT,manifest_blob_id TEXT,
            content_type TEXT,size_bytes INTEGER,sha256 TEXT,filename TEXT,
            body_text TEXT,body_fields INTEGER NOT NULL,source_fields INTEGER NOT NULL,
            source_thread_id TEXT,source_message_id TEXT,source_url TEXT,metadata TEXT,
            state TEXT NOT NULL,result BLOB,PRIMARY KEY(binding,operation,request_id));
        CREATE TABLE IF NOT EXISTS outbound_values(
            binding TEXT NOT NULL,operation TEXT NOT NULL,request_id TEXT NOT NULL,field TEXT NOT NULL,
            position INTEGER NOT NULL,value TEXT NOT NULL,stage TEXT,
            PRIMARY KEY(binding,operation,request_id,field,position),
            FOREIGN KEY(binding,operation,request_id) REFERENCES outbound_requests(binding,operation,request_id));
        CREATE TABLE IF NOT EXISTS outbound_memory_items(
            binding TEXT NOT NULL,operation TEXT NOT NULL,request_id TEXT NOT NULL,position INTEGER NOT NULL,
            item_id TEXT NOT NULL,blob_id TEXT NOT NULL,path TEXT NOT NULL,metadata TEXT,metadata_present INTEGER NOT NULL,
            PRIMARY KEY(binding,operation,request_id,position),
            FOREIGN KEY(binding,operation,request_id) REFERENCES outbound_requests(binding,operation,request_id));
        CREATE TABLE IF NOT EXISTS local_messages(
            binding TEXT NOT NULL,item_id TEXT NOT NULL,message_id TEXT NOT NULL,producer_id TEXT NOT NULL,
            thread_id TEXT NOT NULL,in_reply_to TEXT,kind TEXT NOT NULL,subject TEXT NOT NULL,body_text TEXT NOT NULL,
            committed_at TEXT,source_thread_id TEXT,source_message_id TEXT,source_url TEXT,metadata TEXT NOT NULL,
            PRIMARY KEY(binding,item_id),FOREIGN KEY(binding,item_id) REFERENCES inbox(binding,item_id));
        CREATE TABLE IF NOT EXISTS local_message_values(
            binding TEXT NOT NULL,item_id TEXT NOT NULL,field TEXT NOT NULL,position INTEGER NOT NULL,value TEXT NOT NULL,
            PRIMARY KEY(binding,item_id,field,position),FOREIGN KEY(binding,item_id) REFERENCES inbox(binding,item_id));
        CREATE TABLE IF NOT EXISTS local_memory_versions(
            binding TEXT NOT NULL,item_id TEXT NOT NULL,collection_id TEXT NOT NULL,revision INTEGER NOT NULL,
            source_actor_id TEXT NOT NULL,scope TEXT NOT NULL,PRIMARY KEY(binding,item_id),
            FOREIGN KEY(binding,item_id) REFERENCES inbox(binding,item_id));
        CREATE TABLE IF NOT EXISTS local_memory_items(
            binding TEXT NOT NULL,item_id TEXT NOT NULL,memory_item_id TEXT NOT NULL,blob_id TEXT NOT NULL,
            path TEXT NOT NULL,metadata TEXT NOT NULL,deleted_in_revision INTEGER,
            PRIMARY KEY(binding,item_id,memory_item_id),FOREIGN KEY(binding,item_id) REFERENCES inbox(binding,item_id));
        CREATE TABLE IF NOT EXISTS local_receipt_updates(
            binding TEXT NOT NULL,item_id TEXT NOT NULL,consumer_id TEXT NOT NULL,subject_item_id TEXT NOT NULL,
            stage TEXT NOT NULL,version INTEGER NOT NULL,PRIMARY KEY(binding,item_id),
            FOREIGN KEY(binding,item_id) REFERENCES inbox(binding,item_id));
    ''')
    # Upgrade the pre-release JSON queue in one transaction; never discard pending work.
    if db.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='outbox'").fetchone():
        with db:
            for row in db.execute('SELECT * FROM outbox').fetchall():
                save_outbound(db, row['binding'], row['operation'], row['path'], json.loads(row['body']))
                db.execute('UPDATE outbound_requests SET state=?,result=? WHERE binding=? AND operation=? AND request_id=?',
                           (row['state'], row['result'], row['binding'], row['operation'], row['request_id']))
            db.execute('DROP TABLE outbox')
    # Existing immutable input archives can also be indexed without moving their checkpoint.
    if db.execute('PRAGMA user_version').fetchone()[0] < 1:
        with db:
            for row in db.execute('SELECT * FROM inbox').fetchall():
                save_inbound(db, row['binding'], row, json.loads(row['content']))
            db.execute('PRAGMA user_version=1')


def save_outbound(db, binding, operation, path, body):
    if operation not in OPERATIONS or set(body) - OPERATIONS[operation]:
        raise ValueError('Unsupported outbound operation or fields.')
    key = body['request_id']
    fingerprint = hashlib.sha256(encoded(body)).digest()
    previous = db.execute('SELECT fingerprint,path FROM outbound_requests WHERE binding=? AND operation=? AND request_id=?', (binding, operation, key)).fetchone()
    if previous:
        if previous['fingerprint'] != fingerprint or previous['path'] != path:
            raise ValueError('Local request ID was already used with different content.')
        return key
    text = body.get('body') or {}
    source = body.get('source') or {}
    if set(text) - {'text', 'options'} or set(source) - set(SOURCE_FIELDS):
        raise ValueError('Unsupported message body or source fields.')
    values = {
        'binding': binding, 'operation': operation, 'request_id': key, 'path': path,
        'fingerprint': fingerprint, 'fields': mask(FIELDS, body),
        'null_fields': mask(FIELDS, {k: v for k, v in body.items() if v is None}),
        **{field: body.get(field) for field in SCALARS},
        'body_text': text.get('text'), 'body_fields': mask(('text', 'options'), text),
        'source_fields': mask(SOURCE_FIELDS, source), 'source_thread_id': source.get('native_thread_id'),
        'source_message_id': source.get('native_message_id'), 'source_url': source.get('url'),
        'metadata': encoded(body.get('metadata')).decode(), 'state': 'queued',
    }
    columns = ','.join(values)
    db.execute(f'INSERT INTO outbound_requests({columns}) VALUES({",".join("?" for _ in values)})', tuple(values.values()))
    for field, entries in [('recipient_ids', body.get('recipient_ids')), ('attachment_ids', body.get('attachment_ids')),
                           ('body.options', text.get('options')), ('delete_ids', body.get('delete_ids')),
                           ('receipt.items', body.get('items') if operation == 'receipt.ack' else None)]:
        for position, entry in enumerate(entries or []):
            value, stage = (entry['item_id'], entry['stage']) if field == 'receipt.items' else (entry, None)
            if isinstance(entry, dict) and set(entry) != {'item_id', 'stage'}:
                raise ValueError('Unsupported ACK fields.')
            db.execute('INSERT INTO outbound_values VALUES(?,?,?,?,?,?,?)', (binding, operation, key, field, position, value, stage))
    if operation == 'memory.publish':
        for position, entry in enumerate(body.get('items') or []):
            if set(entry) - {'item_id', 'blob_id', 'path', 'metadata'}:
                raise ValueError('Unsupported memory item fields.')
            db.execute('INSERT INTO outbound_memory_items VALUES(?,?,?,?,?,?,?,?,?)',
                       (binding, operation, key, position, entry['item_id'], entry['blob_id'], entry['path'], encoded(entry.get('metadata')).decode(), int('metadata' in entry)))
    return key


def outbound_body(db, row):
    fields = present(row['fields'], FIELDS)
    body = {'request_id': row['request_id'], **{field: row[field] for field in fields if field in SCALARS}}
    identity = (row['binding'], row['operation'], row['request_id'])
    children = db.execute('SELECT field,value,stage FROM outbound_values WHERE binding=? AND operation=? AND request_id=? ORDER BY field,position', identity).fetchall()
    for field in ('recipient_ids', 'attachment_ids', 'delete_ids'):
        if field in fields:
            body[field] = [r['value'] for r in children if r['field'] == field]
    if 'body' in fields:
        body['body'] = {}
        if row['body_fields'] & 1:
            body['body']['text'] = row['body_text']
        if row['body_fields'] & 2:
            body['body']['options'] = [r['value'] for r in children if r['field'] == 'body.options']
    if 'source' in fields:
        values = (row['source_thread_id'], row['source_message_id'], row['source_url'])
        body['source'] = {f: v for f, v in zip(SOURCE_FIELDS, values) if f in present(row['source_fields'], SOURCE_FIELDS)}
    if 'metadata' in fields:
        body['metadata'] = json.loads(row['metadata'])
    if 'items' in fields:
        if row['operation'] == 'receipt.ack':
            body['items'] = [{'item_id': r['value'], 'stage': r['stage']} for r in children if r['field'] == 'receipt.items']
        else:
            entries = db.execute('SELECT * FROM outbound_memory_items WHERE binding=? AND operation=? AND request_id=? ORDER BY position', identity)
            body['items'] = [{**{f: r[f] for f in ('item_id', 'blob_id', 'path')}, **({'metadata': json.loads(r['metadata'])} if r['metadata_present'] else {})} for r in entries]
    for field in present(row['null_fields'], FIELDS):
        body[field] = None
    if hashlib.sha256(encoded(body)).digest() != row['fingerprint']:
        raise ValueError('Stored outbound parameters do not match their durable fingerprint.')
    return body


def save_inbound(db, binding, item, content):
    key = item['item_id']
    if item['item_type'] == 'message':
        source = content.get('source') or {}
        db.execute('INSERT OR IGNORE INTO local_messages VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)',
                   (binding, key, content['message_id'], content['producer_id'], content['thread_id'], content.get('in_reply_to'), content['kind'], content['subject'], content['body']['text'], content.get('committed_at'), source.get('native_thread_id'), source.get('native_message_id'), source.get('url'), encoded(content.get('metadata', {})).decode()))
        for field, entries in [('recipient_ids', content.get('recipient_ids', [])), ('attachment_ids', content.get('attachment_ids', [])), ('body.options', content['body'].get('options', []))]:
            for position, value in enumerate(entries):
                db.execute('INSERT OR IGNORE INTO local_message_values VALUES(?,?,?,?,?)', (binding, key, field, position, value))
    elif item['item_type'] == 'memory_revision':
        collection = content['collection']
        db.execute('INSERT OR IGNORE INTO local_memory_versions VALUES(?,?,?,?,?,?)', (binding, key, item['resource_id'], item['resource_version'], collection['source_actor_id'], collection['scope']))
        for entry in content['items']:
            db.execute('INSERT OR IGNORE INTO local_memory_items VALUES(?,?,?,?,?,?,?)', (binding, key, entry['item_id'], entry['blob_id'], entry['path'], encoded(entry['metadata']).decode(), entry.get('deleted_in_revision')))
    elif item['item_type'] == 'receipt':
        db.execute('INSERT OR IGNORE INTO local_receipt_updates VALUES(?,?,?,?,?,?)', (binding, key, content['consumer_id'], content['subject_item_id'], content['stage'], item['resource_version']))
