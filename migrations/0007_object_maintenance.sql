-- Durable journal BEFORE object I/O. Cleaning rows are retained as tombstones:
-- a PUT already in flight may finish after deletion and must be swept again.
CREATE TABLE object_writes (
 object_key TEXT PRIMARY KEY,submission_id TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'active' CHECK(state IN('active','cleaning')),
 created_at INTEGER NOT NULL,last_swept_at INTEGER,
 lease_nonce TEXT,lease_until INTEGER,
 FOREIGN KEY(submission_id) REFERENCES submissions(submission_id)
);
CREATE INDEX object_writes_submission ON object_writes(submission_id);
CREATE INDEX uploads_expiry ON uploads(state,expires_at);
INSERT INTO object_writes(object_key,submission_id,created_at)
 SELECT object_key,submission_id,created_at FROM uploads;
INSERT INTO object_writes(object_key,submission_id,created_at)
 SELECT body_ref,submission_id,created_at FROM submissions WHERE body_ref IS NOT NULL
 ON CONFLICT(object_key) DO NOTHING;

-- A per-Space position prevents exposing other Spaces' traffic through sequence gaps.
ALTER TABLE feed_items ADD COLUMN space_sequence INTEGER;
WITH positions AS (
 SELECT sequence,row_number() OVER(PARTITION BY space_id ORDER BY sequence) AS position FROM feed_items
)
UPDATE feed_items SET space_sequence=(SELECT position FROM positions WHERE positions.sequence=feed_items.sequence);
UPDATE consumer_checkpoints SET sequence=(SELECT count(*) FROM feed_items f WHERE f.space_id=consumer_checkpoints.space_id AND f.sequence<=consumer_checkpoints.sequence);
CREATE UNIQUE INDEX feed_space_position ON feed_items(space_id,space_sequence);
CREATE TRIGGER assign_feed_space_position AFTER INSERT ON feed_items
 WHEN NEW.space_sequence IS NULL
 BEGIN
 UPDATE feed_items SET space_sequence=(SELECT coalesce(max(space_sequence),0)+1 FROM feed_items WHERE space_id=NEW.space_id)
 WHERE sequence=NEW.sequence;
 END;
