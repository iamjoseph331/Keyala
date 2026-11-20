# Keyala developer site plan

## Purpose

Help developers understand the Keyala approval flow, evaluate its trust boundary,
and try the developer experience without connecting to a server or sending a request.

## Visitor path

1. The hero introduces the product and shows the user approval moment on a phone.
2. The flow explains where the developer, Keyala server, phone, and model provider
   sit in one request.
3. A browser-only approval simulator shows the request payload, fictional estimate,
   choice of approval scope, and inspectable simulated response.
4. OpenAI and Anthropic examples show the one-line base URL change and explain how a
   pending approval differs from the immediate response path.
5. The trust section states that developers never receive user keys, while server
   operators can decrypt them to forward approved requests.
6. Quickstart and repository links give developers a next step and current source.

## Implementation constraints

- Plain HTML, CSS, and JavaScript with no build step or runtime dependencies. The console makes explicit API requests to the server the developer selects.
- All assets use relative paths so the site works at GitHub Pages `/Keyala/`.
- The demo is explicitly simulated and stores nothing. It uses no credentials and makes
  no network requests.
- Code examples use placeholder environment variables and the documented v1 routes.
- The page makes no production readiness, customer, or security guarantee claims.
- Responsive layout, keyboard operation, visible focus, and reduced-motion support.

## Developer console and tutorials

- `developers/` creates a server or browser app and shows the app token once. The token stays in memory and is sent only as a bearer to the selected API origin for reports.
- App reports cover request volume, known model cost, missing usage, status, and country. The sample report is labeled and loaded only on request.
- `tutorials/` documents local mock development, pairing, OpenAI and Anthropic calls, polling, and static browser integration.

## Acceptance checks

- Open `index.html` directly and from a `/Keyala/` project site path.
- Verify simulator approve, deny, inspect, scope changes, and reset work without network
  activity or a key.
- Check narrow viewport layout, keyboard focus, and reduced-motion styling.
- Confirm local links stay within the project site and external links point to the repo.
