# Daily Reflection

Write a few lines about your day; the AI answers with a short, gentle reflection and maybe one question; keep talking
for up to 10 messages, then close the day. Past days stay in the browser. UI in English, 繁體中文, 日本語; the AI answers in
the language you write in.

One static `index.html`, no backend, no API key. Every message is one call to `anthropic/claude-haiku-5.5` through
Keyala on the visitor's own OpenRouter key (one tap in the Keyala app). The first connection is "ask each time", so
every message needs a tap on the phone; after the second one the page suggests choosing "Next calls" or "Budget". That,
and the very personal content, is what this example is for. Plan and test script:
[docs/examples-plan.md](../../docs/examples-plan.md).

## Before publishing

1. In `https://app.keyala.io/console` → My apps → Create app: name `Daily Reflection`, homepage
   `https://keyala.io/examples/daily-reflection/`, allowed origin `https://keyala.io`.
2. Put the App ID in `APP_ID` in `index.html` (it is `app_REPLACE_WITH_CONSOLE_ID` until then, and connecting fails).
3. Add `examples/daily-reflection` to the copy step and `paths` of `.github/workflows/pages.yml`.

## Run it locally (mock vendor, no real credits)

```sh
sdk/demo.sh --simulate-phone          # terminal 1: fake OpenAI/Anthropic/OpenRouter keys; declares app_daily_reflection
cd examples && python3 -m http.server 3001
open "http://localhost:3001/daily-reflection/?ky=http://localhost:8080&app=app_daily_reflection"
```

Every reply is "Hello from the Keyala mock vendor." To try denial, deny the pending request in the Keyala app (or with
`POST /v1/me/requests/{id}/deny`) before the simulated phone approves it (`SIM_DELAY`, default 3 s).

## The request

`POST /v1/openrouter/chat/completions` with `model: anthropic/claude-haiku-5.5`, `max_tokens: 400`, `temperature: 0.7`
and `messages` = system prompt + the whole day so far (system/user/assistant roles). About $0.001 at most per message.
`X-Keyala-Wait: 0`, so a call that needs a tap answers `202` at once (that is also how the page knows to show the hint);
the page polls `GET /v1/requests/{id}` and never resends. If a call is denied, expires or fails, the message goes back
into the text box.

## Stored in the browser

`dr-grant:{server}`, `dr-journal` (`[{date, messages, closed?}]`), `dr-ui`, `dr-hint-seen`, `dr-pending:{server}` (a
reply waiting for approval, so a reload keeps polling). "Where your words go" explains what Keyala keeps (encrypted, 30
days) and has "Delete everything in this browser". Never the API key.
