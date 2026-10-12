CREATE UNIQUE INDEX tokens_space_identity ON tokens(token_id,space_id);
CREATE TABLE token_recipients_v2(token_id TEXT NOT NULL,space_id TEXT NOT NULL,actor_id TEXT NOT NULL,
 PRIMARY KEY(token_id,actor_id),FOREIGN KEY(token_id,space_id) REFERENCES tokens(token_id,space_id),FOREIGN KEY(space_id,actor_id) REFERENCES actors(space_id,actor_id));
INSERT INTO token_recipients_v2 SELECT * FROM token_recipients;
DROP TABLE token_recipients;
ALTER TABLE token_recipients_v2 RENAME TO token_recipients;
CREATE TABLE token_collections_v2(token_id TEXT NOT NULL,space_id TEXT NOT NULL,collection_id TEXT NOT NULL,action TEXT NOT NULL CHECK(action IN('read','write')),
 PRIMARY KEY(token_id,collection_id,action),FOREIGN KEY(token_id,space_id) REFERENCES tokens(token_id,space_id),FOREIGN KEY(space_id,collection_id) REFERENCES memory_collections(space_id,collection_id));
INSERT INTO token_collections_v2 SELECT * FROM token_collections;
DROP TABLE token_collections;
ALTER TABLE token_collections_v2 RENAME TO token_collections;
CREATE TABLE token_blob_grants_v2(token_id TEXT NOT NULL,space_id TEXT NOT NULL,blob_id TEXT NOT NULL,
 PRIMARY KEY(token_id,blob_id),FOREIGN KEY(token_id,space_id) REFERENCES tokens(token_id,space_id),FOREIGN KEY(space_id,blob_id) REFERENCES blobs(space_id,blob_id));
INSERT INTO token_blob_grants_v2 SELECT * FROM token_blob_grants;
DROP TABLE token_blob_grants;
ALTER TABLE token_blob_grants_v2 RENAME TO token_blob_grants;
CREATE TABLE message_attachments_v2(space_id TEXT NOT NULL,message_id TEXT NOT NULL,position INTEGER NOT NULL,blob_id TEXT NOT NULL,
 PRIMARY KEY(space_id,message_id,position),FOREIGN KEY(space_id,message_id) REFERENCES messages(space_id,message_id),FOREIGN KEY(space_id,blob_id) REFERENCES blobs(space_id,blob_id));
INSERT INTO message_attachments_v2 SELECT * FROM message_attachments;
DROP TABLE message_attachments;
ALTER TABLE message_attachments_v2 RENAME TO message_attachments;
CREATE TRIGGER token_consumer_binding_insert BEFORE INSERT ON tokens
 WHEN NOT EXISTS(SELECT 1 FROM consumers WHERE space_id=NEW.space_id AND consumer_id=NEW.consumer_id AND actor_id=NEW.actor_id)
 BEGIN SELECT RAISE(ABORT,'token consumer must belong to actor'); END;
CREATE TRIGGER token_consumer_binding_update BEFORE UPDATE OF space_id,consumer_id,actor_id ON tokens
 WHEN NOT EXISTS(SELECT 1 FROM consumers WHERE space_id=NEW.space_id AND consumer_id=NEW.consumer_id AND actor_id=NEW.actor_id)
 BEGIN SELECT RAISE(ABORT,'token consumer must belong to actor'); END;
