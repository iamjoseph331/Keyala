-- Web console: web-scoped sessions (12 h), phone sign-in for the web, account-owned developer apps, web password.
ALTER TABLE sessions ADD COLUMN scope TEXT NOT NULL DEFAULT 'app';
ALTER TABLE sessions ADD COLUMN expires_at TEXT;
ALTER TABLE users ADD COLUMN password_hash TEXT;
ALTER TABLE apps ADD COLUMN owner_user_id TEXT REFERENCES users(id);
CREATE TABLE web_logins (
  id TEXT PRIMARY KEY, secret_hash TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending',
  user_id TEXT REFERENCES users(id), user_agent TEXT, country TEXT,
  created_at TEXT NOT NULL, expires_at TEXT NOT NULL);
