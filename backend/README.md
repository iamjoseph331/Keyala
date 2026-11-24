# Keyala server

Rust (axum + sqlx/SQLite). One binary, one SQLite file. See `../docs/PLAN.md` for the contract.

## Run locally

```sh
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"   # if rust comes from Homebrew rustup
export KEYALA_MASTER_KEY=$(openssl rand -base64 32)    # keep it: it encrypts every stored API key
KEYALA_DEV_LOGIN=1 KEYALA_VENDOR_MOCK=1 cargo run          # mock vendor: no real credits, no real keys
cargo test && cargo clippy --all-targets -- -D warnings
```

## Environment

| Var | Default | Meaning |
|---|---|---|
| `KEYALA_MASTER_KEY` | required | base64 of 32 bytes; AES-256-GCM key for stored API keys |
| `PORT` | `8080` | listen port (Render sets it) |
| `KEYALA_DATABASE_URL` | `sqlite://masterkey.db?mode=rwc` | SQLite file |
| `KEYALA_PUBLIC_URL` | `http://localhost:8080` | base of `connect_url` |
| `KEYALA_DEV_LOGIN` | off | `1` enables `POST /v1/auth/dev` (testing only) |
| `KEYALA_PASSWORD_LOGIN` | unset | `username:sha256hex(password)` enables `POST /v1/auth/password` for that one account. Use a random password, e.g. `P=$(openssl rand -base64 24); printf 'qa:%s' "$(printf %s "$P" \| shasum -a 256 \| cut -d' ' -f1)"`. Keep `$P` out of git and logs |
| `KEYALA_PRICES_VERIFIED` | off | `1` = operator checked `src/pricing.rs` against vendor pricing pages. Without it (and without mock) every model is rejected with `model_not_priced` (fail closed) |
| `KEYALA_APPLE_BUNDLE_ID` | `app.sorevan.masterkey` | required `aud` of Apple identity tokens |
| `KEYALA_APPLE_JWKS_URL` | Apple's | override only for tests |
| `KEYALA_GOOGLE_CLIENT_IDS` | unset | comma-separated Google OAuth client IDs (iOS, later Android/web) accepted as ID-token `aud`. Unset = `POST /v1/auth/google` answers 404 and `/v1/auth/methods` says `google: false` |
| `KEYALA_GOOGLE_JWKS_URL` | Google's | override only for tests |
| `KEYALA_PASSKEY_RP_ID` | host of `KEYALA_PUBLIC_URL` | WebAuthn RP ID passkeys are bound to. Passkeys are offered only when `KEYALA_PUBLIC_URL` is `https://` on this host. Changing it orphans every stored passkey |
| `KEYALA_PASSKEY_ORIGINS` | unset | extra accepted origins, comma separated (`https://{rp_id}` is always accepted) |
| `KEYALA_ANDROID_PACKAGE`, `KEYALA_ANDROID_CERT_SHA256` | unset | Android app for `/.well-known/assetlinks.json` and its passkey origin; fingerprints as `AB:CD:…`, comma separated. Both or neither |
| `KEYALA_APNS_KEY_P8`, `KEYALA_APNS_KEY_ID`, `KEYALA_APNS_TEAM_ID` | unset | APNs token auth (.p8 PEM contents; `\n` escapes ok). All three needed, otherwise push is a no-op |
| `KEYALA_APNS_TOPIC` | `app.sorevan.masterkey` | `apns-topic` (bundle id) |
| `KEYALA_APNS_SANDBOX` | off | `1` = api.sandbox.push.apple.com |
| `KEYALA_APNS_DETAILED` | off | `1` puts app/vendor/model/estimate in the push text; default is a generic lock-screen message (never prompt text either way) |
| `KEYALA_PUBLIC_APPS` | unset | JSON array of browser apps `[{"id":"app_x","name":"X","homepage":null,"allowed_origins":["https://x.github.io"]}]`, upserted on every start (survives the free-plan DB reset) so a static page can ship a fixed `app_id` |
| `KEYALA_COUNTRY_HEADER` | unset | header your trusted edge sets to the client country (`cf-ipcountry` behind Cloudflare). The country of the phone approving a connect is stored as a 2-letter code (never the IP) for developer reports. Leave unset if nothing in front of the server sets it, or clients could forge it |
| `KEYALA_VENDOR_MOCK` | off | `1` fakes vendor responses (a message containing `KEYALA_MOCK_FAIL` yields a 500) |

`POST /v1/auth/apple` and `POST /v1/auth/google` verify Apple / Google identity tokens. A Google sign-in is its own
account (Google `sub`), never linked to an Apple account by email. Passkeys (`/v1/me/passkeys`, `/v1/auth/passkey`) need
fresh identity to add and work for any account; the app needs `webcredentials:{host}`, which this server's
`apple-app-site-association` declares. `GET /v1/auth/methods` tells clients which of these the server offers. APNs push is optional (see env).

## Full loop with curl

```sh
B=http://localhost:8080
# 1. user: dev login, store a key (write-only; only last4 is ever returned)
S=$(curl -s $B/v1/auth/dev -d '{"email":"me@example.com"}' | jq -r .session_token)
curl -s -X PUT $B/v1/me/keys/openai -H "Authorization: Bearer $S" -d '{"api_key":"sk-demo-1234567890"}'
# 2. developer: register an app, start a connect
APP=$(curl -s $B/v1/apps -d '{"name":"Demo app"}' | jq -r .app_secret)
CODE=$(curl -s $B/v1/connect -H "Authorization: Bearer $APP" -d '{"vendor":"openai"}' | jq -r .connect_code)
# 3. user: approve the connection with a policy (ask | count | budget | trust)
curl -s $B/v1/me/connect/$CODE/approve -H "Authorization: Bearer $S" -d '{"policy":{"mode":"ask"}}'
# 4. developer: pick up the grant token (shown once)
G=$(curl -s $B/v1/connect/$CODE -H "Authorization: Bearer $APP" | jq -r .grant_token)
# 5. developer: call the model; X-Keyala-Wait: 0 returns 202 immediately instead of holding
curl -s -i $B/v1/openai/chat/completions -H "Authorization: Bearer $G" -H "X-Keyala-Wait: 0" \
  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"hi"}]}'
# 6. user: see the pending request and approve it (optionally with a new policy)
RID=$(curl -s "$B/v1/me/requests?status=pending" -H "Authorization: Bearer $S" | jq -r '.[0].id')
curl -s -X POST $B/v1/me/requests/$RID/approve -H "Authorization: Bearer $S" -d '{}'
# 7. developer: poll the result
curl -s $B/v1/requests/$RID -H "Authorization: Bearer $G"
```

## Official SDKs

Set `base_url` to `{host}/v1/openai` (OpenAI) or `{host}/v1/anthropic` (Anthropic) and use the grant
token as the API key. The Anthropic routes also accept the grant token in `x-api-key`.

```sh
python3 -m venv /tmp/mk-venv && /tmp/mk-venv/bin/pip install openai anthropic
# server running with KEYALA_VENDOR_MOCK=1, plus two trust-mode grants (one per vendor)
KEYALA_URL=http://localhost:8080 KEYALA_GRANT_OPENAI=keyala_grant_... KEYALA_GRANT_ANTHROPIC=keyala_grant_... \
  /tmp/mk-venv/bin/python tests/sdk_smoke.py
```

## Retries and idempotency

Every proxy call is executed at most once, but your HTTP client may re-send a request it thinks failed.
Either **disable SDK retries** (`max_retries=0` / `maxRetries: 0`) or **send an `Idempotency-Key`**
header (<= 128 chars, unique per logical call, scoped to the grant). A repeated POST with the same key
and body returns the original request's outcome and sends nothing new; the same key with a different
body is `409 idempotency_conflict`. After a `202` (held) or `504` (uncertain), poll
`GET /v1/requests/{id}`: never resubmit.

## Notes

- Tokens (`keyala_app_`, `keyala_grant_`, `keyala_sess_`) are stored as SHA-256 hashes only. The grant token is
  generated and shown on the first poll after the user approves the connect code.
- "Approve with next N calls" = `count` policy with N-1 remaining after the approved call.
- Prices in `src/pricing.rs` are placeholders: verify against vendor pricing pages.
- Deploy: repo-root `render.yaml` (Render Blueprint): `main` -> `masterkey-prod`, `staging` -> `masterkey-staging`,
  both free plan, deployed only after Backend CI passes. Free plan has no disk, so SQLite data resets on every
  deploy/restart/spin-down; use `plan: starter` + a disk at `/data` to keep it. `KEYALA_PUBLIC_URL` defaults to
  Render's `RENDER_EXTERNAL_URL`. With a custom domain (e.g. `app.keyala.io`), set `KEYALA_PUBLIC_URL` to it: it is
  also the passkey RP ID, and the iOS app only accepts an RP ID equal to the server host it talks to.
