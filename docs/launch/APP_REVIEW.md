# TestFlight submission preparation

Status: working checklist, NOT submitted to Apple. Initial beta is free and invite-only, with en/zh-Hant/ja. Provider API usage is separate and is not included in the beta.

Contact: Joseph Chen, josephchen637@gmail.com. Phone/contact fields required by App Store Connect must be provided by Joseph; do not invent them.

## Beta description draft

MasterKey lets you connect your model provider account, pair an AI tool, and inspect the exact request and estimated cost on your iPhone before you allow it. Developers receive permission tokens rather than your original provider API key. You can inspect request history and manage app access. The beta supports OpenAI, Anthropic and OpenRouter with bounded, non-streaming text requests.

What to test: sign-in, provider connection, QR pairing, request review/Allow/Deny, error recovery, account controls, English/Traditional Chinese/Japanese layout and device verification. Do not submit real API keys or private prompt content as feedback.

## Review access plan (gate, not a ready credential)

Claude must provide a restricted reviewer service/account and repeatable pairing/request page using fictional data and no reviewer-owned paid provider key. Keep mock execution isolated from normal users. Do not enable public dev login, embed a shared master key, or ship operator credentials. Document the actual access steps and expiry here once verified. A Review Notes document is not proof that access works.

## Required checks before upload/invitations

- Signed release archive on paid team; version/build number and three localizations present.
- production APNs entitlement/path for TestFlight tested (staging currently uses sandbox push).
- Real device Apple sign-in, language switching, camera, Universal Links, biometric cancellation and success.
- Account deletion including Apple revocation, session invalidation and in-flight cases verified.
- Support/privacy links reachable on the release deployment; App Privacy answers match actual data handling.
- Provider-specific terms/payment-link treatment reviewed against current Apple guidelines; no claim of approval from these notes.
- Beta Review contact and test credentials configured by owner; upload, submission and tester invitations remain explicit final actions.

Official references: https://developer.apple.com/testflight/ ; https://developer.apple.com/help/app-store-connect/test-a-beta-version/invite-external-testers ; https://developer.apple.com/app-store/review/guidelines/ .
