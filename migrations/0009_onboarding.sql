ALTER TABLE spaces ADD COLUMN primary_actor_id TEXT;
ALTER TABLE spaces ADD COLUMN primary_nonce TEXT;
ALTER TABLE spaces ADD COLUMN primary_revision INTEGER NOT NULL DEFAULT 0 CHECK(primary_revision >= 0);
ALTER TABLE tokens ADD COLUMN default_recipient_id TEXT;
CREATE TRIGGER tokens_default_recipient_insert BEFORE INSERT ON tokens
 WHEN NEW.default_recipient_id IS NOT NULL AND NOT EXISTS(
  SELECT 1 FROM actors WHERE space_id=NEW.space_id AND actor_id=NEW.default_recipient_id)
 BEGIN SELECT RAISE(ABORT,'default recipient belongs to another space'); END;
CREATE TRIGGER tokens_default_recipient_update BEFORE UPDATE OF default_recipient_id,space_id ON tokens
 WHEN NEW.default_recipient_id IS NOT NULL AND NOT EXISTS(
  SELECT 1 FROM actors WHERE space_id=NEW.space_id AND actor_id=NEW.default_recipient_id)
 BEGIN SELECT RAISE(ABORT,'default recipient belongs to another space'); END;
CREATE TABLE space_invitations (
 invitation_id TEXT PRIMARY KEY,space_id TEXT NOT NULL,
 token_digest BLOB NOT NULL UNIQUE CHECK(length(token_digest)=32),
 owner_version INTEGER NOT NULL CHECK(owner_version >= 0),
 state TEXT NOT NULL CHECK(state IN ('pending','redeemed','revoked')),
 created_at INTEGER NOT NULL,expires_at INTEGER NOT NULL,redeemed_at INTEGER,redeem_nonce TEXT,
 FOREIGN KEY(space_id) REFERENCES spaces(space_id)
);
CREATE INDEX space_invitations_pending ON space_invitations(space_id,state,expires_at);

CREATE TABLE access_requests (
 request_id TEXT PRIMARY KEY,contact TEXT NOT NULL,created_at INTEGER NOT NULL,
 status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','issued')),
 space_id TEXT REFERENCES spaces(space_id)
);
CREATE INDEX access_requests_status ON access_requests(status,created_at,request_id);
