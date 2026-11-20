# Keyala web SDK

One script, served by every Keyala server. It talks to the server it was loaded from.

```html
<script src="https://masterkey-prod-vkgm.onrender.com/sdk/keyala.js"></script>
```

Source: [backend/src/keyala.js](../../backend/src/keyala.js) (built into the server).

## Ask when it is needed (recommended)

Call `connect()` the first time the user does something that needs AI, then carry on with that same action.

```js
button.onclick = async () => {
  grant ??= (await Keyala.connect({ app: "app_your_app", vendor: "openai" })).grant_token;
  const r = await fetch(Keyala.server + "/v1/openai/chat/completions", {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: "Bearer " + grant },
    body: JSON.stringify({ model: "gpt-4o-mini", max_tokens: 512, messages }),
  });
};
```

## Or a button

```html
<keyala-connect app="app_your_app" vendor="openai"></keyala-connect>
<script>
  document.querySelector("keyala-connect").addEventListener("connected", (e) => save(e.detail.grant_token));
</script>
```

## What the dialog does

- On a computer it shows the QR and the code. On a phone it shows an "Open Keyala" button instead, since a phone cannot scan its own screen.
- If the phone opens the link without the app installed, the dialog says so ("install, sign in, then scan again"). The code stays valid for 10 minutes.
- Text in English, 繁體中文 or 日本語 (browser language, or `lang`).
- Rejects with `err.code`: `cancelled`, `denied`, `unreachable` or `failed`. The SDK never stores the token; keep it however suits your app.

## Users who only have an OpenRouter key

Nothing changes in your code. If the user turned on "use my OpenRouter key for OpenAI and Anthropic apps" in Keyala, a connection made while it was on is billed to their OpenRouter key: you keep calling `/v1/openai/chat/completions` or `/v1/anthropic/messages` with the same body and get the same response shape. Only these models are routed (others fail `400 model_not_routable`): `gpt-4o`, `gpt-4o-mini`, `gpt-4.1`, `gpt-4.1-mini`, `claude-opus-5-5`, `claude-sonnet-5-5`, `claude-haiku-5-5`. Routed calls also reject OpenAI `max_completion_tokens`, `n` and the `developer` role, and Anthropic `metadata` and `top_k` (`400 unsupported_option`). The response `model` field then shows the OpenRouter slug (for example `openai/gpt-4o-mini`). Streaming and tools stay unsupported, as for direct keys. The grant JSON tells you nothing extra; the user sees the route before approving.

Browser apps need their page's origin in the app's `allowed_origins`. Keep `max_retries`/`maxRetries` at 0 in official SDKs, or send an `Idempotency-Key`.
