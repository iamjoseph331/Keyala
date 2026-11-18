# MasterKey — plan

> Developers ship AI apps without paying for users' tokens. Users lend their API key without
> ever handing it over — and approve every call from their phone.

## The pitch in one diagram
```
 Developer's app ──(OpenAI/Anthropic-compatible call, grant token)──▶ MasterKey server
                                                                         │ cost estimate + policy check
                                          push / live event ◀────────────┤
 User's iPhone (MasterKey app): sees full payload + est. cost ──approve──▶│
                                                                         │ decrypts user's key, forwards
                                                         OpenAI / Anthropic ◀┘
                     response ──▶ developer (and recorded for the user to audit)
```
Developer DX goal: **change one line — the base URL** — and use the official OpenAI/Anthropic SDKs.

## Trust model (what we promise, honestly)
- User's key is write-only: encrypted at rest (AES-256-GCM, server master key from env), never
  returned by any API, never sent to the developer. Only `last4` is ever shown.
- The developer never sees the key; the user sees every payload and response.
- The user still trusts whoever runs the MasterKey server → server is open source and
  self-hostable (one binary + one SQLite file). That is the answer, and the README says so.

## Stack (decided)
- **Backend:** Rust, `axum` + `tokio` + `sqlx` + **SQLite** (single file, zero-ops, fits free
  hosting with a volume). `reqwest` for vendor calls, `aes-gcm` for key encryption.
  Postgres only if someone actually needs it.
- **iOS:** SwiftUI, iOS 17+, no third-party deps unless unavoidable. Polling + SSE for live
  updates; APNs when an Apple Developer account is available (see Open questions).
- **Site:** static GitHub Pages in `site/` (plain HTML/CSS, no build step), simulator screen
  recordings + screenshots as demo media.
- **Mock vendor:** `MK_VENDOR_MOCK=1` makes the server fake OpenAI/Anthropic responses so
  demos, tests and screenshots never burn real credits or expose real keys.

## Core flows
1. **Register app** — developer calls `POST /v1/apps` → `app_id` + `app_secret`.
2. **Connect a user (device-code style, no web UI needed)** — developer backend calls
   `POST /v1/connect` → short `connect_code` (e.g. `MK-4F7Q`) + `connect_url` (universal link /
   QR). User opens it in the iOS app, sees the app name + requested vendor, and approves the
   first grant for that app with `ask` (one phone approval per call). Afterward, the user can
   widen the policy from Grants. Developer polls `GET /v1/connect/{code}` → receives a `grant_token`.
3. **Call the model** — developer calls `POST /v1/openai/chat/completions` (or
   `/v1/anthropic/messages`) with `Authorization: Bearer mk_grant_…`. Server estimates cost,
   checks the grant policy:
   - allowed → forward immediately;
   - needs approval → create pending request, notify phone, **hold the HTTP call** up to
     `X-MasterKey-Wait` seconds (default 60, max 120). Approved in time → real vendor response.
     Timeout → `202 {request_id, status:"pending"}`; developer polls `GET /v1/requests/{id}`.
4. **Approve on phone** — options: *once*, *next N calls*, *up to $X*, *trust (with alert at
   $Y/day)*. Approval updates the grant policy. Deny → developer gets `403 masterkey_denied`.
5. **Audit** — every request stores payload, estimated + actual cost (from vendor `usage`),
   response, timestamps. User browses it any time.

### Cost & money safety (per Codex security review)
- Estimate is a **reservation ceiling, not a guarantee**: input tokens = UTF-8 byte length of the
  full normalized request JSON + 16 per message + 64 per request (1 token/byte is a safe upper
  bound for BPE tokenizers); output = `max_tokens`, **required and capped** per model (server
  injects a cap if missing). Text-only in v1: requests with images/audio/tools/files are rejected
  (`400 unsupported_content`).
- Models are **allowlisted** (static price table); unknown model → `400 model_not_allowed`.
  Billable options outside the allowlist (e.g. `n>1`, web search) → rejected.
- Internal money is **integer micro-USD** (`i64`). Public JSON `*_usd` floats are display only.
- Budget/count consumption is an **atomic reservation** in one SQL transaction before sending;
  settled to actual cost from vendor `usage` afterwards (refund the difference).
- Each request executes **at most once** (status transition `approved → sending` guarded in SQL).
  Timeout/network error after send → status `uncertain`, reserved amount kept, **never retried**.
- Provider URLs are fixed constants; nothing from the request picks the upstream host.
- A grant is bound to exactly one user + vendor at creation; the key used is always
  `grant.user_id`'s key — never selectable by the caller.

## Contract (v1) — source of truth for backend and iOS
All JSON, `snake_case`, timestamps RFC3339 UTC, money as `usd` floats (display only) .
Errors: `{"error":{"type":"...","message":"..."}}` with proper HTTP status.

### Objects
```jsonc
Policy   { "mode": "ask" | "count" | "budget" | "trust",
           "remaining_count": 5,          // mode=count
           "remaining_usd": 2.50,         // mode=budget
           "alert_usd_per_day": 1.00,     // mode=trust (optional elsewhere)
           "expires_at": null }
App      { "id", "name", "homepage", "icon_url" }
Grant    { "id", "app": App, "vendor": "openai"|"anthropic"|"openrouter", "policy": Policy,
           "created_at", "last_used_at", "spent_usd", "request_count" }
Request  { "id", "grant_id", "app": App, "vendor", "model", "endpoint",
           "status": "pending"|"approved"|"sending"|"denied"|"expired"|"completed"|"failed"|"uncertain",
           "estimated_usd", "actual_usd", "created_at", "decided_at", "completed_at",
           "payload": {...},            // full request body (detail view only)
           "response": {...} }          // full vendor response (detail view only)
Key      { "vendor", "last4", "created_at" }
```

### Developer-facing
| Method | Path | Auth | Notes |
|---|---|---|---|
| POST | `/v1/apps` | none (rate-limited) | `{name, homepage?, icon_url?, allowed_origins?}` → `{app_id, app_secret}` |
| POST | `/v1/connect` | app secret, or browser | `{vendor, suggested_policy?}` → `{connect_code, connect_url, connect_qr_url, expires_at}` (10 min). `connect_url` = `{server}/connect/{code}` (universal link; a page with the QR on a computer), `connect_qr_url` = an SVG QR of it for the developer's page. Browser: `{app_id, vendor, suggested_policy?}`, no auth, `Origin` must be in the app's `allowed_origins` (else `403 origin_not_allowed`; 20/10 min per IP) → also `connect_secret` (`mk_cs_…`) |
| GET | `/v1/connect/{code}` | app secret or `connect_secret` | `{status: pending\|approved\|denied\|expired, grant_token?}` |
| POST | `/v1/openai/chat/completions` | grant | OpenAI body passthrough; vendor response verbatim |
| POST | `/v1/anthropic/messages` | grant | Anthropic body passthrough; vendor response verbatim |
| POST | `/v1/openrouter/chat/completions` | grant | OpenAI-shaped body for vendor `openrouter`; exact slugs from the allowlist only (`openai/gpt-4o-mini`, `anthropic/claude-haiku-5.5`, ...); OpenRouter's `models` fallback, `provider` routing and `plugins` are rejected (`unsupported_option`) because they change the price |
| GET | `/v1/requests/{id}` | grant | `{status, response?}` |
| GET | `/v1/app` | app secret | `{app_id, name, homepage, icon_url, allowed_origins, created_at}` |
| POST | `/v1/app/secret` | app secret | rotate: `{app_secret}`; the old secret stops working at once |
| GET | `/v1/app/report?days=30` | app secret | usage report for the last `days` (1-365, UTC days, today included), see below |

**Browser (public) clients:** an app registered with `allowed_origins` (1-10 origins like `https://x.github.io`,
`http://` only for localhost; or declared by the operator in `MK_PUBLIC_APPS`) can run entirely in a static page:
connect with its `app_id`, claim the grant with `connect_secret`, keep the grant in the browser. Developer endpoints
(`/v1/apps`, `/v1/app*`, `/v1/connect*`, proxy, and `/v1/requests/*`) answer CORS with
`Access-Control-Allow-Origin: *`. Authenticated developer routes use bearer tokens. `/v1/me/*` has no CORS.
The browser pairing preview shows `origin` because only browsers enforce that header.

**App secrets survive a database reset:** `app_secret` is opaque and carries the app record sealed
(AES-GCM) with the server's master key. If the server's database was wiped (Render free plan) but the master key
is unchanged, the first call with the secret re-creates the app with its name, homepage and origins. Rotation bumps a
generation: after a reset, a rotated secret supersedes older ones as soon as it is used. User grants, keys and report
history are not recoverable this way; users reconnect.

**Developer report** (`GET /v1/app/report`), aggregates only, never user ids, emails, prompts or responses:
```jsonc
{ "app_id", "days": 30, "from": "2026-10-17", "to": "2026-11-15",
  "totals":  { "requests", "completed", "denied", "expired", "failed", "uncertain", "pending",
               "cost_usd",        // sum of vendor-reported cost of completed calls
               "estimated_usd",   // sum of pre-call estimates of every approved call
               "unmetered",       // completed calls whose vendor response had no usage
               "input_tokens", "output_tokens", "users", "new_users" },
  "daily":     [{ "date": "2026-11-15", "requests", "completed", "cost_usd" }],      // every day, zeros included
  "models":    [{ "vendor", "model", "requests", "completed", "cost_usd", "input_tokens", "output_tokens" }],
  "countries": [{ "country": "JP" | null, "users", "requests", "cost_usd" }] }  // most requests first
```
`country` is ISO 3166-1 alpha-2 of the user's phone when they approved the connect (from the header the
operator's trusted edge sets, `MK_COUNTRY_HEADER`, e.g. `cf-ipcountry` on Render; null when unset or absent). Only the code is stored, never
the IP. `users` = distinct users with a request in range; `new_users` = users whose first grant for this app was created in range.

Proxy responses carry `X-MasterKey-Request-Id`. Streaming (`stream: true`) → `400` in v1.
Optional `Idempotency-Key` header (≤128 chars, unique per grant): a repeat with the same body returns the
original request's outcome without sending again; a different body → `409 idempotency_conflict`.
Disable SDK retries or use this header; after `202`/`504` poll `GET /v1/requests/{id}`, never resubmit.

### User-facing (iOS app)
| Method | Path | Notes |
|---|---|---|
| POST | `/v1/auth/apple` / `/v1/auth/google` | `{id_token}` → `{session_token, user}` |
| POST | `/v1/auth/password` | only if `MK_PASSWORD_LOGIN` is set (one operator-provisioned account, a fallback while Apple sign-in is unavailable): `{username, password}` → `{session_token, user}`; 404 unconfigured, 401 `invalid_credentials`, 429 `rate_limited` (10 per 10 min per address). Identity `{username}@password.invalid`, never linked to Apple or email accounts |
| POST | `/v1/auth/dev` | only if `MK_DEV_LOGIN=1`: `{email}` → `{session_token, user}` |
| GET | `/v1/me/keys` | `[Key]` |
| PUT | `/v1/me/keys/{vendor}` | `{api_key}` → `Key` (key never returned). 400 `invalid_key` if the shape is wrong for the vendor (OpenAI `sk-…` not `sk-ant-`, Anthropic `sk-ant-…`; admin keys `sk-admin-`/`sk-ant-admin` refused) or the vendor's free `GET /v1/models` answers 401. A 403 (e.g. a restricted key without the models-list permission) or an unreachable vendor = saved with `checked: false`; a 200 = `checked: true` (field added to this response and to enrollment upload). Mock: a key containing `MK_MOCK_INVALID` is rejected |
| DELETE | `/v1/me/keys/{vendor}` | |
| POST | `/v1/me/devices` | `{apns_token}` |
| GET | `/v1/me/connect/{code}` | preview `{app, vendor, suggested_policy, expires_at, origin, requires_ask}` (`origin` null for server apps; `requires_ask` is true when this user has no prior grant for this app) |
| POST | `/v1/me/connect/{code}/approve` | `{policy}` → `Grant` |
| POST | `/v1/me/connect/{code}/deny` | |
| GET | `/v1/me/grants` | `[Grant]` |
| PATCH | `/v1/me/grants/{id}` | `{policy}` → `Grant` |
| DELETE | `/v1/me/grants/{id}` | revoke |
| GET | `/v1/me/requests?status=&limit=&before_id=` | `[Request]` without payload/response; newest first by created_at DESC,id DESC. Optional before_id is an exclusive cursor scoped to this user (foreign/missing ID: 404). |
| GET | `/v1/me/requests/{id}` | full `Request` |
| POST | `/v1/me/requests/{id}/approve` | `{policy?}` (omit = once) → `Request` |
| POST | `/v1/me/requests/{id}/deny` | → `Request` |
| GET | `/v1/me/events` | SSE: `request.created`, `request.updated`, `grant.updated` (data = object JSON) |

### One-tap OpenRouter (OAuth PKCE, run by the server)
OpenAI and Anthropic have no API that creates a personal key; OpenRouter does. The server keeps the PKCE verifier,
so the new key goes from OpenRouter to the server and never through the phone.
| Method | Path | Auth | Notes |
|---|---|---|---|
| POST | `/v1/me/oauth/openrouter` | session | `{limit_usd?}` → `{flow_id, auth_url, expires_at}` (10 min). `limit_usd` prefills OpenRouter's spending cap (the user can change it). Open `auth_url` in `ASWebAuthenticationSession` with callback scheme `masterkey` |
| GET | `/v1/me/oauth/openrouter/flows/{flow_id}` | session (owner only, else 404) | `{flow_id, provider, status: pending\|exchanging\|saved\|failed\|cancelled\|expired, reason, last4, expires_at}`. This, not the callback URL, is the outcome |
| POST | `/v1/me/oauth/openrouter/flows/{flow_id}/cancel` | session (owner only) | pending → `cancelled` (later callbacks for it do nothing); other states unchanged; returns the flow |
| GET | `/oauth/openrouter/callback` | single-use `state` | OpenRouter redirects here; pending → exchanging exactly once, the server exchanges the code (verifier deleted first), stores the key as vendor `openrouter`, emits SSE `key.updated`, and answers a page that redirects to `masterkey://oauth/openrouter?status=ok&flow={flow_id}` or `?status=error&reason=…[&flow=…]`. No key or last4 in the URL. Declined at OpenRouter = no state, so `reason=cancelled` without `flow`: the app cancels its own flow. A crash mid-exchange marks the flow `failed`/`interrupted` on restart (codes are single use; never retried) |
Mock mode: `auth_url` is the server's own `/oauth/openrouter/mock`, which approves at once (key `sk-or-v1-mock…`), so the
whole loop runs locally. OpenRouter keys can also be pasted (`sk-or-…`, checked with the free `GET /api/v1/key`).

### Add a key from a computer (enrollment)
The user is usually signed in to the provider's console on a computer, so the key is pasted there,
on the server's own page `{server}/add-key` (same origin as the API, no CORS; CSP, no-referrer, no-store).
| Method | Path | Auth | Notes |
|---|---|---|---|
| POST | `/v1/enroll` | none (10/10 min per IP) | `{vendor}` → `{enroll_id, enroll_url, qr_url, enroll_secret, expires_at}` (10 min). `enroll_id` = `en_` + 22 random chars (unguessable; QR only, no typed code). `enroll_secret` (`mk_es_…`) stays in the page's memory |
| GET | `/v1/enroll/{id}` | `enroll_secret` | `Enrollment` (the page polls) |
| POST | `/v1/enroll/{id}/key` | `enroll_secret` | `{api_key}` → `Enrollment`; only while `claimed`, exactly once (409 `not_claimed` otherwise); same checks as PUT keys (400 `invalid_key`; response adds `checked`). The candidate is then immutable |
| GET | `/v1/me/enroll/{id}` | session | `Enrollment` + `current_last4` (the key it would replace, or null). Any signed-in user while `pending`, afterwards only the claimer (else 404) |
| POST | `/v1/me/enroll/{id}/claim` | session | empty body → `Enrollment`; `pending` → `claimed`, binds this user; 409 `not_pending` if already claimed/expired |
| POST | `/v1/me/enroll/{id}/confirm` | session | empty body → `Key`; `submitted` → `saved` and the key stored, in one transaction; 409 `not_submitted`. Single use |
| POST | `/v1/me/enroll/{id}/cancel` | session | → 204; `claimed`/`submitted` → `cancelled`, candidate wiped |
`Enrollment { id, vendor, status: pending|claimed|submitted|saved|cancelled|expired, last4, expires_at }`.
SSE `enroll.updated` (data = Enrollment) goes to the claimer. Uploaded candidates are AES-GCM encrypted
(AAD `enroll|{id}`) and wiped on save, cancel or expiry. Wrong key after upload = cancel and start again.
`/enroll/{id}` (universal link, page with an "Open in app" link) and `/enroll/{id}/qr.svg`, `/connect/{code}/qr.svg`.
`/.well-known/apple-app-site-association` covers `/connect/*` and `/enroll/*` for `{MK_APPLE_TEAM_ID}.{MK_APPLE_BUNDLE_ID}`
(default `83APHP8F76.app.sorevan.masterkey`). Custom scheme `masterkey://connect/{code}` and `masterkey://enroll/{id}` remain.

Auth header everywhere: `Authorization: Bearer <token>`. Token prefixes: `mk_app_`, `mk_grant_`,
`mk_sess_` (so a leaked token is identifiable).

**First-grant protection:** For a user and app pair, the first grant can only be approved with
policy mode `ask`. Any earlier grant counts, including revoked grants and grants for another
vendor. Other policy modes return `400 policy_not_allowed_yet`; the pending connect remains
available to retry with `ask`. After approval, the user can deliberately widen the policy with
`PATCH /v1/me/grants/{id}`. The preview's `requires_ask` reflects this same user+app history rule;
approval checks it again transactionally, so the preview is guidance and the server remains
authoritative if another grant is created concurrently.

## Milestones
- **M0** plan + contract (this file) — both agents ACK.
- **M1 backend core** — dev login, keys, grants, requests, proxy with hold/poll, mock vendor,
  SSE. `cargo test` covers the policy + approval path end to end.
- **M2 iOS core** — dev login, add key, live pending list, approve sheet (payload + est. cost +
  4 scopes), history + detail (payload/response), grants list with revoke.
- **M3 connect pairing** — QR / universal link + code entry; `sdk/` example app ("Chat with your
  own key") demonstrating the full loop.
- **M4 real-world** — Sign in with Apple, APNs (owner has a paid Apple Developer account),
  Dockerfile + repo-root `render.yaml`: free-plan `masterkey-prod` (branch `main`) and
  `masterkey-staging` (branch `staging`), real vendors, deploy after CI passes. No disk on free plan:
  data resets on every deploy/restart.
- **M5 launch page** — `site/` with hero, 30-s demo video (simulator recording of the example
  app + phone approval side by side), quickstart, trust model. README mirrors it.

## Open questions (owner)
Answered 2026-11-14: paid Apple Developer account = yes; login = Sign in with Apple + dev
login (Google later); hosting = local + Docker, owner deploys to Render; ownership split approved.
