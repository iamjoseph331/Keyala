-- Per-user data key (DEK): 32 random bytes wrapped with the master key (AES-GCM, AAD dek|{user_id}).
-- requests.payload/response are stored as `enc1:` + base64url(nonce||ciphertext) under that key;
-- rows written before this migration stay plaintext until the startup pass encrypts them.
ALTER TABLE users ADD COLUMN dek_ct BLOB;
ALTER TABLE users ADD COLUMN dek_nonce BLOB;
-- Token counts as columns: reports can no longer read them from the (now encrypted or purged) response.
ALTER TABLE requests ADD COLUMN input_tokens INTEGER;
ALTER TABLE requests ADD COLUMN output_tokens INTEGER;
-- 1 = payload/response removed by the 30-day retention sweep; metadata (model, cost, tokens) stays.
ALTER TABLE requests ADD COLUMN purged INTEGER NOT NULL DEFAULT 0;
UPDATE requests SET
  input_tokens = CASE WHEN json_valid(response) THEN COALESCE(json_extract(response,'$.usage.prompt_tokens'), json_extract(response,'$.usage.input_tokens')) END,
  output_tokens = CASE WHEN json_valid(response) THEN COALESCE(json_extract(response,'$.usage.completion_tokens'), json_extract(response,'$.usage.output_tokens')) END
WHERE response IS NOT NULL;
