CREATE TABLE uploads (
 space_id TEXT NOT NULL,upload_id TEXT NOT NULL,blob_id TEXT NOT NULL,producer_id TEXT NOT NULL,submission_id TEXT NOT NULL UNIQUE,
 object_key TEXT NOT NULL UNIQUE,content_type TEXT NOT NULL,size_bytes INTEGER NOT NULL CHECK(size_bytes>=0),sha256 BLOB NOT NULL CHECK(length(sha256)=32),filename TEXT,
 state TEXT NOT NULL CHECK(state IN('pending','sealed')),created_at INTEGER NOT NULL,expires_at INTEGER NOT NULL,
 PRIMARY KEY(space_id,upload_id),UNIQUE(space_id,blob_id),FOREIGN KEY(space_id,producer_id) REFERENCES actors(space_id,actor_id),FOREIGN KEY(submission_id) REFERENCES submissions(submission_id)
);
CREATE TABLE blobs (
 space_id TEXT NOT NULL,blob_id TEXT NOT NULL,owner_actor_id TEXT NOT NULL,object_key TEXT NOT NULL UNIQUE,
 content_type TEXT NOT NULL,size_bytes INTEGER NOT NULL CHECK(size_bytes>=0),sha256 BLOB NOT NULL CHECK(length(sha256)=32),filename TEXT,sealed_at INTEGER NOT NULL,
 PRIMARY KEY(space_id,blob_id),FOREIGN KEY(space_id,owner_actor_id) REFERENCES actors(space_id,actor_id)
);
CREATE TABLE memory_collections (
 space_id TEXT NOT NULL,collection_id TEXT NOT NULL,source_actor_id TEXT NOT NULL,scope TEXT NOT NULL,display_name TEXT NOT NULL,
 current_revision INTEGER NOT NULL DEFAULT 0 CHECK(current_revision>=0 AND current_revision<=9007199254740991),created_at INTEGER NOT NULL,
 PRIMARY KEY(space_id,collection_id),FOREIGN KEY(space_id,source_actor_id) REFERENCES actors(space_id,actor_id)
);
CREATE TABLE memory_revisions (
 space_id TEXT NOT NULL,collection_id TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision>0),parent_revision INTEGER NOT NULL,
 manifest_blob_id TEXT NOT NULL,publisher_actor_id TEXT NOT NULL,submission_id TEXT NOT NULL UNIQUE,committed_at INTEGER NOT NULL,
 PRIMARY KEY(space_id,collection_id,revision),FOREIGN KEY(space_id,collection_id) REFERENCES memory_collections(space_id,collection_id),
 FOREIGN KEY(space_id,manifest_blob_id) REFERENCES blobs(space_id,blob_id),FOREIGN KEY(submission_id) REFERENCES submissions(submission_id)
);
CREATE TABLE memory_revision_items (
 space_id TEXT NOT NULL,collection_id TEXT NOT NULL,revision INTEGER NOT NULL,item_id TEXT NOT NULL,blob_id TEXT NOT NULL,path TEXT NOT NULL,metadata TEXT NOT NULL,
 deleted_in_revision INTEGER,
 PRIMARY KEY(space_id,collection_id,revision,item_id),FOREIGN KEY(space_id,collection_id,revision) REFERENCES memory_revisions(space_id,collection_id,revision),
 FOREIGN KEY(space_id,blob_id) REFERENCES blobs(space_id,blob_id)
);
CREATE UNIQUE INDEX memory_active_paths ON memory_revision_items(space_id,collection_id,revision,path) WHERE deleted_in_revision IS NULL;
CREATE INDEX memory_blob_references ON memory_revision_items(space_id,blob_id,collection_id);
ALTER TABLE submissions ADD COLUMN result_manifest_blob_id TEXT;
