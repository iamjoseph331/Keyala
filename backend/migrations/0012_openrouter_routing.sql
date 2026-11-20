-- Optional routing of OpenAI/Anthropic-protocol apps through the user's OpenRouter key.
-- users.openrouter_for_direct: account setting, OFF by default; only affects NEW connects.
-- grants.billing_vendor: frozen at approval. NULL = direct (same as grants.vendor), 'openrouter' = routed.
-- requests.billing_vendor / upstream_model: what this request was actually sent to and under which model slug
-- (NULL = direct). The requested vendor/model columns keep the original protocol and model.
ALTER TABLE users ADD COLUMN openrouter_for_direct INTEGER NOT NULL DEFAULT 0;
ALTER TABLE grants ADD COLUMN billing_vendor TEXT;
ALTER TABLE requests ADD COLUMN billing_vendor TEXT;
ALTER TABLE requests ADD COLUMN upstream_model TEXT;
