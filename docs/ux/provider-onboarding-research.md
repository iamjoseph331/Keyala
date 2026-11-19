# Provider onboarding: findings and proposed next steps

Verified against official documentation on 2026-11-18. This is research and a proposed workflow, not an implemented API contract. No provider credentials or chargeable requests were used.

## What can replace key entry?

**OpenAI has a keyless route worth pursuing.** Sign in with ChatGPT supports eligible plan usage in open-source tools. Its OSS flow registers a client dynamically and uses PKCE, a stable host identity, validated identity tokens, and separately granted inference permission. The official overview directs paid or remotely hosted applications to an interest form. MasterKey's hosted multi-app gateway is not established as eligible by these documents; open-source licensing alone does not settle that question.

It also needs a separate backend adapter: official plan inference uses Responses with `store:false` and `stream:true`. Current preview limitations exclude `max_output_tokens`; MasterKey v1 requires capped output and rejects streaming. Reusing the API-key input or dropping a bearer token into today's Chat Completions proxy would violate the current contract and its budget assumptions. A local/self-hosted prototype could explore this separately, after deciding the intended host and confirming the relay use case.

Sources: [OSS overview](https://developers.openai.com/siwc/token-sharing-open-source), [sign-in](https://developers.openai.com/siwc/token-sharing-open-source/sign-in), [inference](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference), [preview limits](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations).

**Anthropic also has a documented account-linking alternative.** Claude Platform OAuth beta provides linked billing: the customer authorizes payment from an API organization that pays Anthropic directly. The approving customer needs Admin or Billing privileges. A subscription alone or cloud marketplace billing does not qualify. MasterKey must register an OAuth app, configure workload identity federation and an operator service account, and pass the linked customer's billing identity on each supported request. Requests cannot silently fall back to MasterKey paying when customer billing fails. App testing/review and the gateway's eligibility still need account-specific confirmation. This is an organizational integration, not a way to mint everyone's personal API key.

Sources: [OAuth overview](https://platform.claude.com/docs/en/manage-claude/oauth-apps/overview), [linked billing](https://platform.claude.com/docs/en/manage-claude/oauth-apps/organization-billing-connections), [review](https://platform.claude.com/docs/en/manage-claude/oauth-apps/app-review).

**Automatic personal-key creation is a different question.** OpenAI's project-key management does not create another user's personal key; programmatic creation uses service-account credentials and administrative permissions. Anthropic's Admin API explicitly cannot create API keys. Asking consumers for organization admin keys would add friction and excessive privilege.

Sources: [OpenAI key types](https://developers.openai.com/api/docs/guides/api-keys), [Anthropic Admin API FAQ](https://platform.claude.com/docs/en/manage-claude/admin-api#can-the-admin-api-create-api-keys).

## Immediate app flow

### Approved addition: OpenRouter OAuth

Joseph selected OpenRouter as a third provider during design review. Its official OAuth flow creates a user-controlled OpenRouter key after authorization, using mandatory S256 PKCE. A fresh verifier and returned state bind the attempt; the key must stay encrypted at the server and out of browser storage, URLs and logs. Limit and expiration parameters are suggestions the user can change; the authorization screen's choice remains authoritative.

This uses an OpenRouter account and its credits. It does not link the user's existing OpenAI or Anthropic API account, and the UI must make that distinction clear. Claude owns the third-provider backend and exact contract; Codex owns the native authorization session and provider UI. Live account authorization is distinct from the no-spend mock tests.

Source: [OpenRouter OAuth PKCE](https://openrouter.ai/docs/guides/overview/auth/oauth).

1. Sign in to MasterKey. The official service is preconfigured; server settings belong under developer options.
2. Pick OpenAI or Anthropic. Open the provider's official key page if needed.
3. Copy the key once and use the explicit Paste control. A Mac and iPhone on the same Apple Account can use Universal Clipboard when enabled. Typed entry remains a fallback, with `sk-...` / `sk-ant-...` format hints. Prefix hints do not prove a valid key or available credit.
4. Save explicitly. Retain input on network failure; clear it after success or cancellation. Never automatically read the clipboard or make a paid inference call to validate a key.
5. From the service's desktop page, scan its pairing QR with MasterKey. On the same phone, tap Open MasterKey. Review the source and provider, then approve ask-each-time access. The existing random MK code remains an accessibility/camera fallback.

A four-digit code is not needed after scanning. If later added, it must confirm a separately high-entropy session with short expiry and strict attempt limits, rather than replacing today's pairing identifier with 10,000 possibilities.

## Desktop portal proposal

Recommended fallback for users who cannot use Universal Clipboard:

- Open the official portal on a computer and select the provider. The page requests a short-lived enrollment session and displays a QR containing only its public identifier.
- Scan in the already signed-in phone. Show the official portal origin and selected provider; authorize this one key-import operation. The desktop must not receive the phone's general session token.
- Paste the key on the computer. Upload over HTTPS through a distinct browser capability scoped to this enrollment and provider. Keep it out of URLs, QR payloads, browser storage, analytics and logs.
- Phone shows provider and masked suffix for final confirmation, especially when replacing an existing key. Atomically consume the enrollment once on save; expiry/cancel/replay cannot save a key.
- The browser shows success and discards plaintext. No reveal/export API. Refreshing cannot restore the secret. Pending uploaded candidates must be encrypted, promptly expired and deleted.

The phone still works without a portal. This proposal needs new backend endpoints, exact-origin CORS, polling/expiry/rate limits, session isolation and tests. It must not enable CORS on all `/v1/me` routes or use a developer grant/app secret as a user login. Claude owns the backend; contract changes must be logged and committed together.

## Decisions for the next implementation phase

- Prefer official provider authorization for eligible accounts; keep paste/desktop import for ineligible accounts and self-hosting.
- Confirm whether MasterKey is a hosted gateway or a local agent for the OpenAI integration, and obtain provider confirmation for proxying requests from third-party apps.
- Register Anthropic OAuth and establish a supported workload identity before implementing a live linking button. Do not expose an inert or misleading button now.
- Choose a visual proposal before replacing the shipping icon or applying a full visual redesign.
