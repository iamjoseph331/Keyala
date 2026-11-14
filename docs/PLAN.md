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
   QR). User opens it in the iOS app, sees the app name + requested vendor, picks an initial
   policy, approves. Developer polls `GET /v1/connect/{code}` → receives a `grant_token`.
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
- Estimate is a **conservative reservation, not a guarantee**: input tokens ≈ `utf8_bytes / 2`
  (overestimates vs chars/4 for Unicode) + fixed per-message overhead; output = `max_tokens`,
  **required and capped** per model (server injects a cap if missing). Text-only in v1:
  requests with images/audio/tools/files are rejected (`400 unsupported_content`).
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
Grant    { "id", "app": App, "vendor": "openai"|"anthropic", "policy": Policy,
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
| POST | `/v1/apps` | none (rate-limited) | `{name, homepage?, icon_url?}` → `{app_id, app_secret}` |
| POST | `/v1/connect` | app secret | `{vendor, suggested_policy?}` → `{connect_code, connect_url, expires_at}` (10 min) |
| GET | `/v1/connect/{code}` | app secret | `{status: pending\|approved\|denied\|expired, grant_token?}` |
| POST | `/v1/openai/chat/completions` | grant | OpenAI body passthrough; vendor response verbatim |
| POST | `/v1/anthropic/messages` | grant | Anthropic body passthrough; vendor response verbatim |
| GET | `/v1/requests/{id}` | grant | `{status, response?}` |

Proxy responses carry `X-MasterKey-Request-Id`. Streaming (`stream: true`) → `400` in v1.

### User-facing (iOS app)
| Method | Path | Notes |
|---|---|---|
| POST | `/v1/auth/apple` / `/v1/auth/google` | `{id_token}` → `{session_token, user}` |
| POST | `/v1/auth/dev` | only if `MK_DEV_LOGIN=1`: `{email}` → `{session_token, user}` |
| GET | `/v1/me/keys` | `[Key]` |
| PUT | `/v1/me/keys/{vendor}` | `{api_key}` → `Key` (key never returned) |
| DELETE | `/v1/me/keys/{vendor}` | |
| POST | `/v1/me/devices` | `{apns_token}` |
| GET | `/v1/me/connect/{code}` | preview `{app, vendor, suggested_policy}` |
| POST | `/v1/me/connect/{code}/approve` | `{policy}` → `Grant` |
| POST | `/v1/me/connect/{code}/deny` | |
| GET | `/v1/me/grants` | `[Grant]` |
| PATCH | `/v1/me/grants/{id}` | `{policy}` → `Grant` |
| DELETE | `/v1/me/grants/{id}` | revoke |
| GET | `/v1/me/requests?status=&limit=` | `[Request]` without payload/response |
| GET | `/v1/me/requests/{id}` | full `Request` |
| POST | `/v1/me/requests/{id}/approve` | `{policy?}` (omit = once) → `Request` |
| POST | `/v1/me/requests/{id}/deny` | → `Request` |
| GET | `/v1/me/events` | SSE: `request.created`, `request.updated`, `grant.updated` (data = object JSON) |

Auth header everywhere: `Authorization: Bearer <token>`. Token prefixes: `mk_app_`, `mk_grant_`,
`mk_sess_` (so a leaked token is identifiable).

## Milestones
- **M0** plan + contract (this file) — both agents ACK.
- **M1 backend core** — dev login, keys, grants, requests, proxy with hold/poll, mock vendor,
  SSE. `cargo test` covers the policy + approval path end to end.
- **M2 iOS core** — dev login, add key, live pending list, approve sheet (payload + est. cost +
  4 scopes), history + detail (payload/response), grants list with revoke.
- **M3 connect pairing** — QR / universal link + code entry; `sdk/` example app ("Chat with your
  own key") demonstrating the full loop.
- **M4 real-world** — Sign in with Apple, APNs (owner has a paid Apple Developer account),
  Dockerfile + `render.yaml` (owner deploys to Render for live testing; SQLite needs a Render
  persistent disk or data resets on redeploy).
- **M5 launch page** — `site/` with hero, 30-s demo video (simulator recording of the example
  app + phone approval side by side), quickstart, trust model. README mirrors it.

## Open questions (owner)
Answered 2026-11-14: paid Apple Developer account = yes; login = Sign in with Apple + dev
login (Google later); hosting = local + Docker, owner deploys to Render; ownership split approved.
