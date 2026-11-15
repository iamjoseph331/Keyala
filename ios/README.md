# MasterKey iOS

Native SwiftUI client, iOS 17 or newer. Demo mode starts with local sample data and never calls the server. For a live backend, turn demo mode off, enter the server URL, then sign in. HTTPS is required; HTTP is accepted only for loopback hosts when **Allow HTTP for localhost** is enabled.

## Signing and device services

The release bundle identifier is `app.sorevan.masterkey`. Register it on the paid team with **Sign in with Apple** and **Push Notifications** enabled, and set the same value for `MK_APPLE_BUNDLE_ID` and `MK_APNS_TOPIC` on the server. Xcode development builds use sandbox APNs. TestFlight and App Store builds use production APNs.

The Personal Team preview starts with offline sample data. For a real approval round trip using mock vendor responses, turn off Demo mode and connect to a private debug backend over trusted HTTPS with `MK_DEV_LOGIN=1`, `MK_VENDOR_MOCK=1`, and fake keys. Restrict network access to the test devices because development login lets anyone claim any email address.

### Temporary physical-device build with a Personal Team

Install the matching iOS component from **Xcode → Settings → Components** if Xcode reports it missing. Connect and unlock the iPhone, trust the Mac if prompted, and enable Developer Mode on the phone. Sign in under **Xcode → Settings → Accounts** and use the Personal Team's ID in the build command. Find the build destination and installation device identifiers with:

```sh
xcodebuild -project ios/MasterKey/MasterKey.xcodeproj -scheme MasterKey -showdestinations
xcrun devicectl list devices
```

Build from a temporary copy so the shipping target keeps its paid capabilities. From the repository root:

```sh
preview_dir=$(mktemp -d /private/tmp/masterkey-preview.XXXXXXXX)
cp -R ios/MasterKey "$preview_dir/MasterKey"
preview_project="$preview_dir/MasterKey/MasterKey.xcodeproj"
plutil -convert xml1 "$preview_project/project.pbxproj"
/usr/libexec/PlistBuddy \
  -c 'Delete :objects:A10000000000000000000001:attributes:TargetAttributes:A10000000000000000000002:SystemCapabilities' \
  -c 'Delete :objects:A10000000000000000000042:buildSettings:CODE_SIGN_ENTITLEMENTS' \
  -c 'Delete :objects:A10000000000000000000043:buildSettings:CODE_SIGN_ENTITLEMENTS' \
  "$preview_project/project.pbxproj"
/usr/libexec/PlistBuddy -c 'Clear dict' "$preview_dir/MasterKey/MasterKey.entitlements"

xcodebuild -project "$preview_project" \
  -scheme MasterKey -configuration Debug \
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
  "$preview_dir/build/Build/Products/Debug-iphoneos/MasterKey.app"
```

Open MasterKey on the iPhone. If iOS asks you to trust your developer profile, select your Apple Account under **Settings → General → VPN & Device Management** and follow the prompts.

Personal Team provisioning expires after seven days. Rebuild and reinstall when it expires.

## Build

```sh
xcodebuild -project ios/MasterKey/MasterKey.xcodeproj \
  -target MasterKey -configuration Debug -sdk iphonesimulator \
  CODE_SIGNING_ALLOWED=YES CODE_SIGN_IDENTITY=- \
  CONFIGURATION_BUILD_DIR=/private/tmp/masterkey-ios-products \
  OBJROOT=/private/tmp/masterkey-ios-obj \
  SYMROOT=/private/tmp/masterkey-ios-sym build
```

An ad-hoc signature enables Keychain sessions in the simulator without an Apple
certificate. If Xcode has a newer SDK than the installed simulator runtimes,
the icon compiler may require a matching runtime. For local UI testing without
that download, append `EXCLUDED_SOURCE_FILE_NAMES=Assets.xcassets`; this skips
the icon catalog only. Install a matching runtime to validate the full icon build.

The `masterkey://connect/<code>` URL opens the pairing preview.
`masterkey://requests/<request_id>` opens the authenticated request inspector;
notification taps use the same route. App identity in the pairing preview is
developer-provided and is not verified by MasterKey.

Contract checks run without the simulator:

```sh
swiftc -module-cache-path /private/tmp/masterkey-swift-modules \
  ios/MasterKey/Models.swift ios/MasterKey/Networking.swift \
  ios/Tests/ContractTests.swift -o /private/tmp/masterkey-contract-tests
/private/tmp/masterkey-contract-tests
```
