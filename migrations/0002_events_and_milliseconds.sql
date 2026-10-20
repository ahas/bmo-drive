-- The service's kept events, the last of them (docs/api.md, "Events"): each numbered as it is written, the
-- number only growing, with its kind, its time in epoch milliseconds and its inner data as JSON.
CREATE TABLE IF NOT EXISTS events (
  event_id INTEGER PRIMARY KEY AUTOINCREMENT,
  kind TEXT NOT NULL,
  at INTEGER NOT NULL,
  data TEXT NOT NULL
);

-- A link's times are epoch milliseconds, as every time the service answers.
UPDATE file_shares SET created_at = created_at * 1000, expires_at = expires_at * 1000;
