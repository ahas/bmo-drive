-- Every share link and every kept event is one owner's (docs/api.md, "Owners"): the owner key it was made
-- for, empty for a row the service gives the administrator's key as it starts. One entry of one owner has at
-- most one live link, so the same place and path may stand once for each owner.
CREATE TABLE file_shares_owned (
  share_token TEXT PRIMARY KEY,
  owner TEXT NOT NULL DEFAULT '',
  place TEXT NOT NULL,
  path TEXT NOT NULL,
  kind TEXT NOT NULL,
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  expires_at INTEGER,
  UNIQUE (owner, place, path)
);

INSERT INTO file_shares_owned (share_token, owner, place, path, kind, created_by, created_at, expires_at)
SELECT share_token, '', place, path, kind, created_by, created_at, expires_at FROM file_shares;

DROP TABLE file_shares;

ALTER TABLE file_shares_owned RENAME TO file_shares;

ALTER TABLE events ADD COLUMN owner TEXT NOT NULL DEFAULT '';

-- The quotas set for owners (docs/files.md, "Quotas"), in bytes; an owner with no row has the settings'
-- default.
CREATE TABLE quotas (
  owner TEXT PRIMARY KEY,
  quota_bytes INTEGER NOT NULL,
  set_at INTEGER NOT NULL
);
