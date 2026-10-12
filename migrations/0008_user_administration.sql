CREATE TABLE space_admin_credentials (
 space_id TEXT PRIMARY KEY,credential_digest BLOB NOT NULL UNIQUE CHECK(length(credential_digest)=32),
 version INTEGER NOT NULL,revoked_at INTEGER,created_at INTEGER NOT NULL,
 FOREIGN KEY(space_id) REFERENCES spaces(space_id)
);
ALTER TABLE admin_sessions ADD COLUMN space_id TEXT REFERENCES spaces(space_id);
