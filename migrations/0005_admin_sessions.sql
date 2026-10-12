CREATE TABLE admin_sessions(session_digest BLOB PRIMARY KEY CHECK(length(session_digest)=32),admin_fingerprint BLOB NOT NULL CHECK(length(admin_fingerprint)=32),created_at INTEGER NOT NULL,expires_at INTEGER NOT NULL);
CREATE INDEX admin_session_expiry ON admin_sessions(expires_at);
