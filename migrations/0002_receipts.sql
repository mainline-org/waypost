CREATE UNIQUE INDEX IF NOT EXISTS feed_space_item ON feed_items(space_id,item_id);
CREATE TABLE receipts (
 space_id TEXT NOT NULL,receipt_id TEXT NOT NULL,consumer_id TEXT NOT NULL,subject_item_id TEXT NOT NULL,
 stage INTEGER NOT NULL CHECK(stage IN(1,2)),version INTEGER NOT NULL CHECK(version>0),updated_at INTEGER NOT NULL,last_submission_id TEXT NOT NULL,
 PRIMARY KEY(space_id,receipt_id),UNIQUE(space_id,consumer_id,subject_item_id),
 FOREIGN KEY(space_id,consumer_id) REFERENCES consumers(space_id,consumer_id),FOREIGN KEY(space_id,subject_item_id) REFERENCES feed_items(space_id,item_id)
);
CREATE TABLE feed_receipt_updates (
 space_id TEXT NOT NULL,item_id TEXT NOT NULL,receipt_id TEXT NOT NULL,subject_item_id TEXT NOT NULL,subject_resource_id TEXT NOT NULL,
 consumer_id TEXT NOT NULL,actor_id TEXT NOT NULL,stage INTEGER NOT NULL CHECK(stage IN(1,2)),receipt_version INTEGER NOT NULL,updated_at INTEGER NOT NULL,
 PRIMARY KEY(space_id,item_id),UNIQUE(space_id,receipt_id,receipt_version),
 FOREIGN KEY(space_id,item_id) REFERENCES feed_items(space_id,item_id),FOREIGN KEY(space_id,receipt_id) REFERENCES receipts(space_id,receipt_id)
);
CREATE TABLE submission_ack_items (
 submission_id TEXT NOT NULL,space_id TEXT NOT NULL,position INTEGER NOT NULL,subject_item_id TEXT NOT NULL,desired_stage INTEGER NOT NULL CHECK(desired_stage IN(1,2)),
 candidate_receipt_id TEXT NOT NULL,candidate_feed_id TEXT NOT NULL,result_stage INTEGER,result_version INTEGER,
 PRIMARY KEY(submission_id,position),UNIQUE(submission_id,subject_item_id),FOREIGN KEY(submission_id) REFERENCES submissions(submission_id),
 FOREIGN KEY(space_id,subject_item_id) REFERENCES feed_items(space_id,item_id)
);
CREATE TABLE consumer_checkpoints (
 space_id TEXT NOT NULL,consumer_id TEXT NOT NULL,view_revision INTEGER NOT NULL,grant_fingerprint TEXT NOT NULL,
 sequence INTEGER NOT NULL CHECK(sequence>=0),updated_at INTEGER NOT NULL,
 PRIMARY KEY(space_id,consumer_id,view_revision,grant_fingerprint),FOREIGN KEY(space_id,consumer_id) REFERENCES consumers(space_id,consumer_id)
);
ALTER TABLE submissions ADD COLUMN result_checkpoint_cursor TEXT;
