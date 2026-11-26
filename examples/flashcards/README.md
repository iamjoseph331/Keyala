# Flashcards

Pick a language to learn, a level (JLPT N5–N1 for Japanese, CEFR A1–C2 otherwise), a topic and 10/15/20 cards.
The AI makes a deck (word, reading, meaning, example sentence and its translation) and the page shows it as flip cards
with Know / Don't know review. Meanings are in the UI language (English, 繁體中文, 日本語).

One static `index.html`, no backend, no API key. Each deck is **one** `gpt-4o-mini` call through Keyala on the
visitor's own OpenAI key (or their OpenRouter key, if they turned on routing in Keyala). The phone shows the request
and an estimate before it runs; the page shows the same ceiling ("at most $0.0023" for 20 cards).
Plan and test script: [docs/examples-plan.md](../../docs/examples-plan.md).

## Before publishing

1. In `https://app.keyala.io/console` → My apps → Create app: name `Flashcards`, homepage
   `https://keyala.io/examples/flashcards/`, allowed origin `https://keyala.io`.
2. The App ID is in `APP_ID` in `index.html` (`app_EAqJcusVcVz1jPVq`).
3. Add `examples/flashcards` to the copy step and `paths` of `.github/workflows/pages.yml`.

## Run it locally (mock vendor, no real credits)

```sh
sdk/demo.sh --simulate-phone          # terminal 1: Keyala with fake keys; declares app_flashcards for localhost:3001
cd examples && python3 -m http.server 3001
open "http://localhost:3001/flashcards/?ky=http://localhost:8080&app=app_flashcards"
```

The mock vendor answers every call with a plain sentence, so a deck request ends in "The AI's answer couldn't be read
as cards". That is the error path; use the built-in example deck for the review UI.

## The request

`POST /v1/openai/chat/completions` with `model: gpt-4o-mini`, `max_tokens` = 180 per card, `temperature: 0.7`,
`response_format: {"type":"json_object"}` and the card shape spelled out in the system prompt. Keyala accepts only
`text` and `json_object` for `response_format` (not `json_schema`), so `parseDeck()` checks every card and keeps the
valid ones. `X-Keyala-Wait: 0` makes a call that needs a phone tap answer `202` at once; the page then polls
`GET /v1/requests/{id}` and never resends. No retries: "Try again" is a new request, only on the user's tap.

## Stored in the browser

`fc-grant:{server}` (grant token), `fc-decks` (up to 50 decks with last score), `fc-ui`, `fc-pending:{server}` (a
request waiting for approval, so a reload keeps polling). Never the API key.
