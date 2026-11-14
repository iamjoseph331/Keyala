# Proposed MVP contract (awaiting Claude agreement)

Use JSON with snake_case keys, RFC3339 UTC timestamps, opaque string IDs, and integer
USD microdollars (1 USD = 1,000,000 units). Errors: {"error":{"code":"...","message":"..."}}.
All routes except health and explicit demo bootstrap require Bearer authentication.

## User app

- `GET /health` -> {"status":"ok","mode":"demo"|"live"}
- `POST /v1/auth/demo` -> {"access_token":"...","user":{"id":"...","name":"Joseph"}}
  Only available with explicit local demo mode. No embedded production secret.
- `POST /v1/auth/google` {"id_token":"..."} -> same session shape.
- `GET /v1/requests` -> {"requests":[Request]}. Authenticated user's records only.
- `GET /v1/requests/{id}` -> Request. Includes exact payload and completed response.
- `POST /v1/requests/{id}/decision` {"decision":"approve"|"deny"} -> Request.
- `GET /v1/keys` -> {"keys":[{"id":"...","provider":"openai","label":"Personal","last_four":"..."}]}.
- `POST /v1/keys` {"provider":"openai"|"anthropic","label":"...","api_key":"..."} -> key metadata only.
- `DELETE /v1/keys/{id}` -> 204.
- `GET /v1/grants` -> {"grants":[Grant]}.
- `POST /v1/grants` {"app_id":"...","provider":"openai","model":"...",
  "max_requests":5,"budget_microusd":100000,"expires_at":"...","trust":false,
  "alert_threshold_microusd":50000} -> Grant.
  Limits are optional for trust mode, expiry mandatory, alerts still apply.
- `DELETE /v1/grants/{id}` -> 204.
- `POST /v1/devices` {"token":"...","environment":"sandbox"|"production"} -> 204.

Request fields: `id`, `app_id`, `app_name`, `provider`, `model`, `status`,
`created_at`, `expires_at`, `payload` (provider JSON), `payload_sha256`,
`estimated_cost_microusd`, `reserved_cost_microusd`, `actual_cost_microusd` (nullable),
`response` (nullable JSON), `error` (nullable string).
States: pending, approved, denied, executing, completed, failed, expired, uncertain.

Grant fields: `id`, `app_id`, `app_name`, `provider`, `model`, `max_requests`
(nullable), `used_requests`, `budget_microusd` (nullable), `spent_microusd`,
`reserved_microusd`, `expires_at`, `trust`, `alert_threshold_microusd` (nullable).

## Developer surface

Developer service authenticates as its app, using a user-authorized connection token
scoped to that app. Never accept a caller-supplied arbitrary user ID as authorization.
Submit an immutable request, return 202/request ID, poll status, then fetch response.
The wrapper handles polling with a finite timeout; it never sees a vendor key.

## Native app behavior

SwiftUI: approval inbox, request inspector, history, key vault metadata, revocable
grants, settings for backend URL/session. Full exact JSON visible before approval.
Do not approve from notification actions. Keychain for session tokens. Clear visual
demo mode. Foreground refresh works without APNs credentials.

## External references checked

- https://developers.openai.com/api/reference/resources/responses/methods/create
- https://platform.claude.com/docs/en/api/messages/create
- https://developer.apple.com/documentation/usernotifications/sending-notification-requests-to-apns

This is a proposal. Coordinate changes in AGENT_HANDOFF.md before treating it as stable.
