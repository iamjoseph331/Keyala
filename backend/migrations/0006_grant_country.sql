-- Developer reports: country (ISO 3166-1 alpha-2) of the user's phone when they approved the connect. Code only, never the IP.
ALTER TABLE grants ADD COLUMN country TEXT;
CREATE INDEX requests_app ON requests(app_id, created_at);
