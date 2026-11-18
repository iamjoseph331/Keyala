-- One-tap provider sign-in (OpenRouter OAuth PKCE): single-use state -> user + verifier, 10 minutes.
CREATE TABLE oauth_states (
  state_hash TEXT PRIMARY KEY, user_id TEXT NOT NULL REFERENCES users(id), provider TEXT NOT NULL,
  verifier TEXT NOT NULL, created_at TEXT NOT NULL, expires_at TEXT NOT NULL);
