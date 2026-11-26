# Two new example apps: plan

Status: plan only, nothing built yet. Written 2026-11-26 against `docs/PLAN.md`, `backend/src/pricing.rs`,
`backend/src/proxy.rs`, `backend/src/keyala.js` and `examples/resume-tailor/` as they are on `dev`.

Two static pages for user testing of Keyala:

| | Daily Reflection | Flashcards |
|---|---|---|
| What Keyala tests | many small calls, each needing approval under `ask`, so testers widen the policy; very personal content | one larger structured (JSON) call; the cost estimate on the phone |
| Provider | OpenRouter (one tap in the Keyala app) | OpenAI |
| Model | `anthropic/claude-haiku-5.5` | `gpt-4o-mini` |
| Endpoint | `POST /v1/openrouter/chat/completions` | `POST /v1/openai/chat/completions` |
| URL | `https://keyala.io/examples/daily-reflection/` | `https://keyala.io/examples/flashcards/` |

## Where they live (recommendation)

**Under `examples/` in this repo**, next to `examples/resume-tailor/`, not as separate repos.

- The Pages workflow already publishes `examples/*` at `https://keyala.io/examples/...` from `main`. A new app needs only
  two lines in `.github/workflows/pages.yml` (see Hosting), with no new repo, Pages setup or domain.
- The model lists and error handling copy the server's allowlist (`pricing.rs`) and contract. Kept in the same repo, a
  contract change and the examples change in one PR, and CI/review sees both. Interview Prepper lives in its own repo
  because it was already an independent project; that reason doesn't apply here.
- Same three UI languages, look and SDK wiring as Resume Tailor, so it is easy to copy from.

Trade-offs to know about:

- All examples share the origin `https://keyala.io` (with `site/`). Each page must prefix its `localStorage` keys
  (`dr-`, `fc-`), and a script injected into any keyala.io page could read every example's grant. Pages load no
  third-party scripts except the Keyala SDK, so the risk is small, but it is shared.
- On the phone the origin is `https://keyala.io`, so testers may read the apps as made by Keyala itself rather than by
  an outside developer. For Flashcards this doesn't matter. For Reflection, the "do I trust whoever runs this?"
  question is mostly about the Keyala operator anyway. If the owner later wants a real third-party feel, the page can
  move to its own GitHub Pages repo unchanged except `APP_ID` and the registered origin.

Ownership: `examples/` is not in the AGENTS.md ownership table (Resume Tailor was done by Claude); `pages.yml` sits with
`site/` (Codex). Whoever builds this records a handoff for the two `pages.yml` lines in the local log.

## Shared design (both apps)

Each app is **one self-contained `index.html`** (plus `README.md`, and the icon/QR SVGs copied from Resume Tailor).
No build step, no framework, no CDN dependencies besides `https://app.keyala.io/sdk/keyala.js`. The ~60 lines of Keyala
plumbing below are copied from `examples/resume-tailor/index.html` (`ky`, `failure`, the 202 loop), not shared as a
library, so a developer can read one file and see everything.

### Server and app id

Same switches as Resume Tailor:

```js
const LOCAL = ['localhost', '127.0.0.1'].includes(location.hostname);
const qs = new URLSearchParams(location.search);
const SERVERS = { prod: 'https://app.keyala.io', staging: 'https://staging.keyala.io' };
const KEYALA_URL = ((LOCAL && qs.get('ky')) || SERVERS[qs.get('server')] || SERVERS.prod).replace(/\/$/, '');
const APP_ID = (LOCAL && qs.get('app')) || 'app_…'; // from the console, see Registration
```

The SDK is loaded from `KEYALA_URL + '/sdk/keyala.js'` (a `<script>` tag for prod; insert it dynamically when
`?ky=`/`?server=` changes the server, as Interview Prepper does).

### Pairing: just in time

No "Connect" step up front. The first time the user presses the button that needs AI (Reflect / Make deck), the page
calls

```js
grant ??= store.get() || (await Keyala.connect({ app: APP_ID, vendor, server: KEYALA_URL, lang: ui })).grant_token;
store.set(grant);
```

and then carries on with that same action. The SDK dialog shows the QR + code on a computer and an "Open Keyala" button
on a phone. A user's first connection to an app can only be approved as **ask** (first-grant protection), so the phone
sheet shows only that; the page says nothing about it. `connect()` rejects with `err.code`:

| `err.code` | Page shows |
|---|---|
| `cancelled` | nothing; the button is enabled again |
| `denied` | "You declined the connection on your phone." |
| `unreachable` | "Could not reach Keyala. It may be waking up; try again in a minute." |
| `failed` | `err.message` |

A small "Connected to Keyala · Disconnect" line in the footer forgets the grant in this browser only ("to revoke it
for good, use Apps in the Keyala app").

### Calling and waiting

Every call: `Content-Type: application/json`, `Authorization: Bearer <grant>`, `Idempotency-Key: <crypto.randomUUID()>`
(one per user action, never reused for a different body), `X-Keyala-Wait: 25`. No automatic retries anywhere.

- `200` → vendor response, verbatim.
- `202 {request_id}` → show the waiting state, poll `GET /v1/requests/{id}` (same grant) every 1.5 s until the status is
  terminal: `completed` (use `response`), `denied`, `expired`, `failed`, `uncertain`. Never resend the POST.
  Pending requests expire on the server after 10 minutes, so polling always ends.
- Save `{request_id, …}` of an in-flight call in `localStorage` and resume polling after a reload, so a paid answer is
  not lost when the tab is reloaded while the phone is in the user's hand. Clear it at any terminal status.

| Outcome | Where it comes from | Page shows | Action offered |
|---|---|---|---|
| waiting | `202`, poll `pending`/`approved`/`sending` | "Approve on your phone. Keyala shows the full text and the estimated cost." + spinner | none (closing the tab is fine; the request expires in 10 min) |
| denied | `403 keyala_denied`, or poll `denied` | "You declined this in Keyala." | "Send again" (new request, new key) |
| expired | `410 keyala_expired`, or poll `expired` | "Not approved within 10 minutes." | "Send again" |
| revoked / server reset | `401 unauthorized` (on the POST or the poll) | "This browser's Keyala connection was revoked or reset." | drop the stored grant; "Connect again" runs `connect()` then resends that action |
| may have run | `504 keyala_uncertain`, or poll `uncertain` | "The provider didn't answer in time. It was not retried; check History in the Keyala app." | none for this call |
| failed | poll `failed`, `502`, other 4xx | `error.message` from the body | "Send again" |
| key missing | `400 no_key`, or poll `failed` with `response.error.type` `no_key` | "Add your {provider} key in the Keyala app, then send again." | "Send again" |

Any other `400` (`model_not_allowed`, `unsupported_option`, `max_tokens_too_large`) means the page is out of date with the
server: show the message and treat it as a bug.

### Languages

UI in English, 繁體中文, 日本語: browser language by default, a picker in the header, remembered in `localStorage`, all
strings in one `T = { en, 'zh-Hant', ja }` table like Resume Tailor. `lang` is passed to `Keyala.connect`. The AI's
language is a separate thing (see each app).

### What is stored in the browser

| Stored (`localStorage`, prefixed per app, keyed by `KEYALA_URL`) | Never stored |
|---|---|
| grant token | the provider API key (the page never has it) |
| the app's own data (journal / decks, progress) | `connect_secret` (the SDK keeps it in memory) |
| UI language | Keyala account details |
| in-flight `request_id` (until terminal) | anything sent to a server other than Keyala |

Every `localStorage` access is in `try/catch`; with storage blocked the app still works for the open tab.

### Cost shown on the page

The page shows the same ceiling the server reserves (`ceilingUSD` from Resume Tailor: UTF-8 bytes of the request JSON +
16 per message + 64 input tokens, plus `max_tokens` output, times the model's price), labelled "at most", so it can be
compared with the estimate on the phone and the actual cost in Keyala History.

## App 1: Daily Reflection

The user writes a few lines about their day; the AI answers with a short, gentle reflection and maybe one question;
they can go on for a few turns, then close the day.

### Screens

1. **Today** (first visit and every visit): date, a short promise ("Your words go to the AI through your own
   OpenRouter account. Nobody else reads them."), a textarea "How was your day?", button **Reflect**. Below: past days
   (date + first line), newest first. Header: language picker, "Powered by Keyala" (opens the same three-step panel as
   Resume Tailor: get the app, add a key or connect OpenRouter with one tap, approve each call).
2. **Connect** (only the first Reflect in this browser): the SDK dialog. While it is open the user's text stays in the
   textarea. After success the call goes out immediately.
3. **Conversation**: the user's message as a bubble, then the assistant bubble in its waiting state (see above). The
   reply appears in place. Under it: a reply box and **Finish for today**.
   - **Widen hint:** the page can't read the grant's policy, but a `202` means the call needed approval. After the
     **second** call in a day that needed approval, show once (dismissible, remembered): "Tired of tapping? On the
     next approval in Keyala pick 'Next calls' (say 10) or 'Budget' (say $0.10) instead of 'Once'."
   - Turn limit: 10 user messages per day. After that the reply box is replaced by "That's a good place to stop for
     today." This bounds the payload (each turn resends the whole day) and the cost.
4. **Past day** (tap an entry): the conversation read-only, with **Delete this day**.
5. **Where your words go** (link in the footer and on Today): see Privacy below, plus **Delete everything in this
   browser** (journal + grant).

### Request

```jsonc
POST {KEYALA_URL}/v1/openrouter/chat/completions
{
  "model": "anthropic/claude-haiku-5.5",
  "max_tokens": 400,
  "temperature": 0.7,
  "messages": [
    { "role": "system", "content": SYSTEM },
    { "role": "user", "content": "<the day's first message>" },
    { "role": "assistant", "content": "<reply>" },
    { "role": "user", "content": "<follow-up>" }
  ]
}
```

Checked against `pricing.rs`: vendor `openrouter`, slug `anthropic/claude-haiku-5.5` is in the table (verified price
$0.10 / $0.50 per 1M); keys `model`, `messages`, `max_tokens`, `temperature` are in `OPENAI_KEYS`; roles
`system`/`user`/`assistant` are allowed; `max_tokens` 400 ≤ cap 4096; text content only; no `stream`, `n`, tools,
`models`, `provider` or `plugins`. Reply text = `choices[0].message.content`.

```text
SYSTEM = You are a warm, calm companion for a short end-of-day reflection. The user tells you about their day.
Reply in 2 to 4 sentences: notice one or two specific things they said, reflect them back kindly, and when it fits,
ask one gentle open question. Do not give advice unless asked, do not diagnose, do not use lists or headings.
Reply in the language the user writes in. You are not a therapist. If the user mentions wanting to hurt themselves
or someone else, respond with care and encourage them to contact someone they trust or local emergency services.
```

Cost: the last turn of a full day is about 6 KB of JSON ≈ 6,400 input tokens + 400 output → ceiling ≈ $0.001. A full
day of 10 turns is under $0.01.

Vendor: OpenRouter only, on purpose (it is the provider this app exists to exercise, and a tester without a key gets
one with one tap). `Keyala.connect({ vendor: 'openrouter' })`. Skipped: a provider/model picker; add one if testers
without OpenRouter ask.

### Stored / not stored

`dr-grant:{KEYALA_URL}`, `dr-journal` (`[{date, messages:[{role, content}]}]`, the day's messages), `dr-ui`,
`dr-pending`, `dr-hint-seen`. Nothing is sent anywhere except Keyala.

### Privacy panel ("Where your words go"), stated honestly

- The page has no server. Your journal is stored only in this browser; delete it here at any time.
- Each message goes through Keyala to OpenRouter and on to the model provider, using your key. This page's author never
  sees it.
- Keyala keeps the text of each call (encrypted) so you can review it in the app, and deletes it 30 days after the call.
  Cost and token counts stay. Deleting your Keyala account removes it at once. There is no way yet to delete a single
  call earlier.
- OpenRouter and the model provider handle your text under their own policies.

(The missing per-request deletion is a finding to record if testers ask for it, not something to build now.)

### Size

`index.html` ≈ 550–650 lines (≈ 150 of them the three string tables), `README.md` ≈ 60 lines.

## App 2: Flashcards

The user picks a target language, a level and a topic; the AI makes a deck of vocabulary cards; the user reviews them
as flip cards with Know / Don't know.

### Screens

1. **New deck**: target language (English, 日本語, 한국어, 繁體中文, Español, Français, Deutsch), level (JLPT N5–N1 when the
   target is Japanese, CEFR A1–C2 otherwise), topic (free text, with a few chips such as Food, Travel, Work, Feelings),
   number of cards (10 / 15 / 20). Under the button: "At most $0.003 on your OpenAI account. Your phone shows the
   exact request and estimate before anything runs." Button **Make deck**. On first visit a built-in 5-card example
   deck is shown under the form ("Try the example"), which also lets the UI be developed with the mock vendor.
2. **Connect** (first Make deck only): the SDK dialog, then the call goes out.
3. **Making the deck**: waiting state; "Approve on your phone." The ceiling is repeated here so the tester can compare it
   with the phone.
4. **Review**: one card at a time. Front: the word (and the reading, which can be hidden). Tap / Space flips. Back:
   meaning, example sentence, its translation. Buttons **Don't know** (←) and **Know** (→). Progress "7 / 20". Cards
   marked Don't know go back to the end of the round. End screen: "You knew 14 of 20", **Review the 6 missed**, **New
   deck**.
5. **My decks**: saved decks (language, level, topic, date, last score), open or delete.

Skipped: spaced repetition, audio, editing cards, sharing decks. Add when testers ask for them.

### Request

```jsonc
POST {KEYALA_URL}/v1/openai/chat/completions
{
  "model": "gpt-4o-mini",
  "max_tokens": 3600,              // 20 cards; 10 → 1800, 15 → 2700 (180 per card)
  "temperature": 0.7,
  "response_format": { "type": "json_object" },
  "messages": [
    { "role": "system", "content": SYSTEM },
    { "role": "user", "content": "Target language: Japanese. Level: JLPT N4. Topic: cooking at home. Cards: 20. Explain meanings in: English." }
  ]
}
```

Checked against `pricing.rs`: `openai` / `gpt-4o-mini` is allowlisted ($0.15 / $0.60 per 1M); `response_format` is
allowed **only as `{"type":"text"}` or `{"type":"json_object"}`** (`json_schema` is rejected as
`unsupported_option: response_format.type`), so the schema goes in the prompt and the page validates the result;
`max_tokens` ≤ 4096; no `n`, `stream` or tools. OpenAI requires the word "JSON" in the messages when `json_object` is
used; the system prompt has it. If the user turned on "use my OpenRouter key for OpenAI apps" in Keyala, the same
request is routed to `openai/gpt-4o-mini` unchanged (`response_format` is not on the routed reject list).

```text
SYSTEM = You make vocabulary flashcards for language learners. Answer with JSON only, exactly this shape:
{"cards":[{"word":"","reading":"","meaning":"","example":"","example_meaning":""}]}
- word: a word or short phrase in the target language that suits the level and the topic; no duplicates.
- reading: pronunciation help: hiragana for Japanese, pinyin for Chinese, romanization for Korean, IPA otherwise.
- meaning: a short meaning in the explanation language.
- example: one natural sentence in the target language at the same level, using the word.
- example_meaning: that sentence in the explanation language.
Make exactly the requested number of cards.
```

The explanation language is the UI language (English / Traditional Chinese / Japanese).

Parsing (`parseDeck(text)`, a pure function): `JSON.parse` the content; keep cards whose five fields are non-empty
strings (`reading` may be empty), trim, drop duplicates by `word`, at most the requested count. Fewer than the requested
count is fine (say "Made 18 cards"). Zero valid cards or invalid JSON → "The AI's answer couldn't be read as cards." with
**Try again** (a new, paid request, so only on the user's tap). The response also has `finish_reason`; `length` means
the cap cut it off, which the per-card budget should prevent; if it happens, the cards before the cut are lost (the JSON
is invalid), so raise the per-card budget rather than repair JSON.

Cost: input ≈ 1.2 KB ≈ 1,300 tokens, output cap 3,600 → ceiling ≈ $0.0024 for 20 cards; real usage is usually about
half. This is the number to compare with the phone.

### Stored / not stored

`fc-grant:{KEYALA_URL}`, `fc-decks` (`[{id, created_at, language, level, topic, cards, last_score}]`, at most 50, the
oldest dropped), `fc-ui`, `fc-pending`. Decks are not personal, so no special privacy panel; the footer says decks stay
in this browser.

### Size

`index.html` ≈ 650–800 lines (flip card CSS, review loop, three string tables), `README.md` ≈ 60 lines.

## Registration (owner, once per app)

1. Sign in at `https://app.keyala.io/console` (phone QR or web password) → **My apps** → **Create app**.
   - Name: `Daily Reflection` / `Flashcards` (this is what the phone shows on the connect and approval sheets).
   - Homepage: `https://keyala.io/examples/daily-reflection/` / `https://keyala.io/examples/flashcards/`.
   - Allowed origins: `https://keyala.io` (one origin covers every path on the site).
2. Copy the **App ID** (`app_…`) into `APP_ID` in that app's `index.html`. The app secret shown once is not used by a
   static page; keep it in a password manager in case it is needed for `GET /v1/app/report` (the console already shows
   reports, so it is not needed for testing).
3. Prod has a persistent disk, so console apps survive deploys. Staging resets its database on every
   deploy/restart/sleep, so a console app made there disappears: test on prod, or on staging with an app declared in
   `KEYALA_PUBLIC_APPS` (as `app_resume_tailor` is). Not needed for the planned user testing.

Local development: `sdk/demo.sh --simulate-phone` (mock vendor, no credits) declares `app_resume_tailor` for
`http://localhost:3001`; serve the example on port 3001 and open
`http://localhost:3001/?ky=http://localhost:8080&app=app_resume_tailor`. The phone screens then say "Resume Tailor",
which is fine locally. The mock answers every call with a fixed sentence: Reflection shows it as the reply; Flashcards
shows the "couldn't be read" error, which tests that path (the example deck covers the review UI).

## Hosting

GitHub Pages from `main`, as Resume Tailor. In `.github/workflows/pages.yml`:

- add `"examples/daily-reflection/**", "examples/flashcards/**"` to the `paths` trigger;
- add `cp -R examples/daily-reflection examples/flashcards _site/examples/` next to the resume-tailor copy.

Release path as usual: `dev` → PR to `staging` → PR to `main`. Optionally list both apps on the site's examples/docs
page (Codex's `site/`), after testing.

## Test script (human tester, iPhone with Keyala + a computer)

Before: Keyala installed and signed in; for Flashcards an OpenAI key added in Keyala; for Reflection no OpenRouter key
yet (step 2 adds it). Note the time of each step; the facilitator records answers to the questions in *italics*.

### Daily Reflection

1. Open `https://keyala.io/examples/daily-reflection/` on the computer. Write three lines about your day, press
   **Reflect**. *Expected:* the Keyala dialog with a QR.
2. In Keyala, connect OpenRouter with one tap if asked (spending cap of your choice), then scan the QR. *Expected:*
   the sheet shows "Daily Reflection", origin `https://keyala.io`, provider OpenRouter, and only "Ask each time".
   Approve.
3. *Expected:* an approval request on the phone right away, showing your text and an estimate. Read it, approve.
   The reply appears on the page. *Did you read the text on the phone? Did the estimate mean anything to you?*
4. Reply twice more, approving each. *Expected:* after the second approval the page shows the "Tired of tapping?" hint.
   *How did it feel to approve every message?*
5. On the next approval change Allow from "Once" to "Next calls" (10) or "Budget" ($0.10). Send two more replies. *Expected:* answers come
   back with no phone prompt.
6. Deny one: in Keyala, set Daily Reflection back to ask each time (or wait until the count/budget is used up), send a message, tap Deny.
   *Expected:* "You declined this in Keyala." and **Send again**.
7. Send a message and don't touch the phone for 10 minutes. *Expected:* "Not approved within 10 minutes."
8. Reload the page while a request is waiting, then approve on the phone. *Expected:* the reply still appears.
9. Open History in Keyala: find today's calls with the full text and reply, estimated and actual cost.
10. In Keyala revoke Daily Reflection (Apps). Send a message. *Expected:* "connection was revoked", **Connect again**
    → new QR → after approving, the message goes out.
11. Open "Where your words go". *Who do you think can read what you wrote? Does 30 days feel right? Would you write
    about something private here?* Press **Delete everything in this browser**; the journal is gone.
12. Repeat step 1 on the phone's own browser. *Expected:* an "Open Keyala" button instead of a QR.
13. Switch the page to 繁體中文 and 日本語; write one message in that language. *Expected:* UI and reply in that language.

### Flashcards

1. Open `https://keyala.io/examples/flashcards/`. Try the example deck (no Keyala needed): flip, Know, Don't know,
   the missed round.
2. Choose 日本語, JLPT N5, topic "food", 20 cards. Note the "at most $…" line. Press **Make deck**, scan, approve the
   connection (Ask only).
3. *Expected:* the request on the phone with the prompt and an estimate close to the page's number. *Did you compare
   them? Was the number reassuring, confusing or irrelevant?* Approve.
4. *Expected:* about 20 cards with kana readings and English meanings. Review all; check the end screen and the
   missed round.
5. In Keyala History, compare estimate and actual cost of that call. *Expected:* actual well below the estimate.
6. Make a deck in another language (Español, B1, "travel", 10 cards) with the UI in 繁體中文. *Expected:* meanings in
   Traditional Chinese, IPA readings.
7. Deny one deck request on the phone. *Expected:* "You declined this in Keyala." and the form is unchanged.
8. Reload the page: the decks are still in **My decks**; delete one.
9. *Would you set a budget for this app? How much?*

## Effort

With a smaller model doing the build from this plan: about half a day per app, plus an hour of testing on a real phone
each (prod, real keys, a few cents). Registration and the `pages.yml` change: 15 minutes. Build Flashcards first: it is
one call and fewer states, and its waiting/error code can then be copied into Reflection.

## Decisions taken here (change if the owner disagrees)

- Both apps under `examples/`, served from keyala.io.
- Reflection: OpenRouter only, `anthropic/claude-haiku-5.5`, 10 turns per day, journal kept in the browser by default
  with delete controls.
- Flashcards: OpenAI only, `gpt-4o-mini`, JSON via `json_object` + prompt schema, 10/15/20 cards, no spaced
  repetition.
- No per-request deletion or server changes; the apps use the contract as it is.
