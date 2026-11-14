# iOS implementation plan

1. Create a minimal iOS 17 SwiftUI Xcode project with a native dark navy, teal, and cream design.
2. Define app-facing models and an isolated API client for the provisional proposal, with clearly marked demo fixtures and no real credentials in demo mode.
3. Build the request inbox and exact-payload inspector with approve/deny actions, request history/detail, key metadata and secure key submission, grant usage/policy/revocation, and connection/session settings backed by Keychain.
4. Add APNs registration and device upload when notification permission is granted, while refreshing on foreground regardless of push availability.
5. Add focused decoding/security checks where practical, then build for an available iOS simulator without code signing.

Contract mapping will remain isolated in the client layer because `docs/API_PROPOSAL.md` is provisional and differs from `docs/PLAN.md`.
