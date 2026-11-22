-- Owners can delete console apps. The row stays (users' history keeps the app name); a deleted app
-- authenticates nothing and accepts no new connections.
ALTER TABLE apps ADD COLUMN deleted_at TEXT;
