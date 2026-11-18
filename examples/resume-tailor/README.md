# Resume Tailor

A one-page AI app that tailors a resume to one or more job descriptions, with an optional
translation, a chosen tone, and any extra experience the user wants to add.

**It ships with no API key and no backend.** It is a single static `index.html`. Each visitor
connects their own OpenAI, Anthropic or OpenRouter key through MasterKey (OpenRouter is one tap in the app, no key to copy), picks any model they like (they pay
for it, so it's their call), and approves every call on their phone after seeing the exact prompt
and an estimated cost. The person who publishes the page never holds anyone's key, runs no
server, and never gets a token bill.

Live: <https://iamjoseph331.github.io/MasterKey/examples/resume-tailor/> (published from `main`
by the Pages workflow; it talks to `masterkey-prod`).

The flow is connect-first: step 1 picks the provider and pairs with the MasterKey app (QR, code, or
"Open MasterKey" on the phone), and the model list only offers connected providers. Before running,
the page shows the per-job cost ceiling computed exactly as the server reserves it (request bytes +
16 per message + 64, plus `max_tokens` of output), so it matches the estimate on the phone. The UI
is in English, Traditional Chinese and Japanese (browser language, or the picker; remembered).
The "Get MasterKey" QR (`masterkey-qr.svg`) points to the MasterKey site until there is an App
Store listing; swap the file then.

## Run it locally (mock vendor, no real credits)

```sh
# terminal 1: MasterKey with fake keys, the Resume Tailor app declared, and a script that plays the phone
sdk/demo.sh --simulate-phone
# drop --simulate-phone to approve on the MasterKey iPhone app instead (dev login: demo@masterkey.dev)

# terminal 2: any static file server on port 3001
cd examples/resume-tailor && python3 -m http.server 3001
open "http://localhost:3001/?mk=http://localhost:8080"
```

`?mk=` (MasterKey URL) and `?app=` (app id) are honoured on localhost only; anywhere else the page
uses `masterkey-prod` and `app_resume_tailor`. `?server=staging` (any host) switches to
`masterkey-staging`, which has OpenRouter before prod does. The mock vendor answers every call with a fixed
sentence, so the output pane shows that instead of a resume.

## How a static page uses MasterKey

1. **Register a browser app** with the origins it is served from, either
   `POST /v1/apps {"name": "...", "allowed_origins": ["https://you.github.io"]}`, or as the
   operator, in `MK_PUBLIC_APPS` (re-created on every start, which matters on Render's free plan
   where the database resets). There is no app secret to hide.
2. **Connect:** `POST /v1/connect {app_id, vendor}` from the page. The browser's `Origin` must be
   allowed. You get a `connect_code` to show the user and a `connect_secret` to keep.
3. **Claim the grant:** poll `GET /v1/connect/{code}` with `Authorization: Bearer <connect_secret>`
   until the user approves; the response carries the `grant_token` once. Store it (this page uses
   `localStorage`).
4. **Call the model:** a normal OpenAI or Anthropic request body (OpenRouter: the OpenAI shape at `/v1/openrouter/chat/completions` with an exact slug such as `anthropic/claude-haiku-5.5`), sent to MasterKey with the grant:

```js
fetch(MK_URL + '/v1/anthropic/messages', {
  method: 'POST',
  headers: { Authorization: 'Bearer ' + grant, 'Content-Type': 'application/json',
             'Idempotency-Key': crypto.randomUUID(), 'X-MasterKey-Wait': '25' },
  body: JSON.stringify({ model, max_tokens, system, messages }),
})
```

A `202` means the user hasn't approved yet: poll `GET /v1/requests/{id}` with the grant. Never
resend the POST (the `Idempotency-Key` makes an accidental resend harmless anyway). A `401` means
the grant was revoked or the server was reset, so drop it and connect again.

## The page

- A hero with a **"Powered by MasterKey"** panel (top right): get the app, add your key once,
  approve each request. The App Store QR is a placeholder: replace the SVG inside `.qr .box` in
  `index.html` with `<img src="appstore-qr.png" alt="...">` once the app is live.
- The result pane shows an **example tailored resume** until the first run, plus a before/after
  section, features and "how it works" below the tool.
- Resume files are read in the browser (PDF via pdf.js, DOCX via mammoth, from jsDelivr); only the
  extracted text goes into the prompt. Scanned PDFs have no text, so users paste instead.
- Output is Markdown: copy it, download `.md`, or Print / PDF. Light and dark mode, phone width.

## Limits, honestly

- The `Origin` check is enforced by browsers only. A script elsewhere could start a connect that
  looks like this app, which is why the phone shows the origin and every call still follows the
  policy the user approved. (A server app's name isn't verified either: anyone can register one.)
- The grant sits in `localStorage`, so a script injected into the page could use it. The page only
  loads pinned CDN files, and with the default "ask" policy each call still needs the user's tap.
- The model list must match the server's allowlist (`backend/src/pricing.rs`); prices shown are
  display only.
