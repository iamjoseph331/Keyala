# iOS custom-domain Phase 2 and footage, 2026-11-20

## Shipped source behavior

- Built-in production/staging are `https://app.keyala.io` and `https://staging.keyala.io`. Exact legacy built-ins (including a root trailing slash) map to the corresponding new origin and persist to UserDefaults. Custom server settings remain unchanged. Sessions remain keyed by their actual origin; no old-origin token is read or copied into the new origin. Sign in again on the new domain.
- Four Associated Domains are present: the two new hosts and the two legacy Render hosts. Keep legacy aliases for this first custom-domain release. Connect and enrollment parsers normalize only those known same-service aliases; production, staging and custom servers remain separate. Both scanned links and pasted HTTPS pairing URLs enforce the environment. Pasted mismatches have an inline explanation and make no preview request.
- Staging password UI accepts the new staging host and its exact legacy alias. OAuth callback contract fixture uses the new staging domain. Existing OAuth owner/flow validation remains intact.
- Native support/privacy contacts in English, Traditional Chinese and Japanese use `support@keyala.io`.
- Joseph selected Claude's `docs/ux/keyala-koala-placeholder.svg`. BrandMark uses its original rounded PNG; AppIcon renders the same vector with a full-bleed green background, 1024x1024 RGB with no alpha. iOS supplies its corner mask. No new artwork was generated.
- Onboarding places a 44pt koala beside the language picker, removes the redundant symbol, reduces gaps/padding/title size, and resets scroll position between steps. All three English pages show their action at standard text size on iPhone 17 Pro. Accessibility text is allowed to scroll; no text-size cap or clipping was added. Largest-size content and accessible Next were checked.

## Verification

- Swift ContractTests passed, including persisted built-in migration, custom setting preservation, same-environment aliases, cross-environment rejection, malformed URL boundaries and the new callback fixture.
- Full-asset ad-hoc signed simulator build passed; no asset exclusion. Full signed device build passed with existing project signing settings. The signed device entitlement contains all four domains and `83APHP8F76.app.sorevan.masterkey`.
- Installed the signed build on the connected iPhone 14 without uninstalling or changing its data. New-domain physical Camera/Universal Link delivery still requires Joseph's observed result; a QR was supplied. Successful installation/AASA is not proof of link delivery.
- Real native simulator workflow against a separately compiled Rust harness bound to 127.0.0.1:8095 with a fresh disposable database: developer sign-in, provider metadata, browser-origin pairing, foreground pending refresh, exact payload review, Allow -> completed, Deny -> denied, history, Ask-each-time permissions and revoke. The full continuous take contains a second explicit pairing and one completed request. No automatic retries or real provider requests.
- Additional native computer-import check: Anthropic enrollment -> explicit claim -> one fake candidate upload -> last4 review -> explicit Save -> Key saved. No full key displayed on the phone.
- Final pasted staging URL while using localhost shows the inline foreign-server rejection. Strict parser/equivalence assertions cover both production/staging directions and their old hosts.
- Both new hosts serve the AASA for the existing Apple ID. User approved Joseph's workspace and the staging-only env update: `KEYALA_PUBLIC_URL=https://staging.keyala.io`; `KEYALA_PUBLIC_APPS` Resume Tailor allows both keyala.io and github.io. Merge update preserved every other env var without reading secret values. Deploy `dep-dc04l2mi0phs73blbor0` is live on service `srv-dbsm5m0ae00c738euib0`, commit `044a7ba`. Public connect with Origin https://keyala.io succeeds and returns a new-domain connect URL. Production configuration/resources were unchanged.
- Render Blueprint remains the source of truth. The current dev render.yaml has these values; main has not been promoted by this work. A main Blueprint sync before that promotion may restore the previous PUBLIC_APPS origin list. Coordinate with Claude before future syncs.

## Recording artifacts

Seven original English H.264 MP4 clips, 1206x2622, no narration/subtitles. Local artifact directory:

`/Users/macbook/.codex/visualizations/2026/11/20/01a1f7ff-d8f9-7852-95c6-6afd4cc09ef6/keyala-phase2/`

`INDEX.md` lists the shots and links to every video/still. Segments are 17.3, 16.6, 23.0, 40.7, 44.5 and 38.2 seconds; uninterrupted full flow is 91.4 seconds. AVFoundation decoded start/middle/end samples of every clip. All recording data is fictional; the mock response and Development server label remain visible. This is not evidence of a successful paid model call. Media is outside git and has not been published.

## Limits and coordination

Physical Universal Links, live OpenRouter consent, Apple deletion, production APNs and hosted country attribution are not established by these mock checks. Country needs an authenticated phone pairing/request on the new domain before checking app reports; no real-account provider call was made here. Backend 0013 retention/purged UI follow-up is separate from this domain task. Claude retains site/README/Phase3; no root README or site changes by Codex.

Initial pull --rebase was blocked by Joseph's pre-existing unstaged changes. InfoPlist.xcstrings, project.pbxproj, workspace and the original entitlement formatting were preserved and excluded from the commit; only the two added domain lines in entitlements are staged.
