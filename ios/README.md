# Keyala iOS

Native SwiftUI client, iOS 17 or newer. Fresh installs connect to the built-in production service and start with Sign in with Apple. Existing explicit staging/custom server choices are preserved. No server URL entry is needed for normal use. HTTPS is required; HTTP is accepted only for loopback hosts when the development option is enabled.

The four tabs are **Inbox**, **Apps**, **Keys**, and **Settings**. Inbox has Waiting and Activity lists and a primary scanner action. Scan the app website’s connection QR or tap its link on this phone, review the source, then approve. The KY- code remains a fallback. Camera denial and unsupported devices retain code entry. Settings contains account, appearance, notifications and Developer options. In **Inbox → Activity**, tap a past request to inspect its full payload and any recorded provider response.

The request review sheet puts the requester and estimated cost first, followed by readable messages and expandable exact JSON. “This call only” is the default access. Optional device verification uses Face ID, Touch ID or the device passcode before sending an approval. **Swipe to allow** starts off and requires accepting a warning in Settings. It only permits once-only approval of an already-loaded request with an estimate of $0.05 or less; other rows open review. Estimates are not guaranteed final charges. Full swipe is disabled.

**Keys → Paste from your computer** shows the current server’s `/add-key` setup address with a system Share action. Open it on a computer, choose a provider, scan the setup QR with this phone, check the server/provider and explicitly approve the computer. Paste the key on the computer. The phone waits for submission, shows the candidate’s last four characters and any saved key it will replace, then requires a separate Save/Replace action. Closing a setup never confirms it; rescan the same unexpired QR to resume. Explicit cancellation discards an owned candidate. The phone never downloads the full key. If setup started while connecting an app, **Continue connecting app** returns to that pairing after saving; a newer pairing or server change supersedes the old one, and Continue rechecks that the pairing is still pending. Recovering a saved enrollment after a lost response reloads key metadata before offering Continue.

Pasting a key directly on this phone remains a fallback: provider-specific format hints, official console links and a system Paste button. Universal Clipboard can transfer a copied key from a Mac. Saving failure keeps the entered key available to retry; leaving the page, switching providers, or changing accounts clears the field. Keys are never encoded in QR codes.

**Keys → Connect OpenRouter** opens the system authentication session with a suggested $10 limit that can be changed on the consent page. This uses the user's OpenRouter account and credits. The backend owns PKCE and stores the key; the phone matches the completion URL to its flow, then reads the owner-authenticated flow status and key metadata before reporting success. Closing the authentication window cancels a still-pending server flow. An exchange already underway keeps its recorded outcome and is never automatically retried. OpenAI and Anthropic retain computer import or direct paste. A live OpenRouter consent/exchange still requires verification with a real account; the full native loop has been checked against the backend's no-spend mock.

## Signing and device services

The release bundle identifier is `app.sorevan.masterkey`. Register it on the paid team with **Sign in with Apple** and **Push Notifications** enabled, and set the same value for `KEYALA_APPLE_BUNDLE_ID` and `KEYALA_APNS_TOPIC` on the server. Xcode development builds use sandbox APNs. TestFlight and App Store builds use production APNs.

For a Personal Team preview, select **Settings → Developer options → Enable developer options → Use sample data** explicitly. Samples are offline and cannot send keys or model calls. This is separate from server mock mode. For a real approval round trip without provider charges, turn samples off and connect to a private backend with `KEYALA_DEV_LOGIN=1`, `KEYALA_VENDOR_MOCK=1`, and fake keys. The phone cannot enable mock mode on a server. Restrict network access to test devices because development login lets anyone claim any email address. Staging and custom server controls are also under Developer options.

### Temporary physical-device build with a Personal Team

Install the matching iOS component from **Xcode → Settings → Components** if Xcode reports it missing. Connect and unlock the iPhone, trust the Mac if prompted, and enable Developer Mode on the phone. Sign in under **Xcode → Settings → Accounts** and use the Personal Team's ID in the build command. Find the build destination and installation device identifiers with:

```sh
xcodebuild -project ios/Keyala/Keyala.xcodeproj -scheme Keyala -showdestinations
xcrun devicectl list devices
```

Build from a temporary copy so the shipping target keeps its paid capabilities. From the repository root:

```sh
preview_dir=$(mktemp -d /private/tmp/keyala-preview.XXXXXXXX)
cp -R ios/Keyala "$preview_dir/Keyala"
preview_project="$preview_dir/Keyala/Keyala.xcodeproj"
plutil -convert xml1 "$preview_project/project.pbxproj"
/usr/libexec/PlistBuddy \
  -c 'Delete :objects:A10000000000000000000001:attributes:TargetAttributes:A10000000000000000000002:SystemCapabilities' \
  -c 'Delete :objects:A10000000000000000000042:buildSettings:CODE_SIGN_ENTITLEMENTS' \
  -c 'Delete :objects:A10000000000000000000043:buildSettings:CODE_SIGN_ENTITLEMENTS' \
  "$preview_project/project.pbxproj"
/usr/libexec/PlistBuddy -c 'Clear dict' "$preview_dir/Keyala/Keyala.entitlements"

xcodebuild -project "$preview_project" \
  -scheme Keyala -configuration Debug \
  -destination 'id=YOUR_XCODE_DESTINATION_ID' \
  -allowProvisioningUpdates -allowProvisioningDeviceRegistration \
  CODE_SIGN_STYLE=Automatic DEVELOPMENT_TEAM=YOUR_TEAM_ID \
  PRODUCT_BUNDLE_IDENTIFIER=app.sorevan.masterkey.preview \
  -derivedDataPath "$preview_dir/build" build
```

The preview uses `app.sorevan.masterkey.preview` and omits the paid-team entitlements. Sign in with Apple and remote push require the paid team. Install with the device ID listed by `devicectl`:

```sh
xcrun devicectl device install app \
  --device YOUR_COREDEVICE_ID \
  "$preview_dir/build/Build/Products/Debug-iphoneos/Keyala.app"
```

Open Keyala on the iPhone. If iOS asks you to trust your developer profile, select your Apple Account under **Settings → General → VPN & Device Management** and follow the prompts.

Personal Team provisioning expires after seven days. Rebuild and reinstall when it expires.

## Build

```sh
xcodebuild -project ios/Keyala/Keyala.xcodeproj \
  -target Keyala -configuration Debug -sdk iphonesimulator \
  CODE_SIGNING_ALLOWED=YES CODE_SIGN_IDENTITY=- \
  CONFIGURATION_BUILD_DIR=/private/tmp/keyala-ios-products \
  OBJROOT=/private/tmp/keyala-ios-obj \
  SYMROOT=/private/tmp/keyala-ios-sym build
```

An ad-hoc signature enables Keychain sessions in the simulator without an Apple
certificate. If Xcode has a newer SDK than the installed simulator runtimes,
the icon compiler may require a matching runtime. For local UI testing without
that download, append `EXCLUDED_SOURCE_FILE_NAMES=Assets.xcassets`; this skips
the icon catalog only. Install a matching runtime to validate the full icon build.

The `keyala://connect/<code>` URL and HTTPS `/connect/<code>` links on the exact production/staging hosts open the pairing preview. A link for another server is rejected with guidance to change it explicitly in Developer options; scanning never silently switches the server.
`keyala://requests/<request_id>` opens the authenticated request inspector;
notification taps use the same route. Enrollment accepts `keyala://enroll/en_<22 base64url characters>` and HTTPS `/enroll/<id>` on the exact production/staging hosts. Universal links arrive through `NSUserActivityTypeBrowsingWeb`; both QR scanning and link taps enforce the same parser and explicit server matching. Enrollment links remain pending through sign-in. Polling stops when the candidate is submitted, terminal, the view closes, or the session changes; the submitted candidate must be immutable on the backend before a final confirmation is accepted. App identity in the pairing preview is
developer-provided and is not verified by Keyala.

Contract checks run without the simulator:

```sh
swiftc -module-cache-path /private/tmp/keyala-swift-modules \
  ios/Keyala/Models.swift ios/Keyala/Networking.swift \
  ios/Tests/ContractTests.swift -o /private/tmp/keyala-contract-tests
/private/tmp/keyala-contract-tests
```


## First use and account controls

A three-step first-use guide explains the personal experimental service, operator
trust, provider credits, pairing and request review. Completion is stored on this
device; reopen the guide in Settings > Getting started. Pairing and enrollment
links remain available while signing in.

Settings > Account separates ending this device’s server session, signing out
other devices, stopping app permissions and deleting the account. Deletion needs
an explicit DELETE confirmation and fresh Apple verification (identity token and
authorization code), or the test account’s own password/dev identity. Apple
verification is discarded if the account or server context changes. An old saved
session without auth_provider needs a fresh sign-in before deletion is offered.
These controls require the matching backend contract in docs/PLAN.md; an old
deployment does not gain them merely by installing the new app.

Run the native API client account checks against an isolated localhost mock with
development login enabled (never a shared or real-key server):

```sh
swiftc -module-cache-path /private/tmp/keyala-swift-modules \
  ios/Keyala/Models.swift ios/Keyala/Networking.swift \
  ios/Tests/AccountLifecycleChecks.swift -o /private/tmp/keyala-account-checks
/private/tmp/keyala-account-checks http://127.0.0.1:8094
```

The check uses a unique fictional identity and covers other-session revocation,
stop-all, logout, failed reauthentication, deletion and fresh account recreation.
It does not prove live Apple token revocation or physical-device behavior.

## App language and OpenRouter routing

Choose Follow system, English, 繁體中文 or 日本語 in Settings > Language, or on
the first-use guide. The choice takes effect immediately, survives relaunch, and
does not sign out or recreate the current request review. OS-owned Apple sign-in
and permission dialogs follow the iPhone's language settings.

Settings > OpenRouter credits controls a server-side account preference, off by
default. The guide explains data forwarding and billing, can be hidden, and can
be reopened. Existing app connections keep their billing route; reconnect to
apply a different route. Pairing and request details show OpenRouter billing.
Only the server's supported models and text-only API features are eligible.

## keyala.io cutover

Production and staging now default to `https://app.keyala.io` and `https://staging.keyala.io`.
Previously saved built-in Render URLs move to the corresponding new domain on launch and are saved.
Custom servers remain unchanged. Sessions are scoped to their actual origin, so sign in again after
this built-in cutover; tokens are not copied between domains. For the first custom-domain release,
HTTPS connect/enrollment links on each service's old Render host are recognized as that same service.
Production and staging remain isolated, including manually pasted pairing URLs. Apple bundle ID and APNs topic stay `app.sorevan.masterkey`.

The temporary koala app icon and in-app mark use Claude's existing artwork. Getting started fits
one page per step at standard text size on iPhone 17 Pro; accessibility sizes can scroll.
Internal validation records and recording locations are kept locally. Public
walkthroughs and demo footage are in the [user guide](https://keyala.io/guide/).

## Web console sign-in

Open `https://app.keyala.io/console` on the computer. Scan its five-minute QR with
the iPhone Camera or Keyala's scanner. `/login/wl_…` links open a native review
showing the requesting browser's user agent, network country when available,
creation time, expiry, and the active server. These details are hints, not identity
verification. Only approve a QR from your own browser. Production and staging
links stay isolated; no session token appears in a link or QR.

Settings > Web password sets or removes a separate 12–200-character password for
the web console. The username is the account email shown there. Both password
changes and web sign-in approval follow the device authentication setting used by
request approvals. A web session is limited by the backend and expires after
12 hours; spending approvals and key changes remain on the phone.

The standalone integration check requires a disposable localhost backend with
`KEYALA_DEV_LOGIN=1` and `KEYALA_VENDOR_MOCK=1`:

```sh
swiftc -module-cache-path /private/tmp/keyala-swift-modules \
  ios/Keyala/Models.swift ios/Keyala/Networking.swift \
  ios/Tests/WebAccessIntegrationChecks.swift -o /private/tmp/keyala-web-checks
KEYALA_TEST_URL=http://127.0.0.1:8097 /private/tmp/keyala-web-checks
```

It exercises one-time browser-session delivery, denial, password save/removal,
and the server's refusal to let a web session change a password or approve
another browser. All accounts and passwords are generated test data.
