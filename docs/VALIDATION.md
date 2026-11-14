# Validation record

2026-11-14, Asia/Tokyo. All provider calls below used the mock adapter and fake keys.
The cost figures in screenshots are test data, not current vendor pricing claims.

## Verified

- Rust backend: Claude reports 31 integration tests and 3 unit tests passing,
  plus clippy. Official OpenAI Python 3.51.0 and Anthropic Python 2.8.0 clients
  passed the mock-provider smoke script in `backend/tests/sdk_smoke.py`.
- Native iOS: signed ad hoc, compiled with Xcode 26.2's simulator SDK, installed
  and run on the existing iPhone 16 Pro simulator with iOS 18.4.
- Native contract check executable passed: list/detail shapes, micro-USD values,
  key/grant decoding, query URLs, remote HTTP rejection, exact 64-bit JSON
  integer display, multilingual text, and oversized-cost rejection.
- Native app against the Rust server on localhost:8091: development login,
  Keychain save and restoration after app replacement, pairing-code preview and
  approval, full multilingual payload inspection, approve once, and response/cost
  inspection. The developer and user APIs returned the same completed response.
  The stored payload matched the submitted object and the grant counted one call.
- Native denial of a second request: both APIs reported denied, response and
  actual cost stayed null, and the provider call count remained one.
- A simulated notification was accepted with `simctl push`. Authenticated request
  deep links opened the same request inspector used by notification routing.
- Developer page: approval, denial, reset, scope choice, full payload/response
  inspection, and provider tabs checked in the browser. Desktop and 390px mobile
  layout checked; JavaScript syntax check and whitespace checks passed.
- GitHub Pages deployment 43593467586 succeeded and its public page was opened
  in the browser. Later source commits trigger the same deployment workflow.

## Environment limits and remaining checks

- This host has the iOS 26.2 SDK and older simulator runtimes. The icon compiler
  requires a matching newer runtime even for a device build. Simulator validation
  used `EXCLUDED_SOURCE_FILE_NAMES=Assets.xcassets` to avoid downloading another
  runtime. Icon source, PNG dimensions, asset JSON, and project references were
  checked; a full asset-catalog build remains unverified.
- Signed physical-device installation, real Apple identity tokens, actual APNs
  delivery, and tapping a delivered notification remain unverified. The simulator
  gesture-control tool could not open Notification Center reliably.
- No live vendor call or real provider charge was made. The operator must verify
  model availability and prices before enabling live forwarding.
- Complete history paging and the single-user demo's loopback/startup safeguards
  are requested in the shared agent log and pending at this milestone.

## Reproduce

See `ios/README.md` for simulator build and contract-check commands, and run
`bash sdk/demo.sh` for the disposable local backend. Use the iOS app for consent;
`--simulate-phone` exercises a different, scripted approval path.

`site/media/ios-request.jpg` and `ios-completed.jpg` are unedited captures of the
native inspector during the local mock test. No real credentials are shown.
