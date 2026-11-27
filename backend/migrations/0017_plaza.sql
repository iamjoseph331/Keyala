-- Plaza (opt-in public listing, reviewed by hand). NULL = not submitted. Editing the name or homepage
-- clears it in the same UPDATE, so an approval can never outlive the text that was reviewed.
ALTER TABLE apps ADD COLUMN plaza_status TEXT CHECK (plaza_status IN ('pending','approved','rejected'));
ALTER TABLE apps ADD COLUMN plaza_at TEXT;
CREATE INDEX apps_plaza ON apps (plaza_status, plaza_at) WHERE plaza_status IS NOT NULL;
