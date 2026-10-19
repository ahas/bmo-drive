-- The file viewer's share links, one row per live link (docs/files.md, "Share links"). One entry has at
-- most one live link, which the table holds rather than the code. The expiry is the one nullable column:
-- a link meant to stand has none.
CREATE TABLE IF NOT EXISTS file_shares (
  share_token TEXT PRIMARY KEY,
  place TEXT NOT NULL,
  path TEXT NOT NULL,
  kind TEXT NOT NULL,
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  expires_at INTEGER,
  UNIQUE (place, path)
);
