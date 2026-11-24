-- The drive's store at 1.0.0, whole (docs/files.md, 「The store」).

CREATE TABLE events (
  event_id INTEGER PRIMARY KEY AUTOINCREMENT,
  kind TEXT NOT NULL,
  at INTEGER NOT NULL,
  data TEXT NOT NULL
, owner TEXT NOT NULL DEFAULT '');

CREATE TABLE "file_shares" (
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

CREATE TABLE quotas (
  owner TEXT PRIMARY KEY,
  quota_bytes INTEGER NOT NULL,
  set_at INTEGER NOT NULL
);

CREATE TABLE member_shares (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  owner TEXT NOT NULL,
  place TEXT NOT NULL,
  path TEXT NOT NULL,
  kind TEXT NOT NULL,
  member TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE (owner, place, path, member)
);

CREATE INDEX member_shares_member ON member_shares (member, id);

