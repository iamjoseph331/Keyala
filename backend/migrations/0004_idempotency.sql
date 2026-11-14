ALTER TABLE requests ADD COLUMN idem_key TEXT;
ALTER TABLE requests ADD COLUMN body_hash TEXT;
CREATE UNIQUE INDEX requests_idem ON requests(grant_id, idem_key) WHERE idem_key IS NOT NULL;
