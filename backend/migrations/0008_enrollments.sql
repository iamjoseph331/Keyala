-- Add a key from a computer: the browser holds `secret_hash`'s token, the phone claims the row,
-- the browser uploads the key (encrypted, AAD enroll|id), the phone confirms, the row is wiped.
CREATE TABLE enrollments (
  id TEXT PRIMARY KEY, secret_hash TEXT NOT NULL, vendor TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending', -- pending|claimed|submitted|saved|cancelled|expired
  user_id TEXT REFERENCES users(id), ciphertext BLOB, nonce BLOB, last4 TEXT,
  created_at TEXT NOT NULL, expires_at TEXT NOT NULL);
