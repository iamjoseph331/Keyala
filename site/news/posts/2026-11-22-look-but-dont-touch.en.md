---
title: "Look, don't touch: how Keyala's web sessions stay read-only"
date: 2026-11-22
category: tech
summary: The web console can show your history and revoke apps, but it can never approve a call. Here is how that rule is enforced on the server, and why it fails closed.
---
Keyala's phone app is where money is spent: it approves calls that run on your API key. When we added a web console, the question was how much of that power a browser should get. The answer we chose is "almost none".

## Two kinds of sessions

Every session now has a scope: `app` for the iPhone app, `web` for the console. A web session also has an expiry, 12 hours after sign-in. The phone's session does not expire this way, because the phone is the device that holds the approval power, and it can require Face ID before each approval.

## Fail closed, not fail open

The easy way to build this is a list of things a web session may **not** do. The problem with a deny list is that every new endpoint is open to the web by default, and one forgotten line becomes a hole.

So we did the opposite. Every handler that needs a signed-in user asks for an access level:

```rust
let user = auth_level(&state, &headers, READ).await?; // web sessions allowed
let user = auth_session(&state, &headers).await?;    // phone only (ACT)
```

`auth_session` is the default and refuses web sessions with `403 web_session_not_allowed`. A handler has to opt in with `READ` to be reachable from the browser. Today that list is short: read your profile, keys (last four characters), grants, requests, preferences and live events; revoke a grant; sign out; and manage your own developer apps. Approving or denying anything, changing keys or policies, starting OAuth, setting a password, approving another web sign-in and deleting the account all stay phone-only.

Revoking is allowed on purpose: it can only make things safer.

## Signing in by phone

A web sign-in works like the pairing flow. The page asks the server for a short-lived sign-in (five minutes) and shows its QR code. The page polls with a secret that only it knows, so seeing the QR is not enough to collect the session. The phone shows what the browser claims to be (user agent and country, which a browser can fake) and asks you to approve. The session token is handed out exactly once.

The weak point of any QR sign-in is phishing: someone sends you their QR and asks you to approve it. That is why the approval screen says to approve only a sign-in you started yourself, and why even a successful phish only gets a read-only, 12-hour session that cannot spend.

## Passwords, if you want them

You can also set a web password in the app. It is stored as an Argon2id hash, and checking it takes the same time whether or not the account exists. A password sign-in gets the same limited web scope; it can never become a phone session.
