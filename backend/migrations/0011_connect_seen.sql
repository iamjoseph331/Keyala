-- Hint for the developer's pairing UI: 'web' = the /connect page was opened (app likely not installed), 'app' = the app previewed it.
ALTER TABLE connect_codes ADD COLUMN seen TEXT;
