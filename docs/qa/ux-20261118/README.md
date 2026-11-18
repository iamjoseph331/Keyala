# UX integration checks, 2026-11-18

## Base iOS workflow

- Full original Xcode project built successfully for iPhone 17 Pro / iOS 26.2 with app icon assets included and `CODE_SIGNING_ALLOWED=NO` (local Simulator access required outside the shell sandbox). This supersedes the earlier asset-excluded compile workaround for this pass. It does not verify paid-team device provisioning.
- Fresh simulator launch showed Apple sign-in onboarding, with no sample workspace or server URL input in the normal flow.
- Settings showed Account, Appearance, Notifications and a Developer options entry. The developer switch started off. Enabling it revealed explicit offline sample-data explanation, built-in production/staging selectors and a custom-server fallback.
- Explicitly enabling offline sample data showed its persistent no-provider-calls banner and the Requests / Apps / Keys tabs. Normal operation retains no sample-data banner.
- Keys displayed grey OpenAI `sk-...` and Anthropic `sk-ant-...` hints, corresponding official console links, an explicit Paste control and Universal Clipboard guidance. Save/Paste were disabled for sample data.
- Focused Swift checks cover default migration and strict pairing parser boundaries. Later enrollment checks are recorded below after integration.

These observations used fictional local samples only. No real Apple sign-in, provider keys, model inference, camera capture or APNs delivery was exercised by this UI check. Physical camera, universal-link delivery and the desktop-import round trip require their own verification; a successful build alone is insufficient.

Build log: `/private/tmp/masterkey-ux-full-build.log` (local temporary artifact).

## Desktop-to-native import

A separately compiled QA harness bound only to `127.0.0.1:8093` ran the real Rust app with `MK_VENDOR_MOCK=1`, development sign-in and an isolated temporary database. The simulator used the disposable identity `ux-qa@example.invalid`.

Verified through browser and native controls: choose Anthropic on `/add-key`, open the returned public enrollment identifier in MasterKey, review server/provider, explicitly approve the computer, paste a fictional `sk-ant-api03-FAKE-QA-ABCD` on the computer, see only `ABCD` on the phone, tap Save, and observe **Key saved** on both phone and browser. No plaintext API key appeared in the phone enrollment review. The first unsigned simulator build could not save Keychain sessions (OSStatus -34018); an ad hoc signed simulator build resolved that test setup issue and the sign-in/import loop passed.

This validates custom-scheme dispatch and the actual import state transitions, not physical QR camera capture or hosted universal-link association delivery. No live provider was contacted.

The final signed build also restored the QA session after reinstall/relaunch. A second Anthropic candidate ending `WXYZ` displayed an explicit replacement warning for existing `ABCD`. Cancelling on the phone produced cancellation on both screens, and the native Keys list still showed `ABCD`. Evidence: `desktop-key-saved.png` and `phone-key-replacement-review.png` (fictional QA keys only).

## Interrupted pairing continuation

A disposable app requested OpenAI while the phone had only Anthropic configured. The native connection review showed the unverified-app warning and disabled connection until an OpenAI key existed. From that review, **Add your OpenAI key first → Add from computer** opened key setup. After importing a fictional OpenAI key ending `EFGH`, the phone offered **Continue connecting app**, returned to the original `MK-Z95A` review, and completed **Connect and ask each time** without retyping or rescanning the pairing code. The native **App connected** result and both provider suffixes were visible. No model request was created. `phone-pairing-resumed.png` records the resumed review.


## Final redesign and OpenRouter integration

The signed, full-asset iPhone 17 Pro / iOS 26.2 build passed after the final palette, four tabs, A-style request sheet and Claude's exact green MK icon were integrated. Figma's Starter MCP quota prevented fetching the final frames again; implementation follows Claude's final frame descriptions, exported exact icon source and `docs/ux/final-screens.html`, with native platform controls rather than a pixel-parity claim.

- Real native **Connect OpenRouter** opened `ASWebAuthenticationSession` against the loopback Rust mock. The revised contract from `206ac68` was exercised: a system-window Cancel recorded one `cancelled` OAuth flow; a new explicit attempt returned its matching flow identifier, owner-authenticated GET reported `saved`, key metadata refreshed, and the native provider row appeared. SQLite independently confirmed one cancelled and one saved flow. No live provider was contacted. Reconnecting an existing provider first shows an explicit replacement warning.
- The request sheet displayed full Traditional Chinese, Japanese and English messages, with exact normalized JSON accessible under **Full request**. Deny/Allow stay in a bottom inset while the request content scrolls. The estimate is explicitly not a final-charge guarantee.
- **Swipe to allow** started off. Its alert visibly offers **Keep off** and **Turn on**; Keep off preserved off. After explicit opt-in in this isolated mock workspace, the loaded $0.000118 request offered **Allow once**, while a $0.083280 request and a low-cost summary without payload offered **Review**. The accessible swipe action completed the mock request once; the database showed a response, `request_count=1`, and the grant still in `ask` mode. The test switch was restored off. Exact $0.05 boundary and absent-payload cases are also checked by Swift contract assertions.
- Enabling device verification caused the system passcode challenge before any approval. Backgrounding the test app cancelled the challenge; returning showed **Needs review**, and the database still had `pending` with no response or actual cost. No device passcode was entered. Successful biometric verification on physical hardware remains unverified.
- Light and dark Settings were inspected. Text and controls remained readable; the dark palette uses a lighter green accent. Appearance was restored to System after inspection.

The largest accessibility text size initially split the two action titles into narrow columns. The sheet now stacks its two action buttons at accessibility sizes, and Inbox stacks requester and cost information. A signed rebuild and native visual check confirmed readable Deny/Allow titles; `review-accessibility.png` records the result. Text size was restored to Large.

Evidence: `inbox-redesign.png`, `review-sheet.png`, `openrouter-connected.png`, `swipe-warning.png`, `settings-dark.png`. All identities, keys and prompts are disposable local QA data. The final native contract suite passes. Live OpenRouter account consent/exchange, physical camera, hosted universal-link delivery, Apple login and APNs require their separate device/account checks.


## Independent cross-check and foreground refresh regression

Claude independently built `00c3254` in a clean checkout and verified the final welcome/icon, tabs, OpenRouter mock authorization, first ask-only pairing and review/Allow completion in an iPhone 16 Pro simulator. His check found foreground requests did not appear until relaunch and the connected provider still said Connect.

The native foreground poll had captured the initial scene phase. It now restarts with the scene phase, refreshes immediately while active, repeats at a five-second cadence, and cancels on background. Cancelled refreshes do not mutate state or report a network error. Inbox also refreshes when shown and retains pull-to-refresh. This is foreground polling, not an SSE subscriber or proof of APNs delivery.

Regression check with the rebuilt signed app: the Inbox stayed continuously open at **You’re all caught up**; a fake `Live refresh check` request was created through the actual Rust mock API; the native list changed to **1 waiting for you** without navigation, a deep link, pulling, or restarting. `inbox-live-refresh.png` records the result. A saved OpenRouter provider now displays **OpenRouter connection / Reconnect OpenRouter**. The original explicit replacement warning remains.
