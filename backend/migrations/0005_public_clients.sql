-- Browser ("public") clients: an app may list allowed web origins (JSON array). NULL = server-side only.
ALTER TABLE apps ADD COLUMN allowed_origins TEXT;
-- A connect started from a browser has no app secret; the grant is claimed with this per-code secret.
ALTER TABLE connect_codes ADD COLUMN secret_hash TEXT;
ALTER TABLE connect_codes ADD COLUMN origin TEXT;
