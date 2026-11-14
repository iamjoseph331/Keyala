CREATE TABLE users (
  id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE, created_at TEXT NOT NULL);
CREATE TABLE sessions (
  token_hash TEXT PRIMARY KEY, user_id TEXT NOT NULL REFERENCES users(id), created_at TEXT NOT NULL);
CREATE TABLE devices (
  user_id TEXT NOT NULL REFERENCES users(id), apns_token TEXT NOT NULL, created_at TEXT NOT NULL,
  PRIMARY KEY (user_id, apns_token));
-- ciphertext = AES-256-GCM(api key), only last4 kept in clear
CREATE TABLE api_keys (
  user_id TEXT NOT NULL REFERENCES users(id), vendor TEXT NOT NULL,
  ciphertext BLOB NOT NULL, nonce BLOB NOT NULL, last4 TEXT NOT NULL, created_at TEXT NOT NULL,
  PRIMARY KEY (user_id, vendor));
CREATE TABLE apps (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, homepage TEXT, icon_url TEXT,
  secret_hash TEXT NOT NULL, created_at TEXT NOT NULL);
CREATE TABLE connect_codes (
  code TEXT PRIMARY KEY, app_id TEXT NOT NULL REFERENCES apps(id), vendor TEXT NOT NULL,
  suggested_policy TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending',
  grant_id TEXT, created_at TEXT NOT NULL, expires_at TEXT NOT NULL);
-- money is integer micro-USD. token_hash is NULL until the developer claims the token via
-- GET /v1/connect/{code} (tokens are never stored in clear).
CREATE TABLE grants (
  id TEXT PRIMARY KEY, user_id TEXT NOT NULL REFERENCES users(id), app_id TEXT NOT NULL REFERENCES apps(id),
  vendor TEXT NOT NULL, token_hash TEXT UNIQUE,
  mode TEXT NOT NULL, remaining_count INTEGER, remaining_micro INTEGER, alert_micro INTEGER, expires_at TEXT,
  created_at TEXT NOT NULL, last_used_at TEXT,
  spent_micro INTEGER NOT NULL DEFAULT 0, request_count INTEGER NOT NULL DEFAULT 0, revoked_at TEXT);
CREATE TABLE requests (
  id TEXT PRIMARY KEY, grant_id TEXT NOT NULL REFERENCES grants(id), user_id TEXT NOT NULL, app_id TEXT NOT NULL,
  vendor TEXT NOT NULL, model TEXT NOT NULL, endpoint TEXT NOT NULL, status TEXT NOT NULL,
  estimated_micro INTEGER NOT NULL, actual_micro INTEGER,
  reserved_micro INTEGER NOT NULL DEFAULT 0, reserved_count INTEGER NOT NULL DEFAULT 0,
  payload TEXT NOT NULL, response TEXT, http_status INTEGER,
  created_at TEXT NOT NULL, decided_at TEXT, completed_at TEXT, expires_at TEXT NOT NULL);
CREATE INDEX requests_user ON requests(user_id, created_at);
CREATE INDEX requests_grant ON requests(grant_id, created_at);
