# Keyala launch strategy and draft outreach

Decision: free invite-only TestFlight, English/Traditional Chinese/Japanese. Draft material only; no outreach or posts sent. Resume Tailor redesign is not a launch prerequisite and stays with Claude.

## Positioning

For indie developers building small BYOK AI tools, Keyala gives their users a phone approval and audit interface while keeping provider keys away from the developer. Lead with control and inspectable requests. Do not promise zero trust, zero cost, hard invoice caps, or that integration only requires changing a base URL: pairing and asynchronous approval handling are real integration work.

User-facing line: Your AI key. Your permission.
Developer-facing line: Let users bring their provider account, review the exact request, and approve it on their iPhone.

## Initial audience and experiment

Recruit 3 indie developers who already have a working text-based AI tool and 10 iPhone users who already understand API billing. These are proposed pilot targets, not forecast conversion figures. Avoid broad consumer acquisition until onboarding and at least 2 independent integrations are proven.

1. Prepare a 30–45 second real-flow recording: developer page starts request → phone requester/prompt/estimate → Allow → response. Use fictional content; redact keys/tokens. Label mock footage; use live footage only after successful funded tests.
2. Joseph selects prospective partners from his existing network. Offer a short integration session and ask for one concrete workflow. Send the draft invitation only after Joseph authorizes recipient and sending.
3. Pair the first 3 developers manually; give a short QR/poll/idempotency recipe and honest limits (text-only, allowlisted models, non-streaming). Do not advertise compatibility with untested apps.
4. Invite 10 testers through TestFlight after review/signing gates. Observe first-run without coaching, then ask where they hesitated and what permission they expected Allow to grant.
5. After 7–14 days of pilot activity, decide whether to expand based on evidence below. No paid ads initially. No affiliate or referral incentive before usefulness is demonstrated.

## Success measures

Manually record aggregated counts and voluntary feedback; do not store prompts, provider keys or new personal identifiers in a marketing sheet. Use existing aggregate reports where suitable.

- At least 2 developers independently reach a successful end-to-end call.
- At least 8 of the first 10 testers can connect a provider, pair and inspect a request without agent intervention; otherwise fix the observed step.
- At least 5 testers return on another day for a real task.
- All denied/cancelled paths produce zero new upstream calls; no duplicate model executions.
- Record time to first success and support reasons; do not optimize signups at the expense of safe comprehension.

## Channels and timing

Private pilot: personal network and developer communities where Joseph already participates, with permission-based invitations.
Broader release: a technical GitHub README, an integration walkthrough and a short demo. Consider Show HN only when people can try the tool or run it themselves. The official Show HN rules exclude signup-only landing pages and request easy access without email barriers: https://news.ycombinator.com/showhn.html . Invite-only TestFlight alone is therefore not the Show HN launch asset; the runnable open-source backend/sample is.

For Japanese developers, prepare a Japanese technical walkthrough describing pairing, approval latency and token handling. For Traditional Chinese readers, emphasize the difference between app permission, Keyala hosting and provider billing. Product Hunt and paid acquisition are deferred until the pilot has retention and support evidence; no speculative ranking or traffic claims.

## Private invitation drafts

### English

I’m testing Keyala, an open-source way to let users approve an AI tool’s requests from their iPhone without giving the tool their provider API key. I’m looking for a few developers with a small BYOK text app to try an integration. The beta is free; provider API usage is still billed to the user’s provider account. Would you be interested in trying one workflow and telling me where it gets awkward?

### 繁體中文

我在做 Keyala，讓使用者在 iPhone 查看 AI 工具要送出的內容，再決定是否允許使用自己的 API key；工具開發者拿不到原始 key。目前想找幾位有小型 BYOK 文字工具的開發者試接一個流程。Beta 免費，模型費用仍由使用者自己的 provider 帳號支付。你有興趣試用，順便告訴我哪裡不好接或不好懂嗎？

### 日本語

Keyala というオープンソースのツールを開発しています。AI ツールに API キーを渡さず、送信する内容を iPhone で確認してから利用を許可できます。小規模な BYOK のテキストアプリを開発している方に、ひとつの用途で試していただきたいです。ベータ版は無料ですが、API の利用料はユーザー自身のプロバイダーアカウントに請求されます。連携を試して、使いにくい点を教えていただけませんか。

## Public post draft (publish only after gates)

Show HN: Keyala – inspect and approve your AI tools’ API requests from an iPhone

I built Keyala because giving every small AI app a provider API key felt like giving it more access than it needed. Developers receive scoped grants; users can inspect the exact outgoing request and estimated cost, allow a call, and revoke access later. The server is open source and self-hostable. The hosted operator still has access to decrypted keys while forwarding calls; this is not an end-to-end encrypted key vault. The current version supports bounded non-streaming text requests for allowlisted models. [Insert verified runnable quickstart and demo links before posting.]

## Operations contact

josephchen637@gmail.com. Keep support requests free of API keys, authorization headers and private prompts. No guaranteed response-time claim before staffing evidence exists.
