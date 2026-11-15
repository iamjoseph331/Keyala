# MasterKey server

Rust (axum + sqlx/SQLite). One binary, one SQLite file. See `../docs/PLAN.md` for the contract.

## Run locally

```sh
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"   # if rust comes from Homebrew rustup
export MK_MASTER_KEY=$(openssl rand -base64 32)    # keep it: it encrypts every stored API key
MK_DEV_LOGIN=1 MK_VENDOR_MOCK=1 cargo run          # mock vendor: no real credits, no real keys
cargo test && cargo clippy --all-targets -- -D warnings
```

## Environment

| Var | Default | Meaning |
|---|---|---|
| `MK_MASTER_KEY` | required | base64 of 32 bytes; AES-256-GCM key for stored API keys |
| `PORT` | `8080` | listen port (Render sets it) |
| `MK_DATABASE_URL` | `sqlite://masterkey.db?mode=rwc` | SQLite file |
| `MK_PUBLIC_URL` | `http://localhost:8080` | base of `connect_url` |
| `MK_DEV_LOGIN` | off | `1` enables `POST /v1/auth/dev` (testing only) |
| `MK_PRICES_VERIFIED` | off | `1` = operator checked `src/pricing.rs` against vendor pricing pages. Without it (and without mock) every model is rejected with `model_not_priced` (fail closed) |
| `MK_APPLE_BUNDLE_ID` | `app.masterkey.ios` | required `aud` of Apple identity tokens |
| `MK_APPLE_JWKS_URL` | Apple's | override only for tests |
| `MK_APNS_KEY_P8`, `MK_APNS_KEY_ID`, `MK_APNS_TEAM_ID` | unset | APNs token auth (.p8 PEM contents; `\n` escapes ok). All three needed, otherwise push is a no-op |
| `MK_APNS_TOPIC` | `app.masterkey.ios` | `apns-topic` (bundle id) |
| `MK_APNS_SANDBOX` | off | `1` = api.sandbox.push.apple.com |
| `MK_APNS_DETAILED` | off | `1` puts app/vendor/model/estimate in the push text; default is a generic lock-screen message (never prompt text either way) |
| `MK_PUBLIC_APPS` | unset | JSON array of browser apps `[{"id":"app_x","name":"X","homepage":null,"allowed_origins":["https://x.github.io"]}]`, upserted on every start (survives the free-plan DB reset) so a static page can ship a fixed `app_id` |
| `MK_COUNTRY_HEADER` | unset | header your trusted edge sets to the client country (`cf-ipcountry` behind Cloudflare). The country of the phone approving a connect is stored as a 2-letter code (never the IP) for developer reports. Leave unset if nothing in front of the server sets it, or clients could forge it |
| `MK_VENDOR_MOCK` | off | `1` fakes vendor responses (a message containing `MK_MOCK_FAIL` yields a 500) |

`POST /v1/auth/apple` verifies Apple identity tokens; Google sign-in is not implemented. APNs push is optional (see env).

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
# 5. developer: call the model; X-MasterKey-Wait: 0 returns 202 immediately instead of holding
curl -s -i $B/v1/openai/chat/completions -H "Authorization: Bearer $G" -H "X-MasterKey-Wait: 0" \
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
# server running with MK_VENDOR_MOCK=1, plus two trust-mode grants (one per vendor)
MK_URL=http://localhost:8080 MK_GRANT_OPENAI=mk_grant_... MK_GRANT_ANTHROPIC=mk_grant_... \
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

- Tokens (`mk_app_`, `mk_grant_`, `mk_sess_`) are stored as SHA-256 hashes only. The grant token is
  generated and shown on the first poll after the user approves the connect code.
- "Approve with next N calls" = `count` policy with N-1 remaining after the approved call.
- Prices in `src/pricing.rs` are placeholders: verify against vendor pricing pages.
- Deploy: repo-root `render.yaml` (Render Blueprint): `main` -> `masterkey-prod`, `staging` -> `masterkey-staging`,
  both free plan, deployed only after Backend CI passes. Free plan has no disk, so SQLite data resets on every
  deploy/restart/spin-down; use `plan: starter` + a disk at `/data` to keep it. `MK_PUBLIC_URL` defaults to
  Render's `RENDER_EXTERNAL_URL`.
