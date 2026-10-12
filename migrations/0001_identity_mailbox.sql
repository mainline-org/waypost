CREATE TABLE IF NOT EXISTS spaces (
 space_id TEXT PRIMARY KEY, display_name TEXT NOT NULL, source_uri TEXT NOT NULL UNIQUE,
 default_actor_id TEXT NOT NULL, default_consumer_id TEXT NOT NULL, created_at INTEGER NOT NULL,
 FOREIGN KEY(space_id,default_actor_id) REFERENCES actors(space_id,actor_id) DEFERRABLE INITIALLY DEFERRED,
 FOREIGN KEY(space_id,default_consumer_id) REFERENCES consumers(space_id,consumer_id) DEFERRABLE INITIALLY DEFERRED
);
CREATE TABLE IF NOT EXISTS actors (
 space_id TEXT NOT NULL,actor_id TEXT NOT NULL,display_name TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('user','agent','client')), disabled INTEGER NOT NULL DEFAULT 0 CHECK(disabled IN(0,1)),
 authority_version INTEGER NOT NULL DEFAULT 0,created_at INTEGER NOT NULL,
 PRIMARY KEY(space_id,actor_id),FOREIGN KEY(space_id) REFERENCES spaces(space_id)
);
CREATE TABLE IF NOT EXISTS consumers (
 space_id TEXT NOT NULL,consumer_id TEXT NOT NULL,actor_id TEXT NOT NULL,
 view TEXT NOT NULL CHECK(view IN('inbox','space_log')),view_revision INTEGER NOT NULL DEFAULT 1,
 disabled INTEGER NOT NULL DEFAULT 0 CHECK(disabled IN(0,1)),created_at INTEGER NOT NULL,
 PRIMARY KEY(space_id,consumer_id),FOREIGN KEY(space_id,actor_id) REFERENCES actors(space_id,actor_id)
);
CREATE TABLE IF NOT EXISTS consumer_filters (
 space_id TEXT NOT NULL,consumer_id TEXT NOT NULL,dimension TEXT NOT NULL CHECK(dimension IN('producer_ids','thread_ids','item_types','collection_ids')),
 value TEXT NOT NULL,PRIMARY KEY(space_id,consumer_id,dimension,value),
 FOREIGN KEY(space_id,consumer_id) REFERENCES consumers(space_id,consumer_id)
);
CREATE TABLE IF NOT EXISTS tokens (
 token_id TEXT PRIMARY KEY,token_digest BLOB NOT NULL UNIQUE CHECK(length(token_digest)=32),
 space_id TEXT NOT NULL,actor_id TEXT NOT NULL,consumer_id TEXT NOT NULL,authority_version INTEGER NOT NULL DEFAULT 0,
 expires_at INTEGER,revoked_at INTEGER,created_at INTEGER NOT NULL,
 FOREIGN KEY(space_id,actor_id) REFERENCES actors(space_id,actor_id),
 FOREIGN KEY(space_id,consumer_id) REFERENCES consumers(space_id,consumer_id)
);
CREATE TABLE IF NOT EXISTS token_scopes (token_id TEXT NOT NULL,scope TEXT NOT NULL,PRIMARY KEY(token_id,scope),FOREIGN KEY(token_id) REFERENCES tokens(token_id));
CREATE TABLE IF NOT EXISTS token_recipients (token_id TEXT NOT NULL,space_id TEXT NOT NULL,actor_id TEXT NOT NULL,
 PRIMARY KEY(token_id,actor_id),FOREIGN KEY(token_id) REFERENCES tokens(token_id),FOREIGN KEY(space_id,actor_id) REFERENCES actors(space_id,actor_id));
CREATE TABLE IF NOT EXISTS token_collections (token_id TEXT NOT NULL,space_id TEXT NOT NULL,collection_id TEXT NOT NULL,action TEXT NOT NULL CHECK(action IN('read','write')),
 PRIMARY KEY(token_id,collection_id,action),FOREIGN KEY(token_id) REFERENCES tokens(token_id));
CREATE TABLE IF NOT EXISTS token_blob_grants (token_id TEXT NOT NULL,space_id TEXT NOT NULL,blob_id TEXT NOT NULL,
 PRIMARY KEY(token_id,blob_id),FOREIGN KEY(token_id) REFERENCES tokens(token_id));
CREATE TABLE IF NOT EXISTS threads (space_id TEXT NOT NULL,thread_id TEXT NOT NULL,created_at INTEGER NOT NULL,
 PRIMARY KEY(space_id,thread_id),FOREIGN KEY(space_id) REFERENCES spaces(space_id));
CREATE TABLE IF NOT EXISTS thread_members (space_id TEXT NOT NULL,thread_id TEXT NOT NULL,actor_id TEXT NOT NULL,
 PRIMARY KEY(space_id,thread_id,actor_id),FOREIGN KEY(space_id,thread_id) REFERENCES threads(space_id,thread_id),FOREIGN KEY(space_id,actor_id) REFERENCES actors(space_id,actor_id));
CREATE TABLE IF NOT EXISTS submissions (
 submission_id TEXT PRIMARY KEY,space_id TEXT NOT NULL,producer_id TEXT NOT NULL,operation TEXT NOT NULL,request_id TEXT NOT NULL,
 fingerprint BLOB NOT NULL CHECK(length(fingerprint)=32),resource_id TEXT NOT NULL,thread_id TEXT,
 state TEXT NOT NULL CHECK(state IN('reserved','prepared','committed','failed')),attempt_nonce TEXT,
 body_ref TEXT,body_sha256 BLOB,body_size INTEGER,created_at INTEGER NOT NULL,prepared_at INTEGER,committed_at INTEGER,
 result_revision INTEGER,result_changed INTEGER,error_code TEXT,error_current_revision INTEGER,
 UNIQUE(space_id,producer_id,operation,request_id),FOREIGN KEY(space_id,producer_id) REFERENCES actors(space_id,actor_id)
);
CREATE TABLE IF NOT EXISTS submission_recipients (submission_id TEXT NOT NULL,space_id TEXT NOT NULL,actor_id TEXT NOT NULL,
 PRIMARY KEY(submission_id,actor_id),FOREIGN KEY(submission_id) REFERENCES submissions(submission_id),FOREIGN KEY(space_id,actor_id) REFERENCES actors(space_id,actor_id));
CREATE TABLE IF NOT EXISTS admin_submissions (operation TEXT NOT NULL,request_id TEXT NOT NULL,fingerprint BLOB NOT NULL CHECK(length(fingerprint)=32),
 space_id TEXT,resource_id TEXT NOT NULL,subresource_id TEXT,created_at INTEGER NOT NULL,PRIMARY KEY(operation,request_id));
CREATE TABLE IF NOT EXISTS messages (
 space_id TEXT NOT NULL,message_id TEXT NOT NULL,submission_id TEXT NOT NULL UNIQUE,producer_id TEXT NOT NULL,thread_id TEXT NOT NULL,
 in_reply_to TEXT,kind TEXT NOT NULL,subject TEXT NOT NULL,body_ref TEXT NOT NULL,committed_at INTEGER NOT NULL,
 PRIMARY KEY(space_id,message_id),FOREIGN KEY(submission_id) REFERENCES submissions(submission_id),
 FOREIGN KEY(space_id,producer_id) REFERENCES actors(space_id,actor_id),FOREIGN KEY(space_id,thread_id) REFERENCES threads(space_id,thread_id)
);
CREATE TABLE IF NOT EXISTS message_recipients (space_id TEXT NOT NULL,message_id TEXT NOT NULL,actor_id TEXT NOT NULL,
 PRIMARY KEY(space_id,message_id,actor_id),FOREIGN KEY(space_id,message_id) REFERENCES messages(space_id,message_id),FOREIGN KEY(space_id,actor_id) REFERENCES actors(space_id,actor_id));
CREATE TABLE IF NOT EXISTS message_attachments (space_id TEXT NOT NULL,message_id TEXT NOT NULL,position INTEGER NOT NULL,blob_id TEXT NOT NULL,
 PRIMARY KEY(space_id,message_id,position),FOREIGN KEY(space_id,message_id) REFERENCES messages(space_id,message_id));
CREATE TABLE IF NOT EXISTS feed_items (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT,item_id TEXT NOT NULL UNIQUE,space_id TEXT NOT NULL,
 item_type TEXT NOT NULL CHECK(item_type IN('message','memory_revision','receipt')),resource_id TEXT NOT NULL,resource_version INTEGER NOT NULL,
 producer_id TEXT NOT NULL,thread_id TEXT,committed_at INTEGER NOT NULL,
 UNIQUE(space_id,item_type,resource_id,resource_version),FOREIGN KEY(space_id) REFERENCES spaces(space_id)
);
CREATE TABLE IF NOT EXISTS feed_recipients (space_id TEXT NOT NULL,item_id TEXT NOT NULL,actor_id TEXT NOT NULL,
 PRIMARY KEY(space_id,item_id,actor_id),FOREIGN KEY(item_id) REFERENCES feed_items(item_id),FOREIGN KEY(space_id,actor_id) REFERENCES actors(space_id,actor_id));
CREATE INDEX IF NOT EXISTS feed_space_sequence ON feed_items(space_id,sequence);
CREATE INDEX IF NOT EXISTS feed_actor ON feed_recipients(space_id,actor_id,item_id);
CREATE INDEX IF NOT EXISTS feed_thread ON feed_items(space_id,thread_id,sequence);
CREATE INDEX IF NOT EXISTS feed_resource ON feed_items(space_id,item_type,resource_id,sequence);
