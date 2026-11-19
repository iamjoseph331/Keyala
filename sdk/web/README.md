# MasterKey web SDK

One script, served by every MasterKey server. It talks to the server it was loaded from.

```html
<script src="https://masterkey-prod-vkgm.onrender.com/sdk/masterkey.js"></script>
```

Source: [backend/src/masterkey.js](../../backend/src/masterkey.js) (built into the server).

## Ask when it is needed (recommended)

Call `connect()` the first time the user does something that needs AI, then carry on with that same action.

```js
button.onclick = async () => {
  grant ??= (await MasterKey.connect({ app: "app_your_app", vendor: "openai" })).grant_token;
  const r = await fetch(MasterKey.server + "/v1/openai/chat/completions", {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: "Bearer " + grant },
    body: JSON.stringify({ model: "gpt-4o-mini", max_tokens: 512, messages }),
  });
};
```

## Or a button

```html
<masterkey-connect app="app_your_app" vendor="openai"></masterkey-connect>
<script>
  document.querySelector("masterkey-connect").addEventListener("connected", (e) => save(e.detail.grant_token));
</script>
```

## What the dialog does

- On a computer it shows the QR and the code. On a phone it shows an "Open MasterKey" button instead, since a phone cannot scan its own screen.
- If the phone opens the link without the app installed, the dialog says so ("install, sign in, then scan again"). The code stays valid for 10 minutes.
- Text in English, 繁體中文 or 日本語 (browser language, or `lang`).
- Rejects with `err.code`: `cancelled`, `denied`, `unreachable` or `failed`. The SDK never stores the token; keep it however suits your app.

Browser apps need their page's origin in the app's `allowed_origins`. Keep `max_retries`/`maxRetries` at 0 in official SDKs, or send an `Idempotency-Key`.
