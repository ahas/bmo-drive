-- The members each entry is shared with (docs/files.md, "Sharing with members"), one row per member of an
-- entry of an owner, which the table holds rather than the code. The permission is read alone, so no row
-- carries one.
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
