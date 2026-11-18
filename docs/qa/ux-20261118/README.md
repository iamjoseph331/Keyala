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
