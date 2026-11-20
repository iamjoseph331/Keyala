# Keyala developer docs

Keyala lets your app use a person's own OpenAI, Anthropic or OpenRouter API key without ever receiving it. The user pairs your app once, gets a scoped grant token to you, and approves calls on their phone (or sets a limit). You keep the official SDK and change one line, the base URL. This one file is the whole developer documentation, so you can also paste it into an AI coding assistant as context.

Hosted servers: production `https://app.keyala.io`, staging `https://staging.keyala.io`. Everything below works on either; use the same server for registration, pairing, calls and reports. Keyala is an experimental beta and free (see the last section for what that means).

## 1. What Keyala is and how a call flows

Your app asks Keyala to connect a user. The user approves on the Keyala iOS app, and your app receives a `grant_token` (bound to that user, your app and one provider). You send ordinary provider requests to Keyala with that token. Keyala checks the grant, reserves the estimated cost, asks the user if the policy needs it, then forwards the call to the provider with the user's stored key and returns the provider's own response.

```
your app --grant token--> Keyala server --(ask user if policy says so)--> user's phone
                              |  approved                                  approve / deny
                              v
                    provider (OpenAI / Anthropic / OpenRouter), user's key
                              |
your app <---- provider response, unchanged ---+
```

## 2. Quickstart (5 minutes)

### 2.1 Register an app

```sh
curl https://app.keyala.io/v1/apps \
  -H 'Content-Type: application/json' \
  -d '{"name":"My AI app","homepage":"https://yourapp.example"}'
```

```json
{ "app_id": "app_...", "app_secret": "keyala_app_..." }
```

`app_secret` is shown once. Store it in a server-side secret manager, never in browser code, a repository, a URL or logs. Registration is limited to 10 per hour per address (`429 rate_limited`). `name` is required (max 100 characters).

A browser-only app also sends `allowed_origins`: 1 to 10 exact origins such as `["https://yourapp.example","http://localhost:3000"]` (scheme and host, no path; `http://` only for localhost). The `app_id` is public and safe in page code; the secret is not. Rotating or losing the secret never exposes user keys.

### 2.2 Connect a user

Choose the path that matches where your code runs.

**Server app.** Use the app secret:

```sh
curl https://app.keyala.io/v1/connect \
  -H "Authorization: Bearer $KEYALA_APP_SECRET" \
  -H 'Content-Type: application/json' \
  -d '{"vendor":"openai"}'
```

```json
{
  "connect_code": "KY-4F7Q",
  "connect_url": "https://app.keyala.io/connect/KY-4F7Q",
  "connect_qr_url": "https://app.keyala.io/connect/KY-4F7Q/qr.svg",
  "expires_at": "2026-11-20T12:10:00.000Z"
}
```

`vendor` is `openai`, `anthropic` or `openrouter`. Show the QR (`connect_qr_url` is an SVG served by Keyala) or the link and code. The code lives 10 minutes. Poll with the same secret:

```sh
curl https://app.keyala.io/v1/connect/KY-4F7Q -H "Authorization: Bearer $KEYALA_APP_SECRET"
```

`status` is `pending` (with a `seen` hint: `null`, `"web"` = the link was opened in a browser so the app is probably not installed, `"app"` = the phone is reviewing), `approved`, `denied` or `expired`. Approval returns `{"status":"approved","grant_token":"keyala_grant_..."}` **exactly once**; save it before you poll again. If you lose it, run a new connect.

**Browser app.** Register with `allowed_origins`, then use the web SDK served by the same server. It opens a dialog (QR plus code on a computer, an "Open Keyala" button on a phone) and resolves with the grant:

```html
<script src="https://app.keyala.io/sdk/keyala.js"></script>
<script>
  let grant;
  async function ensureGrant() {
    grant ??= (await Keyala.connect({ app: "app_...", vendor: "openai" })).grant_token;
    return grant;
  }
</script>
```

`Keyala.connect({ app, vendor, lang? })` rejects with `err.code` of `cancelled`, `denied`, `unreachable` or `failed`. A button form is also available: `<keyala-connect app="app_..." vendor="openai"></keyala-connect>` fires a `connected` event with `detail.grant_token`. The SDK never stores the token; keep it in memory unless you have a deliberate storage design. Without the SDK: `POST /v1/connect` with `{"app_id":"app_...","vendor":"openai"}` and no Authorization header, from an allowed `Origin` (otherwise `403 origin_not_allowed`; 20 codes per 10 minutes per address). The response adds `connect_secret` (`keyala_cs_...`), which authorizes polling `GET /v1/connect/{code}`.

### 2.3 Make the call

The grant token is the API key; the base URL is the Keyala server. Disable SDK retries (see 2.4).

```python
from openai import OpenAI

client = OpenAI(base_url="https://app.keyala.io/v1/openai", api_key=GRANT_TOKEN, max_retries=0)
reply = client.chat.completions.create(
    model="gpt-4o-mini",
    max_tokens=256,
    messages=[{"role": "user", "content": "Hello!"}],
)
```

```python
from anthropic import Anthropic

client = Anthropic(base_url="https://app.keyala.io/v1/anthropic", api_key=GRANT_TOKEN, max_retries=0)
reply = client.messages.create(
    model="claude-haiku-5-5",
    max_tokens=256,
    messages=[{"role": "user", "content": "Hello!"}],
)
```

The Anthropic SDK appends `/v1/messages`; Keyala serves both `/v1/anthropic/messages` and `/v1/anthropic/v1/messages`, and accepts the grant token in `x-api-key` or as a bearer token on those routes. OpenRouter grants use the OpenAI request shape against `/v1/openrouter/chat/completions` (OpenAI SDK base URL `https://app.keyala.io/v1/openrouter`) with an exact slug such as `openai/gpt-4o-mini`.

Raw HTTP:

```sh
curl -i https://app.keyala.io/v1/openai/chat/completions \
  -H "Authorization: Bearer $KEYALA_GRANT_TOKEN" \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: job-2026-11-20-0001' \
  -H 'X-Keyala-Wait: 30' \
  -d '{"model":"gpt-4o-mini","max_tokens":256,"messages":[{"role":"user","content":"Hello!"}]}'
```

### 2.4 Pending approval, waiting and retries

When the policy needs the user's decision, Keyala creates a request, notifies the phone and **holds your HTTP call**. `X-Keyala-Wait` is the number of seconds to hold: default 60, maximum 120 (values above are capped; `0` returns immediately). Every proxy response carries `X-Keyala-Request-Id`.

| Result | Meaning |
|---|---|
| `200` | The provider response, unchanged. |
| `202` `{"request_id":"req_...","status":"pending"}` | Not decided yet (or still being sent). Poll `GET /v1/requests/{request_id}` with the same grant token. This is not a model response. |
| `403 keyala_denied` | The user denied it. |
| `410 keyala_expired` | The approval window passed. |
| `504 keyala_uncertain` | The provider may have received the call; it was not retried. Poll the same request id and never resubmit blindly. |
| provider error status | If the provider itself fails, you get its error body and status (502 if none). |

```sh
curl https://app.keyala.io/v1/requests/REQUEST_ID -H "Authorization: Bearer $KEYALA_GRANT_TOKEN"
```

```json
{ "request_id": "req_...", "status": "completed", "response": { "...": "provider response" } }
```

Statuses: `pending`, `approved`, `sending`, `completed`, `denied`, `expired`, `failed`, `uncertain`. Official SDKs may treat a 202 as an unexpected response; use raw HTTP or a raw-response API where you need to handle it.

**Retries.** Each stored request is executed at most once, but an HTTP client can resend a POST after a lost response. Either set `max_retries=0` (`maxRetries: 0`) or send an `Idempotency-Key` header (1 to 128 visible ASCII characters, unique per logical call, scoped to the grant). The same key with the same body returns the original outcome and sends nothing new; the same key with a different body is `409 idempotency_conflict`.

## 3. Concepts

**Grants and policies.** A grant links one user, your app and one provider. The user chooses a policy: `ask` (approve each call), `count` (a number of calls), `budget` (spend up to an amount), or `trust` (no per-call prompt, optional daily alert). A user's **first** grant for an app can only be approved as `ask`; the user can widen it later from Grants. Calls that fit the policy go straight through; others wait for approval. Users can revoke at any time (your next call returns `401 unauthorized`). App names and browser origins are developer supplied and are not verified identities; the phone shows the origin for browser apps.

**Estimates versus reported usage.** Before a call Keyala reserves a ceiling: the UTF-8 size of the request JSON as input tokens plus the output cap. It is a safety reservation, not a guarantee or a prediction. When the provider reports token usage, the real cost is computed from it. Without usage the actual cost stays unknown (never shown as zero). All figures are estimates of provider billing, not invoices.

**Output cap.** `max_tokens` is capped per model (4096 for the current models). If you omit it Keyala injects the default (1024). A value above the cap is `400 max_tokens_too_large`.

**Supported models.** Text only, allowlisted, priced in the server (`backend/src/pricing.rs`). Current list:

| Provider | Model names |
|---|---|
| OpenAI | `gpt-4o`, `gpt-4o-mini`, `gpt-4.1`, `gpt-4.1-mini` |
| Anthropic | `claude-opus-5-5`, `claude-sonnet-5-5`, `claude-haiku-5-5` |
| OpenRouter | `openai/gpt-4o`, `openai/gpt-4o-mini`, `openai/gpt-4.1`, `openai/gpt-4.1-mini`, `anthropic/claude-opus-5.5`, `anthropic/claude-sonnet-5.5`, `anthropic/claude-haiku-5.5` |

**Unsupported options are rejected, not ignored.** Keys outside an explicit allowlist return `400 unsupported_option` naming the key; images, audio, files and tools return `400 unsupported_content`; `stream: true` returns `400 streaming_unsupported`; `n` other than 1 is rejected. For OpenRouter, `models`, `provider` and `plugins` are rejected because they change the price.

**Retention.** Prompt and response content is encrypted at rest and removed 30 days after a request is decided (completed, failed, denied or expired); model, cost, status and time metadata stay. Pending, approved, sending and uncertain requests keep content until resolved.

**User's optional OpenRouter routing (`billing_vendor`).** A user can turn on, per account, "route OpenAI and Anthropic apps through OpenRouter". Your app is unaffected: same route, model name, body and response shape. At approval the grant freezes its `billing_vendor`; a routed call is billed on the user's OpenRouter key under the mapped slug and price. A model without a verified mapping fails `400 model_not_routable`, and there is no fallback to a direct key. A few options are also rejected when routed: OpenAI `max_completion_tokens`, `n` and role `developer`; Anthropic `metadata` and `top_k`. Existing grants never change; the user reconnects to change the route.

## 4. Errors

All errors are JSON: `{"error":{"type":"...","message":"..."}}`.

| `type` | HTTP | Meaning | What to do |
|---|---|---|---|
| `unauthorized` | 401 | Missing or invalid token, revoked grant, or a browser connect without an app secret or app id | Check the token; ask the user to reconnect if the grant was revoked |
| `invalid_credentials` | 401 | Wrong username or password (operator login only) | n/a for apps |
| `vendor_mismatch` | 403 | Grant is for another provider than the route | Call the route that matches the grant |
| `origin_not_allowed` | 403 | Browser origin is not in the app's `allowed_origins` | Register the exact origin |
| `keyala_denied` | 403 | The user denied the request | Stop; show it in your UI |
| `not_found` | 404 | Unknown request, connect code or grant | Check the id and token |
| `expired` | 410 | Connect code expired (user-side lookups) | Start a new connect |
| `keyala_expired` | 410 | Approval window passed | Let the user trigger a new action |
| `idempotency_conflict` | 409 | Same Idempotency-Key with a different body | Use a new key for a new job |
| `invalid_request` | 400 | Malformed JSON or field (empty `messages`, bad `max_tokens`, bad `name`, `days` outside 1 to 365) | Fix the request |
| `model_not_allowed` | 400 | Model not on the allowlist for that provider | Use a listed model |
| `model_not_priced` | 400 | Server has no verified price for the model | Use another model or server |
| `model_not_routable` | 400 | Routed grant, no OpenRouter mapping for the model | Use a mapped model |
| `max_tokens_too_large` | 400 | Above the per-model cap | Lower `max_tokens` |
| `unsupported_option` | 400 | A request key or value is not supported | Remove it |
| `unsupported_content` | 400 | Non-text content or tools | Send plain text only |
| `streaming_unsupported` | 400 | `stream: true` | Do not stream |
| `rate_limited` | 429 | Too many apps or connect codes from this address | Wait and retry later |
| `upstream_paused` | 503 | Operator paused provider calls | Try again later |
| `data_key_unreadable` | 503 | Server misconfiguration (wrong master key) | Operator issue |
| `keyala_uncertain` | 504 | Outcome of the provider call unknown | Poll the request id; never resubmit |
| `internal` | 500 | Server error | Retry the poll, not the call |

## 5. Dashboard and analytics

Open the Keyala console at `https://app.keyala.io/console` (or `https://staging.keyala.io/console`). Sign in by scanning its QR with the Keyala app and tapping **Approve web sign-in**, or with your account email and a password set in the app under **Settings > Web password**. A web session lasts 12 hours and can only read, revoke grants and manage your own apps; approving calls and changing keys stay on the phone.

Under **My apps** you can create an app (the secret is shown once), add an existing app by pasting its secret (the app becomes owned by your account), open each app's report for 7, 30 or 90 days (calls, cost from reported usage, completed calls with no usage reported, models, countries, a weekday x hour heatmap in your time zone) and rotate its secret. The older token-only page at `https://keyala.io/developers/` still works with just an app secret.

The same data is available from the API, with either the app secret or a signed-in account that owns the app:

| Endpoint | Auth | Purpose |
|---|---|---|
| `GET /v1/app/report?days=N` | `Bearer keyala_app_...` | report for the app the secret names |
| `GET /v1/me/apps` | session | your apps with 7-day calls and cost |
| `POST /v1/me/apps` `{name, homepage?, allowed_origins?}` | session | create an app you own; returns `app_secret` once |
| `POST /v1/me/apps/claim` `{app_secret}` | session | attach an existing unowned app (409 if owned by someone else) |
| `GET /v1/me/apps/{id}/report?days=N` | session | same shape as `/v1/app/report`; 404 if you do not own it |
| `POST /v1/me/apps/{id}/secret` | session | rotate; the old secret stops working at once |

```sh
curl 'https://app.keyala.io/v1/app/report?days=30' -H "Authorization: Bearer $KEYALA_APP_SECRET"
```

`days` is 1 to 365 UTC days, today included. Aggregates only: no prompts, responses, user ids, emails, IPs or keys.

```jsonc
{
  "app_id": "app_...", "days": 30, "from": "2026-10-22", "to": "2026-11-20",
  "totals": {
    "requests": 120, "completed": 100, "denied": 12, "expired": 2, "failed": 3, "uncertain": 1, "pending": 2,
    "cost_usd": 1.2345,        // from provider-reported usage, completed calls
    "estimated_usd": 2.1,      // pre-call estimates of approved calls
    "unmetered": 0,            // completed calls whose response had no usage
    "input_tokens": 90000, "output_tokens": 30000,
    "users": 14, "new_users": 6
  },
  "daily":     [{ "date": "2026-11-20", "requests": 7, "completed": 6, "cost_usd": 0.05 }],
  "models":    [{ "vendor": "openai", "model": "gpt-4o-mini", "requests": 80, "completed": 75, "cost_usd": 0.4, "input_tokens": 1, "output_tokens": 1 }],
  "hours_utc": [[0, 0, 3, "...24 values per row, 7 rows, row 0 = Sunday"]],
  "countries": [{ "country": "JP", "users": 5, "requests": 60, "cost_usd": 0.7 }]
}
```

`hours_utc` counts requests by UTC weekday and hour of creation; shift it to your own time zone when displaying. `country` is the 2-letter code of the user's phone when it approved the connection (from a trusted edge header; `null` when unavailable), not the location of your server. Rotate a leaked secret with `POST /v1/app/secret` (returns the new `app_secret`; the old one dies immediately).

## 6. API reference (developer and public endpoints)

Base URL: your Keyala server. Developer endpoints answer CORS `Access-Control-Allow-Origin: *` and use bearer tokens only.

| Method | Path | Auth | Purpose |
|---|---|---|---|
| POST | `/v1/apps` | none, rate limited | Register an app: `{name, homepage?, icon_url?, allowed_origins?}` returns `{app_id, app_secret}` |
| GET | `/v1/app` | app secret | App name, homepage, icon, `allowed_origins`, `created_at` |
| POST | `/v1/app/secret` | app secret | Rotate the secret |
| GET | `/v1/app/report?days=30` | app secret | Usage report (section 5) |
| POST | `/v1/connect` | app secret, or app id from an allowed browser origin | Start a user connection: `{vendor, suggested_policy?, app_id?}` |
| GET | `/v1/connect/{code}` | app secret or `connect_secret` | Poll; returns the grant token once on approval |
| GET | `/connect/{code}/qr.svg` | none | QR image of the connect link |
| GET | `/sdk/keyala.js` | none | Web SDK |
| POST | `/v1/openai/chat/completions` | grant | OpenAI-compatible text call |
| POST | `/v1/anthropic/messages` | grant (bearer or `x-api-key`) | Anthropic-compatible text call |
| POST | `/v1/anthropic/v1/messages` | grant (bearer or `x-api-key`) | Same, for the official Anthropic SDK |
| POST | `/v1/openrouter/chat/completions` | grant | OpenRouter call, OpenAI body shape |
| GET | `/v1/requests/{id}` | grant | Status and response of one of this grant's requests |
| GET | `/v1/status` | none | `{ok, paused, db, version}` |
| GET | `/healthz` | none | Liveness: `ok` |

The `/v1/me/*` endpoints belong to the Keyala user app (keys, approvals, grants, history, account). They use user sessions, have no CORS, and are not part of the developer API.

## 7. Self-hosting in brief

The server is one Rust binary with one SQLite file (source: `backend/` in the repository). A mock provider lets you develop with no real keys or credits.

```sh
export KEYALA_MASTER_KEY=$(openssl rand -base64 32)   # encrypts every stored key; keep it safe
KEYALA_DEV_LOGIN=1 KEYALA_VENDOR_MOCK=1 cargo run     # http://localhost:8080
```

| Variable | Meaning |
|---|---|
| `KEYALA_MASTER_KEY` | Required. Base64 of 32 bytes. Losing it loses all stored keys. |
| `PORT` (default 8080), `KEYALA_PUBLIC_URL` | Listen port; base of `connect_url`. |
| `KEYALA_DATABASE_URL` | SQLite file location. |
| `KEYALA_VENDOR_MOCK=1` | Fake provider responses (a message containing `KEYALA_MOCK_FAIL` yields a 500). |
| `KEYALA_DEV_LOGIN=1` | Test login endpoint; never on a public server. |
| `KEYALA_PRICES_VERIFIED=1` | Operator confirms the price table; otherwise unverified models fail closed. |
| `KEYALA_PUBLIC_APPS` | JSON list of browser apps (id, name, allowed origins) declared at start. |
| `KEYALA_COUNTRY_HEADER` | Trusted edge header carrying the client country, for report countries. |
| `KEYALA_PAUSE_UPSTREAM=1` | Pause new provider calls (also: a `PAUSE_UPSTREAM` file). |
| `KEYALA_BACKUP_DIR`, `KEYALA_BACKUP_KEEP`, `KEYALA_BACKUP_INTERVAL_SECS` | In-process SQLite backups. |

See `backend/README.md` for the full list and a curl walkthrough of the whole loop.

## 8. Limits and honesty

- Experimental beta, run by an individual. No SLA; the free hosted staging server may reset its data.
- Free during the beta, including the dashboard. Model usage is billed by the provider to the user's own account; a ChatGPT or Claude subscription does not include API credits.
- Costs are estimates and ceilings, not provider invoices. Missing usage means unknown, never zero.
- The server operator can decrypt a stored key in order to forward an approved call, so users trust the operator and the provider. Your app never receives the key.
- Text-only calls, no streaming, a fixed model allowlist; unsupported things fail loudly instead of being silently changed.
- Account deletion removes live keys, grants and history; backups roll off after a retention window instead of being erased instantly.
