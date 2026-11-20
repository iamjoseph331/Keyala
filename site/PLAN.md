# keyala.io website plan (2026-11-20, Joseph's brief)

Owner decisions: site matches the app (paper #FAF7F2, ink #1F1B16, green #3D7A57, koala mascot); home + user guide in
English, 繁體中文, 日本語 (browser language by default, picker, remembered); developer docs in English; developer portal and
user web view are ONE console; web sign-in = scan with the phone OR account + password.

## Map

| URL | What | Built from |
|---|---|---|
| `keyala.io/` | #1 Home: the idea in one screen, then two lanes (developers / users) | `site/index.html` |
| `keyala.io/docs/` | #2 Developer docs, quickstart to dashboard, API reference | `site/docs/keyala.md` (single source) rendered in the page |
| `keyala.io/docs/keyala.md` | the same docs as one Markdown file (download / paste into an AI assistant) | same file |
| `keyala.io/llms.txt` | pointer for AI tools to the Markdown docs | static |
| `keyala.io/guide/` | #3 User guide: install, sign in, add a key, pair an app, approve, OpenRouter for OpenAI/Anthropic apps, limits, revoke; video slots | `site/guide/index.html` |
| `app.keyala.io/console` | #4 + #5 Console: My apps (developer) and My account (keys, connected apps, history) | served by the backend (same origin as `/v1/me`, which has no CORS on purpose) |
| `keyala.io/privacy/`, `/support/` | kept, restyled to the shared base | |
| `keyala.io/developers/`, `/tutorials/` | old pages; redirect to `/docs/` and the console once the console ships | |

Shared: `site/base.css` (tokens, light + dark, nav, footer, type), `site/i18n.js` (data-i18n strings, picker),
`site/brand/koala.svg` (placeholder until Jolene's mascot). Plain HTML/CSS/JS, no build step, relative paths.

## Messages

Problem: an AI app has to decide who pays for the model. Keyala lets the user bring their own key without handing it over.
- Developers: change one line (base URL), official OpenAI/Anthropic SDKs, pairing in one script tag, built-in usage
  dashboard (calls, cost, models, countries, hours), free during the beta.
- Users: approve on your phone, see the exact prompt and response, set per-app limits, the key never leaves Keyala,
  revoke any time.
Honesty rules: experimental personal project; the operator can decrypt keys to forward approved calls; estimates are
estimates; no claims about features that are not live.

## Console (#4 + #5) contract proposal

Web sign-in, two ways, both ending in a web-scoped session:
1. Phone: `POST /v1/web/login` -> `{login_id, login_secret, qr_url, expires_at}`; the page shows the QR of
   `https://app.keyala.io/login/{id}` (universal link) and polls `GET /v1/web/login/{id}` with `login_secret`.
   Phone: `GET /v1/me/web-login/{id}` (preview: browser, country, created) and `POST .../approve` | `.../deny`.
   Approved poll returns `{session_token}` once. 5 minutes, single use.
2. Password: the phone sets it, `PUT /v1/me/password {password}` (12+ chars, argon2id) / `DELETE /v1/me/password`;
   web signs in with `POST /v1/auth/password {username: account email, password}` (the operator account still works).
Web sessions expire after 12 hours and are limited (fail closed, allowlist): read me/keys(last4)/grants/requests,
revoke a grant, log out, and the developer endpoints below. They can NOT approve or deny requests or connects, add or
replace keys, widen a policy, set a password, approve another web login, or delete the account.
Developer apps owned by accounts: `GET/POST /v1/me/apps`, `POST /v1/me/apps/claim {app_secret}`,
`GET /v1/me/apps/{id}/report?days=` (same shape as `/v1/app/report`), `POST /v1/me/apps/{id}/secret`.
iOS (Codex): approve-web-login screen via `/login/*` universal link and in-app scanner; "Web password" in Settings.
