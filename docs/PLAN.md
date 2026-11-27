# Keyala — plan

> Developers ship AI apps without paying for users' tokens. Users lend their API key without
> ever handing it over — and approve every call from their phone.

## The pitch in one diagram
```
 Developer's app ──(OpenAI/Anthropic-compatible call, grant token)──▶ Keyala server
                                                                         │ cost estimate + policy check
                                          push / live event ◀────────────┤
 User's iPhone (Keyala app): sees full payload + est. cost ──approve──▶│
                                                                         │ decrypts user's key, forwards
                                                         OpenAI / Anthropic ◀┘
                     response ──▶ developer (and recorded for the user to audit)
```
Developer DX goal: **change one line — the base URL** — and use the official OpenAI/Anthropic SDKs.

## Trust model (what we promise, honestly)
- User's key is write-only: encrypted at rest (AES-256-GCM, server master key from env), never
  returned by any API, never sent to the developer. Only `last4` is ever shown.
- The developer never sees the key; the user sees every payload and response.
- Payloads and responses are encrypted at rest with a per-user data key (see § Storage at rest) and
  kept 30 days, then removed; cost, model and token counts stay.
- The user still trusts whoever runs the Keyala server → server is open source and
  self-hostable (one binary + one SQLite file). That is the answer, and the README says so.

## Stack (decided)
- **Backend:** Rust, `axum` + `tokio` + `sqlx` + **SQLite** (single file, zero-ops, fits free
  hosting with a volume). `reqwest` for vendor calls, `aes-gcm` for key encryption.
  Postgres only if someone actually needs it.
- **iOS:** SwiftUI, iOS 17+, no third-party deps unless unavoidable. Polling + SSE for live
  updates; APNs when an Apple Developer account is available (see Open questions).
- **Site:** static GitHub Pages in `site/` (plain HTML/CSS, no build step), simulator screen
  recordings + screenshots as demo media.
- **Mock vendor:** `KEYALA_VENDOR_MOCK=1` makes the server fake OpenAI/Anthropic responses so
  demos, tests and screenshots never burn real credits or expose real keys.

## Core flows
1. **Register app** — developer calls `POST /v1/apps` → `app_id` + `app_secret`.
2. **Connect a user (device-code style, no web UI needed)** — developer backend calls
   `POST /v1/connect` → short `connect_code` (e.g. `KY-4F7Q`) + `connect_url` (universal link /
   QR). User opens it in the iOS app, sees the app name + requested vendor, and approves the
   first grant for that app with `ask` (one phone approval per call). Afterward, BYOK grants can
   widen the policy from Grants; Coupon grants remain `ask`. Developer polls `GET /v1/connect/{code}` → receives a `grant_token`.
3. **Call the model** — developer calls `POST /v1/openai/chat/completions` (or
   `/v1/anthropic/messages`) with `Authorization: Bearer keyala_grant_…`. Server estimates cost,
   checks the grant policy:
   - allowed → forward immediately;
   - needs approval → create pending request, notify phone, **hold the HTTP call** up to
     `X-Keyala-Wait` seconds (default 60, max 120). Approved in time → real vendor response.
     Timeout → `202 {request_id, status:"pending"}`; developer polls `GET /v1/requests/{id}`.
4. **Approve on phone** — options: *once*, *next N calls*, *up to $X*, *trust (with alert at
   $Y/day)*. Approval updates the grant policy. Deny → developer gets `403 keyala_denied`.
5. **Audit** — every request stores payload, estimated + actual cost (from vendor `usage`),
   response, timestamps. The user browses it any time; payload and response are kept 30 days after
   the request is decided, then removed (`purged: true`), metadata stays.

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
- A grant is bound to exactly one user + app + vendor and a fixed funding source at connection approval:
  the user's key for BYOK, or its Coupon batch's dedicated key for sponsored calls. The caller cannot
  select a different key per request; sponsored calls never fall back to the user's key.

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
           "purged": false,             // true = payload/response removed by the 30-day retention
           "payload": {...},            // full request body (detail view only; null when purged)
           "response": {...} }          // full vendor response (detail view only; null when purged)
Key      { "vendor", "last4", "created_at" }
```

### Storage at rest (SQLite)
- One SQLite file, `secure_delete` on (deleted rows are zeroed in the file, not left in free pages).
- Tokens (session, grant, app, connect, enroll, OAuth state) are stored only as SHA-256 hashes.
- Provider keys: AES-256-GCM with the master key (`KEYALA_MASTER_KEY`, env, never in the DB), AAD `user|vendor`.
- Each user has a data key (32 random bytes) stored wrapped with the master key (AAD `dek|{user_id}`).
  `requests.payload` and `requests.response` are `enc1:` + base64url(nonce‖ciphertext) under it, AAD
  `req|{id}|payload` / `req|{id}|response`. Rows from before migration 0013 are encrypted at the next start.
- Token counts are columns (`input_tokens`, `output_tokens`), so reports never read content.
- Retention: content of `completed|failed|denied|expired` requests is removed 30 days after the decision
  (`purged=1`); `pending`, `approved`, `sending`, `uncertain` keep theirs.
- Plaintext columns: emails, Apple and Google `sub`, passkey credential IDs and public keys (not secrets), APNs tokens, app names/origins, connect codes, policies, costs, models, timestamps, country codes.
- Wrong master key for a database: requests and history answer `503 data_key_unreadable` and nothing is written.
- Backups (`VACUUM INTO`) hold the same encrypted content; they are NOT crypto-erased on deletion, because
  the wrapped data key is in them too and the master key opens it. They roll off after `KEYALA_BACKUP_KEEP` days.

### Developer-facing
| Method | Path | Auth | Notes |
|---|---|---|---|
| POST | `/v1/apps` | none (rate-limited) | `{name, homepage?, icon_url?, allowed_origins?}` → `{app_id, app_secret}` |
| POST | `/v1/connect` | app secret, or browser | `{vendor, suggested_policy?}` → `{connect_code, connect_url, connect_qr_url, expires_at}` (10 min). `connect_url` = `{server}/connect/{code}` (universal link; a page with the QR on a computer), `connect_qr_url` = an SVG QR of it for the developer's page. Browser: `{app_id, vendor, suggested_policy?}`, no auth, `Origin` must be in the app's `allowed_origins` (else `403 origin_not_allowed`; 20/10 min per IP) → also `connect_secret` (`keyala_cs_…`) |
| GET | `/v1/connect/{code}` | app secret or `connect_secret` | `{status: pending\|approved\|denied\|expired, grant_token?, seen?}`. While pending, `seen` is `null`, `"web"` (the `/connect/{code}` page was opened in a browser, so the app is probably not installed) or `"app"` (the app opened the review). A hint for the pairing UI only, never authority |
| GET | `/sdk/keyala.js` | none | the web SDK (`Keyala.connect({app, vendor})` dialog and `<keyala-connect>` button), served by each server and talking to the server it was loaded from; see sdk/web/README.md |
| POST | `/v1/openai/chat/completions` | grant | OpenAI body passthrough; vendor response verbatim |
| POST | `/v1/anthropic/messages` | grant | Anthropic body passthrough; vendor response verbatim |
| POST | `/v1/openrouter/chat/completions` | grant | OpenAI-shaped body for vendor `openrouter`; exact slugs from the allowlist only (`openai/gpt-4o-mini`, `anthropic/claude-haiku-5.5`, ...); OpenRouter's `models` fallback, `provider` routing and `plugins` are rejected (`unsupported_option`) because they change the price |
| GET | `/v1/requests/{id}` | grant | `{status, response?}` |
| GET | `/v1/app` | app secret | `{app_id, name, homepage, icon_url, allowed_origins, created_at}` |
| POST | `/v1/app/secret` | app secret | rotate: `{app_secret}`; the old secret stops working at once |
| GET | `/v1/app/report?days=30` | app secret | usage report for the last `days` (1-365, UTC days, today included), see below |

**Browser (public) clients:** an app registered with `allowed_origins` (1-10 origins like `https://x.github.io`,
`http://` only for localhost; or declared by the operator in `KEYALA_PUBLIC_APPS`) can run entirely in a static page:
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
  "hours_utc": [[0, 0, 3, ...24], ...7],   // requests by UTC weekday (row 0 = Sunday) x hour (0-23), zeros included
  "countries": [{ "country": "JP" | null, "users", "requests", "cost_usd" }] }  // most requests first
```
`country` is ISO 3166-1 alpha-2 of the user's phone when they approved the connect (from the header the
operator's trusted edge sets, `KEYALA_COUNTRY_HEADER`, e.g. `cf-ipcountry` on Render; null when unset or absent). Only the code is stored, never
the IP. `hours_utc` is when calls were made (request creation time, UTC); the console shifts it to the viewer's time zone. It reveals nothing a developer's own server does not already see, since every call passes through it. `users` = distinct users with a request in range; `new_users` = users whose first grant for this app was created in range.

Proxy responses carry `X-Keyala-Request-Id`. Streaming (`stream: true`) → `400` in v1.
Optional `Idempotency-Key` header (≤128 chars, unique per grant): a repeat with the same body returns the
original request's outcome without sending again; a different body → `409 idempotency_conflict`.
Disable SDK retries or use this header; after `202`/`504` poll `GET /v1/requests/{id}`, never resubmit.

### User-facing (iOS app)
| Method | Path | Notes |
|---|---|---|
| POST | `/v1/auth/apple` | `{id_token}` → `{session_token, user}` |
| GET | `/v1/auth/methods` | no auth → `{google: bool, passkeys: bool, passkey_rp_id: string\|null}`. `google` is true when `KEYALA_GOOGLE_CLIENT_IDS` is non-empty. `passkeys` is true only when `KEYALA_PUBLIC_URL` is `https://` and its host equals the configured passkey RP ID (a passkey cannot work otherwise); `passkey_rp_id` is that RP ID, else `null`. Clients use it to hide Google / passkey buttons the server cannot serve |
| POST | `/v1/auth/google` | `{id_token}` (Google ID token, `aud` one of `KEYALA_GOOGLE_CLIENT_IDS`, comma-separated iOS/Android/web client IDs; issuer `accounts.google.com`) → `{session_token, user}`. 404 when no client ID is configured, 401 `invalid_token`, 502 `google_unreachable`. Identity = Google `sub`: a first Google sign-in **always creates a new account**, never linked to an Apple/password/dev account even with the same email. `email` is Google's only when `email_verified` and not already used by another account, else `{sub}@google.invalid` |
| POST | `/v1/auth/passkey/options` | no body → `{challenge_id, challenge, rp_id, expires_at}` (5 min, single use; 30 per 10 min per address) |
| POST | `/v1/auth/passkey` | `{challenge_id, credential_id, authenticator_data, client_data_json, signature, user_handle?}` (all base64url) → `{session_token, user}` (app session; `user.auth_provider` is the account's own provider, e.g. `google`). 400 `challenge_expired`, 401 `passkey_failed` (unknown credential, wrong origin/rp, bad signature, counter went backwards), 429 (20 per 10 min per address). Discoverable credentials: send no allow-list |
| POST | `/v1/auth/password` | only if `KEYALA_PASSWORD_LOGIN` is set (one operator-provisioned account, a fallback while Apple sign-in is unavailable): `{username, password}` → `{session_token, user}`; 404 unconfigured, 401 `invalid_credentials`, 429 `rate_limited` (10 per 10 min per address). Identity `{username}@password.invalid`, never linked to Apple or email accounts |
| POST | `/v1/auth/dev` | only if `KEYALA_DEV_LOGIN=1`: `{email}` → `{session_token, user}` |
| POST | `/v1/me/passkeys/register/options` | session + fresh identity, body `{id_token? \| password? \| email? \| passkey?}` (same proof and checks as `/v1/me/delete`; a bad body, any other field or no proof → 401 `reauth_failed`; 429 `rate_limited`) → `{challenge_id, challenge, rp_id, user_id, user_name, exclude_credentials: [credential_id], expires_at}` (`user_id` base64url = WebAuthn `user.id`) |
| POST | `/v1/me/passkeys/register` | session, bound to the challenge from `register/options` (no second proof), `{challenge_id, credential_id, attestation_object, client_data_json, name?}` (base64url; `name` ≤ 60, default `Passkey`) → `Passkey {id, name, created_at, last_used_at}`. ES256 (P-256) only, else 400 `unsupported_passkey`; user verification required; `crossOrigin` must be absent or `false`; BS set without BE is invalid; attestation not checked. 400 `challenge_expired` / `passkey_rejected`, 409 `passkey_exists` / `too_many_passkeys` (10) |
| GET | `/v1/me/passkeys` | session → `[Passkey]` |
| DELETE | `/v1/me/passkeys/{id}` | session (owner only, else 404) → 204 |
| GET | `/v1/me/keys` | `[Key]` |
| PUT | `/v1/me/keys/{vendor}` | `{api_key}` → `Key` (key never returned). 400 `invalid_key` if the shape is wrong for the vendor (OpenAI `sk-…` not `sk-ant-`, Anthropic `sk-ant-…`; admin keys `sk-admin-`/`sk-ant-admin` refused) or the vendor's free `GET /v1/models` answers 401. A 403 (e.g. a restricted key without the models-list permission) or an unreachable vendor = saved with `checked: false`; a 200 = `checked: true` (field added to this response and to enrollment upload). Mock: a key containing `KEYALA_MOCK_INVALID` is rejected |
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
| POST | `/v1/me/oauth/openrouter` | session | `{limit_usd?}` → `{flow_id, auth_url, expires_at}` (10 min). `limit_usd` prefills OpenRouter's spending cap (the user can change it). Open `auth_url` in `ASWebAuthenticationSession` with callback scheme `keyala` |
| GET | `/v1/me/oauth/openrouter/flows/{flow_id}` | session (owner only, else 404) | `{flow_id, provider, status: pending\|exchanging\|saved\|failed\|cancelled\|expired, reason, last4, expires_at}`. This, not the callback URL, is the outcome |
| POST | `/v1/me/oauth/openrouter/flows/{flow_id}/cancel` | session (owner only) | pending → `cancelled` (later callbacks for it do nothing); other states unchanged; returns the flow |
| GET | `/oauth/openrouter/callback` | single-use `state` | OpenRouter redirects here; pending → exchanging exactly once, the server exchanges the code (verifier deleted first), stores the key as vendor `openrouter`, emits SSE `key.updated`, and answers a page that redirects to `keyala://oauth/openrouter?status=ok&flow={flow_id}` or `?status=error&reason=…[&flow=…]`. No key or last4 in the URL. Declined at OpenRouter = no state, so `reason=cancelled` without `flow`: the app cancels its own flow. A crash mid-exchange marks the flow `failed`/`interrupted` on restart (codes are single use; never retried) |
Mock mode: `auth_url` is the server's own `/oauth/openrouter/mock`, which approves at once (key `sk-or-v1-mock…`), so the
whole loop runs locally. OpenRouter keys can also be pasted (`sk-or-…`, checked with the free `GET /api/v1/key`).

### Add a key from a computer (enrollment)
The user is usually signed in to the provider's console on a computer, so the key is pasted there,
on the server's own page `{server}/add-key` (same origin as the API, no CORS; CSP, no-referrer, no-store).
| Method | Path | Auth | Notes |
|---|---|---|---|
| POST | `/v1/enroll` | none (10/10 min per IP) | `{vendor}` → `{enroll_id, enroll_url, qr_url, enroll_secret, expires_at}` (10 min). `enroll_id` = `en_` + 22 random chars (unguessable; QR only, no typed code). `enroll_secret` (`keyala_es_…`) stays in the page's memory |
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
`/.well-known/apple-app-site-association` covers `/connect/*` and `/enroll/*` for `{KEYALA_APPLE_TEAM_ID}.{KEYALA_APPLE_BUNDLE_ID}`
(default `83APHP8F76.app.sorevan.masterkey`), plus `webcredentials` for passkeys (app entitlement `webcredentials:{server host}`).
**Passkeys** are bound to `KEYALA_PASSKEY_RP_ID` (default: host of `KEYALA_PUBLIC_URL`, e.g. `app.keyala.io`; changing it
orphans stored passkeys). Accepted origins: `https://{rp_id}`, `KEYALA_PASSKEY_ORIGINS`, and `android:apk-key-hash:…` derived
from each `KEYALA_ANDROID_CERT_SHA256`. **Android:** with `KEYALA_ANDROID_PACKAGE` + `KEYALA_ANDROID_CERT_SHA256` (`AB:CD:…`, comma
separated) the server answers `/.well-known/assetlinks.json` (`handle_all_urls` + `get_login_creds`); otherwise 404. An Android
client uses the same `/v1/auth/google` (its own client ID in `KEYALA_GOOGLE_CLIENT_IDS`) and passkey endpoints. Custom scheme `keyala://connect/{code}` and `keyala://enroll/{id}` remain.

### Web console (`/console`)

A page served by the server itself (same origin as the API, so no CORS; CSP `default-src 'self'`). Two ways in, both end in a
**web-scoped session**: `scope: "web"`, expires 12 h after creation (`expires_at`), never refreshed. App sessions
(`scope` app, no expiry) are unchanged.

**Web-session allowlist (fail closed).** Handlers opt in with `auth_level(READ)`; every other session route answers
`403 web_session_not_allowed`. Allowed: `GET /v1/me`, `GET /v1/me/keys`, `GET /v1/me/grants`, `DELETE /v1/me/grants/{id}`
(revoke), `GET /v1/me/requests` (+ `/{id}`), `GET /v1/me/preferences`, `GET /v1/me/events`, `POST /v1/me/logout`, and
`/v1/me/apps*` below. Everything else is refused: approve/deny requests and connects, PUT/DELETE keys, PATCH grants,
enroll, OAuth, devices, password set/delete, web-login preview/approve/deny, stop-all, revoke-all, delete account,
preferences PATCH. An expired web token is `401 unauthorized`.

**`GET /v1/me`** (any session) -> `{id, email, auth_provider: apple|google|password|dev, has_password, has_passkey}`.

**Phone sign-in for the web**
- `POST /v1/web/login` (no auth; 20 per 10 min per address, else 429) -> `{login_id, login_secret, qr_url, url, expires_at}`.
  `login_id` = `wl_` + 22 base64url chars (25 total); `login_secret` = `keyala_wl_...`; `url` = `{public_url}/login/{login_id}`
  (universal link, AASA `/login/*`; custom scheme `keyala://login/{login_id}`); `qr_url` = `{url}/qr.svg`; valid 5 minutes.
  The server records the browser's `User-Agent` (first 200 chars) and country (`KEYALA_COUNTRY_HEADER`, like grants).
- `GET /v1/web/login/{login_id}` with `Authorization: Bearer {login_secret}` (wrong/missing secret or unknown id: 401) ->
  `{status: pending|approved|denied|expired|used}`. Once approved, the FIRST poll returns
  `{status:"approved", session_token, expires_at, scope:"web", user:{id,email,auth_provider}}` and the row becomes `used`
  (later polls: `{status:"used"}`, no token). The session belongs to the user who approved.
- Phone (app session only; web sessions get 403): `GET /v1/me/web-login/{id}` -> `{id, status, created_at, expires_at,
  user_agent (nullable, max 200 chars), country (nullable, 2 letters)}`; unknown/malformed id: 404.
  `POST /v1/me/web-login/{id}/approve` and `/deny` (empty body) -> `{id, status: "approved"|"denied"}`; only while `pending`
  and unexpired, first decision wins, otherwise `409 not_pending`.
- `GET /login/{id}` (HTML, "Open in the Keyala app" -> `keyala://login/{id}`), `GET /login/{id}/qr.svg`.

**Web password.** `PUT /v1/me/password {password}` (app session; 12..=200 chars else 400; stored as an argon2id hash on the
user; -> `{has_password:true}`), `DELETE /v1/me/password` (-> `{has_password:false}`). `POST /v1/auth/password
{username, password}` now has two meanings: the operator account (`KEYALA_PASSWORD_LOGIN`, unchanged: app-scoped session) or
`username` = an account's email (case-insensitive) with a password set -> web-scoped session (response adds `expires_at`,
`scope:"web"`). Unknown user and wrong password are the same `401 invalid_credentials` (an argon2 verify always runs); 10 per
10 min per address (429). **Behavior change:** the endpoint no longer answers 404 when `KEYALA_PASSWORD_LOGIN` is unset.

**Developer apps owned by accounts** (`apps.owner_user_id`, nullable; web or app session; non-owner = 404)
- `GET /v1/me/apps` -> `[{id, name, homepage, allowed_origins, created_at, requests_7d, cost_usd_7d}]` (7 UTC days, today included).
- `POST /v1/me/apps {name, homepage?, icon_url?, allowed_origins?}` -> `{app_id, app_secret}` (secret shown once; same validation as
  `POST /v1/apps`; 20 per hour per address).
- `POST /v1/me/apps/claim {app_secret}` -> the app summary above. Attaches an unowned app whose secret verifies (`401
  invalid_app_secret`; `409 already_owned` if another account owns it; idempotent for the owner). 10 per 10 min per address.
- `GET /v1/me/apps/{id}/report?days=` -> exactly the `/v1/app/report` shape. `POST /v1/me/apps/{id}/secret` -> `{app_secret}`
  (rotation; the old secret dies at once).
- `PUT /v1/me/apps/{id} {name, homepage?, allowed_origins?}` -> the app summary. Replaces name, homepage and allowed origins (null or
  empty = server app only; same validation as create). New connections use the new values; existing grants keep working; the
  secret is unchanged.
- `DELETE /v1/me/apps/{id}` -> `{deleted: true, grants_revoked, requests_denied}`. Soft delete (`apps.deleted_at`, migration 0015)
  in one transaction: every grant for the app is revoked, pending and approved-but-unsent calls are denied and refunded, pending
  connect codes become `denied`. Afterwards the secret is `401` everywhere (also for claim and sealed re-creation), browser connects
  are `403 origin_not_allowed`, and `/v1/me/apps*` answers `404`. Users keep their history. Calls already `sending` finish.
- Deleting the account detaches its apps (owner set to null); they keep working with their secret.

**Plaza** (KEYALA-3; opt-in, hand-reviewed public list of community apps; migration 0017; staging first, never auto-published)
- Off unless `KEYALA_PLAZA_ENABLED=1`: every route in this section, `/examples` and `/examples/plaza.js` answer `404 not_found`.
  Reviewers are stable verified identities in `KEYALA_PLAZA_REVIEWERS` (comma separated): `google:<google_sub>`,
  `apple:<apple_sub>`, or `password:<username>` for the operator account of `KEYALA_PASSWORD_LOGIN` only. Never emails or user
  ids (those change on an ephemeral deploy); dev-login accounts are never reviewers. Matched against the `google_sub`/`apple_sub`
  columns (Apple first, as `auth_provider`), so same email with another provider or subject never matches. Empty (default) =
  nobody can review, so nothing ever becomes public.
- Status per app, `apps.plaza_status`: `null` (not submitted) | `pending` | `approved` | `rejected`. Only `approved` apps that are not
  deleted and still have an owner are public, and only their `name` and `homepage`.
- `GET /v1/plaza` (public, `Access-Control-Allow-Origin: *`, `no-store`) -> `[{name, homepage}]`, newest approval first, max 200.
- `GET /v1/me/apps` and `PUT` responses gain `plaza` (`null|"pending"|"approved"|"rejected"`); `GET /v1/me` gains `plaza_enabled`
  and `plaza_reviewer` (booleans), and `plaza_review_id` (the caller's own identity string above, or `null` when Plaza is off or the
  account has none, e.g. dev login). The console shows it with a copy button while Plaza is on. No other field changes.
- `POST /v1/me/apps/{id}/plaza` -> `{status:"pending"}`. Owner only (else 404). The app needs an https homepage naming a public DNS
  host, as the WHATWG URL parser (`reqwest::Url`) reads it: valid port, no credentials, no IP in any spelling,
  no `localhost`/`.local`-style suffixes, no whitespace, control, backslash or `<>"'` characters, max 300 chars; Unicode paths and
  IDN hosts are allowed, else `400`. The raw string is stored and compared as typed; nothing is fetched. `409 already_listed`
  if pending or approved; a rejected app may resubmit. 20 per hour per address.
- `DELETE /v1/me/apps/{id}/plaza` -> `{status:null}` (owner withdraw; idempotent).
- A `PUT /v1/me/apps/{id}` that changes `name` or `homepage` sets the status to `null` in the same SQL statement; changing only
  origins keeps it. Deleting the app or the account hides it.
- Reviewer routes (web or app session; everyone else `404`): `GET /v1/plaza/review?status=pending|approved|rejected` ->
  `[{app_id, name, homepage, status, submitted_at}]` (no owner, keys or request content). `POST /v1/plaza/review/{app_id}
  {decision: approve|reject|withdraw, name, homepage}`: `name` and `homepage` are what the reviewer saw and must equal the current
  values, otherwise `409 stale_submission` and nothing changes. `approve`/`reject` need `pending`, `withdraw` needs `approved`
  (-> `rejected`); a wrong state is `409 not_pending`. The check and the change are one conditional UPDATE, so an owner edit and
  a decision cannot both win.
- The console shows "List in Plaza" / "Withdraw" per app and, for reviewers, a review panel.
- `GET /examples`, `/examples/` and `/examples/plaza.js` serve `backend/src/plaza.html` and `plaza.js` (copies of `site/examples/`,
  synchronised by the site owner) with a strict CSP, so staging can show the page before production has it.

### Account controls and operations (launch readiness)
| Method | Path | Auth | Notes |
|---|---|---|---|
| POST | `/v1/me/sessions/revoke-all` | session | `{keep_current?: true}` → `{revoked}`. Signs out every other device (and this one when `keep_current` is false) |
| POST | `/v1/me/logout` | session | no body → `{signed_out: true}`. Deletes only the session whose token is sent (other sessions of the same or any other user are untouched). Calling it again, or with an unknown token, answers 401 `unauthorized` like any dead session, so a client treats 401 on logout as "already signed out" and clears local state either way. Grant tokens and keys are not affected (use stop-all for that) |
| POST | `/v1/me/stop-all` | session | no body → `{grants_revoked, requests_denied, requests_in_flight, enrollments_cancelled, flows_cancelled}`. Lost-device switch: every grant token dies at once; pending and approved-but-unsent requests become `denied` (reservations refunded; an unsent approved request is never sent afterwards); requests already `sending` cannot be recalled and are counted in `requests_in_flight`. Keys stay stored; sessions stay valid. New connects work normally afterwards |
| POST | `/v1/me/delete` | session + fresh identity | `{confirm: "DELETE", id_token?, authorization_code?, password?, email?, passkey?}` → `{deleted: true, apple_revoked: true\|false\|null}`. 400 `confirmation_required` (confirm is not exactly `DELETE`), 401 `reauth_failed`, 409 `request_in_flight` (a call is being sent to a provider; retry in about a minute), 429 `rate_limited`. Deletes in one transaction: sessions, devices, passkeys, stored provider keys, grants (tokens die), every request with its payload and response, enrollments (incl. uploaded candidates), OAuth flows, the user row. Developer reports are computed from stored requests, so a deleted account's calls also leave the app's report (nothing is retained). Irreversible |
**`user.auth_provider`:** every login response (`/v1/auth/apple|google|passkey|password|dev`) returns `user: {id, email, auth_provider}` with `auth_provider` one of `"apple"`, `"google"`, `"password"`, `"dev"` (passkey sign-in returns the account's own), set by the login route that verified the identity. Native should choose the fresh-reauth UI from it (Apple: new Apple sign-in sheet and send `id_token` + `authorization_code`; password: ask for the password; dev: email). It is display/UX only; authorization uses the stored identity (Apple `sub`, operator password account, dev email), never the email text and never links accounts by email. `email` is whatever Apple supplied at first sign-in (possibly a private relay, or the placeholder `{sub}@apple.invalid`); it changes only by changing it with Apple, Keyala has no editable profile. `/v1/auth/dev` refuses any email ending `.invalid`.
**Deletion durability:** the deletion is recorded as `deleted <user_id> <time>` in `deleted-accounts.log` and fsynced BEFORE the database transaction commits. If that file cannot be written the request fails with `503 deletion_unavailable`, nothing is deleted and the session stays valid (retry later). If the commit itself fails after the record was written, an `undeleted <user_id> <time>` line cancels it. `docs/ops/replay-deletions.sh` uses the last line per id after a restore. Apple revocation runs after the commit, so a refused deletion never revokes Apple access. Backups are NOT erased on deletion; they roll off by retention, and a restore re-applies the recorded deletions.
Fresh identity, by how the account signs in: any account with a passkey may instead send `passkey` = a `/v1/auth/passkey` body for a fresh `/v1/auth/passkey/options` challenge (must be this account's passkey); Apple accounts send a **fresh Apple `id_token`** (its `sub` must equal the account's, `iat` within 5 minutes, same audience as sign-in); Google accounts the same with a fresh Google `id_token`; password accounts send `password`; dev-login accounts send their `email` (only exists when `KEYALA_DEV_LOGIN=1`). A session token alone never deletes an account.
Apple revocation: if the app also sends the `authorization_code` from that same Apple sign-in and the operator configured `KEYALA_APPLE_SIGNIN_KEY_P8` (+ `KEYALA_APPLE_SIGNIN_KEY_ID`, team `KEYALA_APPLE_TEAM_ID`), the server exchanges the code at `appleid.apple.com/auth/token` and revokes the refresh token at `/auth/revoke` before deleting; `apple_revoked` is `true`. If not configured or Apple refuses, deletion still completes with `apple_revoked: false` (non-Apple accounts: `null`) and the app should tell the user they can also remove Keyala under Settings > Apple Account > Sign in with Apple. The `.p8` is never in git or logs.
**Operator switches (not user API):** `KEYALA_PAUSE_UPSTREAM=1`, or the file `PAUSE_UPSTREAM` created next to the database, makes new proxy calls answer `503 upstream_paused` with no reservation, and holds approved-but-unsent requests (still `approved`, sent after unpausing). `GET /v1/status` (public, no secrets) → `{ok, paused, db, version}`; `/healthz` stays `ok`. Consistent backup, restore drill and runbook: `docs/ops/`.
`GET /v1/status` (public) -> `{ok, paused, db, version}`, HTTP 503 when the database does not answer. `KEYALA_BACKUP_DIR` (+ `KEYALA_BACKUP_KEEP`, default 7; `KEYALA_BACKUP_INTERVAL_SECS`, default 86400) writes `VACUUM INTO` backups in-process, counted from the newest file. Each deletion appends `deleted <user_id> <time>` (no email) to `deleted-accounts.log` beside the database for restore replay. See docs/ops/.

### Route OpenAI / Anthropic apps through OpenRouter (optional, per account)
Lets an app written for the OpenAI or Anthropic API work for a user who only has an OpenRouter key.
- `GET /v1/me/preferences` -> `{openrouter_for_direct_apis: false}` (default OFF); `PATCH /v1/me/preferences` `{openrouter_for_direct_apis: bool}` -> same shape. Per user, stored on the server that answers (not synced across servers). Session auth, owner only.
- The setting only affects NEW connects. At approval, the grant freezes `billing_vendor`: `openrouter` when the setting is ON and the app's `vendor` is `openai` or `anthropic`, otherwise the vendor itself. Turning the setting on or off later never changes an existing grant, and the user must reconnect an app to change its route. Existing grants (pre-feature) are direct.
- `GET /v1/me/connect/{code}` adds `billing_vendor` (what approval would use). `POST /v1/me/connect/{code}/approve` accepts optional `billing_vendor` (the value the phone showed); if it no longer matches, `409 route_changed` and nothing is consumed. A routed approval requires an OpenRouter key (`409 no_key` naming `openrouter`); the user's direct key is never a fallback.
- `Grant` gains `billing_vendor`. `Request` gains `billing_vendor` and `upstream_model` (the exact OpenRouter slug, null when direct); `vendor`/`model`/`payload` keep the ORIGINAL protocol, model and body, so the review shows both. Native must show original model, `upstream_model` and `billing_vendor` before Allow.
- Wire behaviour: the app keeps its protocol, auth and response shape. OpenAI protocol -> `POST https://openrouter.ai/api/v1/chat/completions`; Anthropic protocol -> OpenRouter's native Anthropic Messages `POST https://openrouter.ai/api/v1/messages` (no conversion; source: OpenRouter's official `openapi.json`, `POST /messages` "Create a message using Anthropic Messages API format", checked 2026-11-20), `Authorization: Bearer <OpenRouter key>` plus `anthropic-version: 2023-06-01`. Only the `model` field is rewritten, using a static verified table (`pricing::or_slug`): gpt-4o, gpt-4o-mini, gpt-4.1, gpt-4.1-mini -> `openai/<same>`; claude-opus-5-5 / sonnet-5-5 / haiku-5-5 -> `anthropic/claude-<tier>-5.5`. Any model without a mapping: `400 model_not_routable`, nothing stored or sent, no fallback. OpenRouter's `models`/`provider`/`plugins` are not reachable.
- Money: the reservation and the settlement use the OpenRouter table's prices for the mapped slug, from the response `usage` (OpenAI `prompt/completion_tokens`, Anthropic `input/output_tokens`). Missing usage keeps `actual_usd: null` with the reservation retained, never zero or the estimate presented as actual. It is still an estimate of OpenRouter's invoice, not the invoice.
- Limits (not all API features are claimed to work): same text-only, no streaming, required output cap and at-most-once/`Idempotency-Key` rules as direct. Additionally rejected when routed (`400 unsupported_option`) because they were not verified on OpenRouter: OpenAI `max_completion_tokens`, `n`, role `developer`; Anthropic `metadata`, `top_k`. Key replacement/deletion, revocation, stop-all, deletion and session ownership use the existing paths (a deleted OpenRouter key makes routed calls fail `no_key`).
- Migration `0012_openrouter_routing.sql`: `users.openrouter_for_direct`, `grants.billing_vendor`, `requests.billing_vendor`, `requests.upstream_model`. Mock mode adds `mock_url` to responses so tests can see which fixed upstream URL was chosen.

### Keyala Coupon (sponsored calls, migration `0018_coupons.sql`)
An app owner funds calls for their users with a **dedicated batch key**; each recipient claims a fixed slice and still approves every call on their phone. Money is integer micro-USD in the database and USD floats in JSON.
- **Objects.** `Coupon` `{id, batch_id, app:{id,name,homepage,icon_url}, vendor, models:[string], remaining_usd, expires_at, status}` (`id` = the claimed coupon; `status` is derived: `revoked` | `expired` | `exhausted` | `active`). `CouponPreview` `{batch_id, app, vendor, models, per_coupon_usd, validity_days, claim_deadline, claimable, coupon: Coupon|null}`. `Grant` and `Request` gain `coupon: Coupon|null`. `GET /v1/me/connect/{code}` gains `coupon: CouponPreview|null`. The batch code is `KC-` followed by 22 base64url characters (16 random bytes) and is **case-sensitive**: it is never trimmed or case-folded anywhere (API, landing page, QR). It is a shareable identifier, not a credential. Error `type`s (the native app switches on them): `coupon_not_found` (404; also for a code of another app or vendor, or someone else's coupon: nothing is revealed), `coupon_revoked` (410), `coupon_expired` (410), `coupon_exhausted` (402; used up, too little for this call, or the batch was stopped), `coupon_claim_limit` (409, no coupon left in the batch), `coupon_model_not_allowed` (403), `coupon_policy_required` (403). Terminal coupon errors are never 401.
- **Recipient API (session).** `GET /v1/me/coupons` -> `[Coupon]`; `GET /v1/me/coupons/{code}` -> `CouponPreview` (web sessions allowed); `POST /v1/me/coupons/{code}/claim` -> `Coupon` (phone session only; idempotent for the same account; one claim per account and batch, forever: an expired coupon is returned as is, never reissued). Claim reserves the full per-coupon amount from the batch's free budget in one guarded UPDATE, so racing claims cannot exceed the budget. Errors: `409 coupon_claim_limit`, `410 coupon_expired` (claim deadline), `410 coupon_revoked`, `402 coupon_exhausted` (batch stopped), `404 coupon_not_found` (also when the app was deleted).
- **Links.** `keyala://coupon/{code}` and `https://{server}/coupon/{code}` (landing page, `/coupon/{code}/qr.svg`; AASA lists `/coupon/*`). A bare `KC-` code is accepted by scanners. The landing page shows nothing about the batch. Claiming does not grant developer authority: connecting still goes through the normal connect approval.
- **Sponsored connect.** `POST /v1/connect` accepts `coupon_code` (must belong to the same app and vendor, otherwise `404 coupon_not_found`). Approving that connect claims (or finds) the account's coupon in the same transaction and creates a grant with `coupon_id`; **no personal key is needed or used**, and OpenRouter routing never applies. The policy of a sponsored grant is `ask` forever: the connect approval, `PATCH /v1/me/grants/{id}` and `POST /v1/me/requests/{id}/approve` all refuse any other policy with `403 coupon_policy_required`, and the proxy treats sponsored grants as `ask` whatever is stored.
- **Spending.** At request creation the coupon must be this user's, bound to this app and vendor, unexpired, unrevoked, cover the model (`403 coupon_model_not_allowed`) and the estimate (`402 coupon_exhausted`). Nothing is reserved while the call is pending (a pending call keeps its full prompt visible for review). Approving it moves the estimate from the coupon's `remaining` to `held` once (ledger key `hold:{request}`; expiry, revoke, binding and balance are re-checked in that UPDATE, failure leaves the call pending). The send claim re-checks the same conditions, and the call is sent only with the batch key (AAD `batch|{id}|{vendor}`); there is no fallback to the user's keys. Completion settles to the provider-reported cost; with no usage block the estimate is charged; failure/denial/revoke before sending releases the hold; `uncertain` keeps it held and is never retried. If its outcome later becomes known, an internal reconciliation (by request id, no HTTP surface) records the money movement once in the ledger. It never rewrites the call: its status and 504 body still say the delivery was uncertain, while reports show the settled cost separately (delivery unknown vs accounting known). If an actual cost ever exceeds its held estimate, the overshoot is recorded truthfully (taken from the coupon, then the batch's free budget), `overshoot_usd` rises and the batch **stops** (no claims or calls). Keyala does not claim a provider-side hard limit on the batch key.
- **Expiry and revoke.** A coupon expires `validity_days` after its claim. A restart-safe sweep (every 30 s and at start) cancels waiting calls of expired/revoked/stopped coupons, releases holds, and returns each coupon's unused `remaining` to the batch exactly once (`closed_at` + ledger key `return:{coupon}`). Holds of `sending`/`uncertain` calls stay held; their late settlement returns the leftover straight to the batch. Revoking a batch wipes its key and does the same immediately. Deleting an app revokes its batches. Deleting a recipient account denies its waiting calls, returns the unused balance and deletes its coupons, requests and content. A call whose outcome is **unknown stays held, never classified as spent**: its coupon loses its user (`user_id` NULL) and keeps only money (reconciliation by request id still works, the row disappears once it holds nothing). Deleting an issuer revokes its batches (key wiped, other users' waiting calls denied), detaches them from the account and erases every stored mail address. The batch, coupon and ledger tables hold no user id or address. `docs/ops/replay-deletions.sh` repeats exactly this after a restore (tested against the real script, with backups from before and after migration 0018).
- **Issuer API (owner of the app; web console sessions allowed).** `GET|POST /v1/me/apps/{app_id}/coupons`; `POST` body `{vendor, models:[string], api_key, per_coupon_usd, total_usd, validity_days (default 7), claim_deadline}`; models must be priced allowlist entries for the vendor; the key goes through the same shape and liveness check as personal keys and is stored encrypted. Batch JSON `{id, coupon_code, claim_url, app, vendor, models, per_coupon_usd, total_usd, available_usd, spent_usd, claimed, validity_days, claim_deadline, revoked_at, stopped_at, key_last4, email_enabled}` (`claimed` counts claims from the ledger, so it survives recipient deletion; `spent_usd` is the batch's settled spend); the key is never returned again. `DELETE /v1/me/apps/{app_id}/coupons/{batch_id}` revokes (returns the batch JSON). `GET .../{batch_id}/report` returns aggregates only, read in one transaction snapshot: `coupons` (claimed from the ledger, active/expired/revoked/exhausted), `first_successful_users`, `money` (`total_usd`, `available_usd`, `unspent_in_coupons_usd`, `held_usd` = `uncertain_held_usd` + `in_flight_held_usd`, `spent_usd` = `actual_cost_usd` (provider usage) + `completed_estimated_cost_usd` (completed without usage, estimate charged), `returned_usd` (incl. late leftovers), `overshoot_usd`), `requests` (zero-filled counts for pending/approved/sending/completed/failed/denied/expired/uncertain of requests still stored, `uncertain_requests_accounted`), tokens, and `models[]` (`requests`, `actual_cost_usd`, `completed_estimated_cost_usd`, `cost_usd` from the ledger). Uncertain money is held with unknown amount, never counted as spent. No prompts, responses, user ids or emails.
- **Ledger.** `coupon_ledger` is append-only with a UNIQUE key per movement (`claim:`, `hold:`, `close:` (kind `settle` = provider usage, `estimate` = no usage, `release`), `return:`/`refund:`, `overshoot:`, `first:`); hold and close rows carry the model name so reports survive the deletion of request rows, so a retry cannot apply twice. Invariant per batch: `total = available + sum(remaining + held) + spent`.
- **Email (optional).** Three events to the issuer's verified address: `capacity` (free budget below one full coupon: no claim slot left; claimed coupons stay valid), `exhausted` (spent, i.e. settled or charged at estimate, reached the total: held or merely unallocated money never counts) and `stopped` (a call cost more than its held estimate and stopped the batch, below or beyond the total: the text says it stopped unexpectedly and never claims the budget is unspent; a stopped batch never also raises `exhausted`). The address is the one of the last **verified** Apple/Google identity token (`users.notify_email`, written only by sign-in; accounts that have not signed in since, and Apple tokens without `email_verified`, have none and fail closed). Dev-login accounts count only under the mock transport; `.invalid` placeholders and anything but one plain address never do; clients cannot choose a recipient. One outbox row per episode (`{kind}:{batch}:{episode}`). The message (from, to, subject, text) is bound once before the first attempt and then immutable (trigger); the outbox id is the provider `Idempotency-Key`, so every retry is the same message. The transport (`mock`/`resend`) is pinned in the row at that bind; a retry under another transport is skipped and erased (mock-bound mail never reaches a real provider), and before every attempt the issuer's current verified address must equal the stored recipient (marker removed, address changed or issuer deleted: skipped and erased). Attempts are claimed atomically only when due, below 5 attempts and within 23 h of the first attempt (Resend de-duplicates keys for 24 h), results apply only to the attempt that owns the row (never resurrecting a skipped one); 429/409/5xx/network retry with backoff 1 min, 5 min, 30 min, 2 h, other 4xx fail at once; a restart resumes only inside those limits. Finished rows keep no address or text; deleting the issuer erases them too. Transport: `POST https://api.resend.com/emails` (fixed in code) with bearer key and JSON `{from,to,subject,text}`, or `KEYALA_EMAIL_MOCK=1` (records the would-be requests in memory, no network). Without either, `email_enabled` is false and `GET /v1/status` shows `coupon_email: off|mock|resend`. No address, text or token is logged.
- **Web SDK.** `Keyala.connect({app, vendor, coupon})` / `<keyala-connect coupon="KC-...">` sends `coupon_code`.

**Server pages:** `/about` and `/privacy` (same page, English / 繁體中文 / 日本語, picked by browser language) state who runs the server (a personal experimental project, no SLA), what is stored and sent where, retention, deletion and backups, and support contact. `/add-key`, `/connect/{code}`, `/enroll/{id}` and the OpenRouter result page link to it. They make no claim about production disk or backups; update the text when production persistence is actually applied.

Auth header everywhere: `Authorization: Bearer <token>`. Token prefixes: `keyala_app_`, `keyala_grant_`,
`keyala_sess_` (so a leaked token is identifiable).

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
login (Google later; 2026-11-24, KEYALA-9: Google = its own new account, no binding/merging with Apple; passkeys for fast sign-in and Android readiness); hosting = local + Docker, owner deploys to Render; ownership split approved.
