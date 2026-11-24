-- Sign in with Google (its own identity, never linked by email) and WebAuthn passkeys.
ALTER TABLE users ADD COLUMN google_sub TEXT;
CREATE UNIQUE INDEX users_google_sub ON users(google_sub) WHERE google_sub IS NOT NULL;
-- public_key = uncompressed P-256 point (ES256 only); credential_id = base64url
CREATE TABLE passkeys (
  id TEXT PRIMARY KEY, user_id TEXT NOT NULL REFERENCES users(id),
  credential_id TEXT NOT NULL UNIQUE, public_key BLOB NOT NULL, sign_count INTEGER NOT NULL DEFAULT 0,
  name TEXT NOT NULL, created_at TEXT NOT NULL, last_used_at TEXT);
-- single-use, 5 minutes. user_id set for registration, NULL for sign-in (discoverable credentials)
CREATE TABLE passkey_challenges (
  id TEXT PRIMARY KEY, user_id TEXT REFERENCES users(id), purpose TEXT NOT NULL,
  challenge TEXT NOT NULL, expires_at TEXT NOT NULL);
