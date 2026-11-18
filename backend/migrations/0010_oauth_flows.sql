-- OAuth attempts become queryable flows: the app correlates its callback with `id` and reads the
-- outcome with its session (the callback URL is never proof). Pending states are short-lived, so
-- dropping the old table loses nothing that matters.
DROP TABLE oauth_states;
CREATE TABLE oauth_flows (
  id TEXT PRIMARY KEY, state_hash TEXT NOT NULL UNIQUE, user_id TEXT NOT NULL REFERENCES users(id),
  provider TEXT NOT NULL, verifier TEXT, -- cleared once the code exchange starts
  status TEXT NOT NULL DEFAULT 'pending', -- pending|exchanging|saved|failed|cancelled (expired is computed)
  reason TEXT, last4 TEXT, created_at TEXT NOT NULL, expires_at TEXT NOT NULL);
