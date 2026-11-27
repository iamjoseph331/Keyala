# Keyala web SDK

One script, served by every Keyala server. It talks to the server it was loaded from.

```html
<script src="https://app.keyala.io/sdk/keyala.js"></script>
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

## Sponsored by a coupon

If you issue coupons for your app (Keyala console → your app → Coupons), pass the batch code and users connect without any key of their own:

```js
const { grant_token } = await Keyala.connect({ app: "app_your_app", vendor: "openai", coupon: "KC-<22 characters>" });
// or: <keyala-connect app="app_your_app" vendor="openai" coupon="KC-<22 characters>">
```

The user's phone claims the coupon (once per account) as part of approving the connection. Rules the server enforces:

- A coupon covers only your app, its vendor and the models you listed. Other models fail `403 coupon_model_not_allowed`. The code is case-sensitive; pass it exactly as issued.
- **Every** sponsored call waits for the user's approval on their phone, with the full prompt visible. Sponsored grants cannot be widened to count, budget or trust policies.
- Calls are paid from the batch's dedicated key, never from the user's own keys. If the coupon is expired, revoked or too small for a call you get `410 coupon_expired`, `410 coupon_revoked` or `402 coupon_exhausted`; a code of another app or vendor is `404 coupon_not_found`.
- Each call reserves its worst-case estimate when approved and settles to the real cost; unused balance returns to your batch when the coupon expires. Keyala keeps this account itself: it cannot make the provider enforce a limit on the batch key, so use a key dedicated to the batch and set a provider-side limit too.
- Reports (`GET /v1/me/apps/{app}/coupons/{batch}/report`) contain only counts and amounts: no prompts, no responses and nothing that identifies a user.
