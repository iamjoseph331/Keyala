-- Keyala Coupon: an app owner sponsors calls with a dedicated provider key; recipients claim coupons.
-- Money is integer micro-USD. Invariant per batch (nothing is created or lost by any step):
--   total = available + SUM(coupon.remaining + coupon.held) + spent
-- The ledger is append-only and every money movement has a UNIQUE key, so a retry can never apply twice.
CREATE TABLE coupon_batches (
  id TEXT PRIMARY KEY,
  code TEXT NOT NULL UNIQUE,                       -- shareable `KC-` + 22 base64url chars (case-sensitive), not a credential
  app_id TEXT NOT NULL REFERENCES apps(id),
  owner_user_id TEXT,                              -- issuer; NULL after the issuer deleted the account
  vendor TEXT NOT NULL,
  models TEXT NOT NULL,                            -- JSON array of allowlisted model names
  key_ct BLOB, key_nonce BLOB, key_last4 TEXT NOT NULL,  -- dedicated key, AES-GCM, AAD batch|{id}|{vendor}; wiped on revoke
  per_coupon_micro INTEGER NOT NULL,
  total_micro INTEGER NOT NULL,
  available_micro INTEGER NOT NULL,                -- free budget not reserved by a coupon (may go negative only after an overshoot)
  spent_micro INTEGER NOT NULL DEFAULT 0,          -- truthfully charged, including estimates for calls without usage
  overshoot_micro INTEGER NOT NULL DEFAULT 0,      -- amount by which actual cost exceeded the held estimate
  validity_days INTEGER NOT NULL,
  claim_deadline TEXT NOT NULL,
  created_at TEXT NOT NULL,
  revoked_at TEXT,
  stopped_at TEXT,                                 -- set on unexpected overshoot: the batch accepts no more claims or calls
  cap_low INTEGER NOT NULL DEFAULT 0, cap_episode INTEGER NOT NULL DEFAULT 0,
  exh_flag INTEGER NOT NULL DEFAULT 0, exh_episode INTEGER NOT NULL DEFAULT 0);
CREATE INDEX coupon_batches_app ON coupon_batches(app_id);
CREATE INDEX coupon_batches_owner ON coupon_batches(owner_user_id);

CREATE TABLE coupons (
  id TEXT PRIMARY KEY,
  batch_id TEXT NOT NULL REFERENCES coupon_batches(id),
  user_id TEXT REFERENCES users(id),               -- NULL after the claimant deleted the account while a call was still held
  remaining_micro INTEGER NOT NULL,                -- spendable
  held_micro INTEGER NOT NULL DEFAULT 0,           -- reserved for approved / sending / uncertain calls (never classified as spent while unknown)
  spent_micro INTEGER NOT NULL DEFAULT 0,
  claimed_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  closed_at TEXT,                                  -- unused balance was returned to the batch (expiry or revoke), once
  UNIQUE (user_id, batch_id));                     -- one claim, forever, per account and batch
CREATE INDEX coupons_batch ON coupons(batch_id);

CREATE TABLE coupon_ledger (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  batch_id TEXT NOT NULL,                          -- no foreign keys and no user id: survives account deletion, holds no identity
  coupon_id TEXT,
  request_id TEXT,
  model TEXT,                                      -- on hold and close rows: lets reports survive the deletion of request rows
  kind TEXT NOT NULL,                              -- claim | hold | settle (provider usage) | estimate (no usage, estimate charged) | release | return | overshoot | first_success
  micro INTEGER NOT NULL,
  ukey TEXT NOT NULL UNIQUE,                       -- claim:{coupon} hold:{req} close:{req} return:{coupon} refund:{req} overshoot:{req} first:{coupon}
  created_at TEXT NOT NULL);
CREATE INDEX coupon_ledger_batch ON coupon_ledger(batch_id);

-- Email transitions. The recipient, sender, subject and text are bound once, before the first send attempt, and are
-- then immutable (trigger below): every retry sends exactly the same message with the same Idempotency-Key (= id).
-- They are erased as soon as the row is finished and when the issuer deletes the account.
CREATE TABLE coupon_outbox (
  id TEXT PRIMARY KEY,                             -- also the provider Idempotency-Key
  batch_id TEXT NOT NULL,
  kind TEXT NOT NULL,                              -- capacity | exhausted | stopped
  dedup_key TEXT NOT NULL UNIQUE,                  -- {kind}:{batch}:{episode}
  status TEXT NOT NULL DEFAULT 'pending',          -- pending | sending | sent | failed | skipped
  provider TEXT,                                   -- resend | mock
  attempts INTEGER NOT NULL DEFAULT 0,
  first_attempt_at TEXT,                           -- providers de-duplicate keys for 24h: nothing is sent after that
  next_attempt_at TEXT NOT NULL,
  from_addr TEXT, recipient TEXT, subject TEXT, body TEXT,
  last_error TEXT,
  created_at TEXT NOT NULL, sent_at TEXT);
CREATE TRIGGER coupon_outbox_immutable BEFORE UPDATE OF from_addr, recipient, subject, body ON coupon_outbox
WHEN OLD.recipient IS NOT NULL AND NEW.recipient IS NOT NULL
  AND (NEW.from_addr IS NOT OLD.from_addr OR NEW.recipient IS NOT OLD.recipient OR NEW.subject IS NOT OLD.subject OR NEW.body IS NOT OLD.body)
BEGIN SELECT RAISE(ABORT, 'coupon_outbox payload is immutable'); END;

-- Address of the last verified Apple/Google identity token (never client input). NULL = not verified: no email.
ALTER TABLE users ADD COLUMN notify_email TEXT;

-- Funding references. NULL = the user's own key as before.
ALTER TABLE connect_codes ADD COLUMN coupon_batch_id TEXT;
ALTER TABLE grants ADD COLUMN coupon_id TEXT REFERENCES coupons(id);
ALTER TABLE requests ADD COLUMN coupon_id TEXT REFERENCES coupons(id);
