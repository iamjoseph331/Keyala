# MasterKey iOS

Native SwiftUI client, iOS 17 or newer. Demo mode starts with local sample data and never calls the server. For a live backend, turn demo mode off, enter the server URL, then sign in. HTTPS is required; HTTP is accepted only for loopback hosts when **Allow HTTP for localhost** is enabled.

## Signing and device services

The app uses bundle identifier `app.masterkey.ios`. Set a team in Xcode and register that bundle ID with **Sign In with Apple** and **Push Notifications** enabled. The project declares both capabilities. Debug signing uses the development APNs environment; Release uses production. APNs also needs matching server-side credentials and device registration. Without those, foreground refresh continues to work. Notifications contain an opaque request ID by default; tapping opens authenticated inspection, never approves a call.

Live Apple sign-in requires the backend’s `/v1/auth/apple` verifier and its Apple audience/team configuration. Development email login requires `MK_DEV_LOGIN=1` on the server. The iOS app embeds neither a server secret nor a vendor key.

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
