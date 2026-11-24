import Foundation

@main
enum ContractTests {
    static func main() throws {
        let loginID = "wl_abcdefghijklmnopqrstuv"
        for host in [AppPreferences.productionURL, AppPreferences.stagingURL] {
            precondition(WebLoginLink.id(from: host + "/login/" + loginID) == loginID)
        }
        precondition(WebLoginLink.id(from: "keyala://login/" + loginID) == loginID)
        precondition(!PairingLink.matchesServer(AppPreferences.stagingURL + "/login/" + loginID, activeServer: AppPreferences.productionURL))
        for link in ["https://evil.invalid/login/" + loginID, "https://app.keyala.io/login/" + loginID + "?token=bad", "https://user@app.keyala.io/login/" + loginID, "https://app.keyala.io/login/short", "keyala://login/" + loginID + "/extra", "keyala://login/%77l_abcdefghijklmnopqrstuv", "keyala://login/" + loginID + "#secret"] {
            precondition(WebLoginLink.id(from: link) == nil, "Reject malformed or untrusted web-login links")
        }
        let pendingLogin = try JSONDecoder().decode(WebLoginPreview.self, from: Data(#"{"user_agent":"Safari","country":"JP","created_at":"2026-11-20T10:00:00Z","expires_at":"2026-11-20T10:05:00Z","status":"pending"}"#.utf8))
        let loginTime = ISO8601DateFormatter().date(from: "2026-11-20T10:04:00Z")!
        precondition(pendingLogin.isPending(at: loginTime))
        precondition(!pendingLogin.isPending(at: loginTime.addingTimeInterval(60)))
        let malformedLogin = try JSONDecoder().decode(WebLoginPreview.self, from: Data(#"{"created_at":"unknown","expires_at":"unknown","status":"pending"}"#.utf8))
        precondition(!malformedLogin.canApprove)
        let preferenceSuite = "keyala-tests-" + UUID().uuidString
        let preferences = UserDefaults(suiteName: preferenceSuite)!
        defer { preferences.removePersistentDomain(forName: preferenceSuite) }
        precondition(AppPreferences.load(preferences).serverURL == AppPreferences.productionURL)
        precondition(!AppPreferences.load(preferences).sampleData && !AppPreferences.load(preferences).developerOptions)
        preferences.set(true, forKey: "demoMode")
        preferences.set("https://localhost:8443", forKey: "serverURL")
        precondition(AppPreferences.load(preferences).serverURL == AppPreferences.productionURL && !AppPreferences.load(preferences).sampleData)
        preferences.set("https://user:secret@example.invalid", forKey: "serverURL")
        precondition(AppPreferences.load(preferences).serverURL == AppPreferences.productionURL)
        preferences.set(AppPreferences.stagingURL, forKey: "serverURL")
        precondition(AppPreferences.load(preferences).serverURL == AppPreferences.stagingURL && AppPreferences.load(preferences).developerOptions)
        for (old, new) in [(AppPreferences.legacyProductionURL, AppPreferences.productionURL), (AppPreferences.legacyStagingURL, AppPreferences.stagingURL)] {
            preferences.set(old, forKey: "serverURL")
            precondition(AppPreferences.load(preferences).serverURL == new)
            precondition(preferences.string(forKey: "serverURL") == new, "Persist the built-in domain cutover")
            precondition(PairingLink.serverURL(from: old + "/connect/KY-4F7Q") == new)
            precondition(PairingLink.code(from: old + "/connect/KY-4F7Q") == "KY-4F7Q")
            precondition(EnrollmentLink.id(from: old + "/enroll/en_abcdefghijklmnopqrstuv") == "en_abcdefghijklmnopqrstuv")
        }
        for host in [AppPreferences.productionURL, AppPreferences.legacyProductionURL] {
            let link = host + "/connect/KY-4F7Q"
            precondition(PairingLink.matchesServer(link, activeServer: AppPreferences.productionURL))
            precondition(PairingLink.matchesServer(link, activeServer: AppPreferences.legacyProductionURL))
            precondition(!PairingLink.matchesServer(link, activeServer: AppPreferences.stagingURL))
            precondition(!PairingLink.matchesServer(link, activeServer: "http://127.0.0.1:8095"))
        }
        for host in [AppPreferences.stagingURL, AppPreferences.legacyStagingURL] {
            precondition(PairingLink.matchesServer(host + "/enroll/en_abcdefghijklmnopqrstuv", activeServer: AppPreferences.stagingURL))
            precondition(!PairingLink.matchesServer(host + "/connect/KY-4F7Q", activeServer: AppPreferences.productionURL))
        }
        preferences.set("https://private.example/api", forKey: "serverURL")
        precondition(AppPreferences.load(preferences).serverURL == "https://private.example/api")
        precondition(preferences.string(forKey: "serverURL") == "https://private.example/api")
        preferences.set(AppPreferences.legacyProductionURL, forKey: "serverURL")
        precondition(!AppPreferences.load(preferences).developerOptions, "Migrated production stays normal production")
        precondition(AppPreferences.builtInURL(for: AppPreferences.productionURL) != AppPreferences.builtInURL(for: AppPreferences.stagingURL))
        precondition(AppPreferences.builtInURL(for: AppPreferences.reviewURL) == AppPreferences.reviewURL)
        precondition(PairingLink.code(from: AppPreferences.reviewURL + "/connect/KY-4F7Q") == "KY-4F7Q")
        precondition(PairingLink.matchesServer(AppPreferences.reviewURL + "/connect/KY-4F7Q", activeServer: AppPreferences.reviewURL))
        precondition(!PairingLink.matchesServer(AppPreferences.reviewURL + "/connect/KY-4F7Q", activeServer: AppPreferences.productionURL))
        precondition(AppPreferences.builtInURL(for: AppPreferences.reviewURL + ".evil") == nil)
        for invalid in ["https://staging.keyala.io.evil", "http://staging.keyala.io", "https://user@staging.keyala.io", "https://staging.keyala.io:443", "https://staging.keyala.io/custom", "https://staging.keyala.io?x=1"] {
            precondition(AppPreferences.builtInURL(for: invalid) == nil)
        }
        preferences.set(true, forKey: "developerSampleData")
        precondition(AppPreferences.load(preferences).sampleData)
        precondition(PairingLink.code(from: "keyala://connect/KY-4F7Q") == "KY-4F7Q")
        precondition(PairingLink.code(from: " ky-4f7q ") == "KY-4F7Q")
        precondition(PairingLink.code(from: AppPreferences.productionURL + "/connect/KY-4F7Q") == "KY-4F7Q")
        precondition(PairingLink.code(from: AppPreferences.stagingURL + "/connect/KY-4F7Q") == "KY-4F7Q")
        precondition(PairingLink.serverURL(from: AppPreferences.stagingURL + "/connect/KY-4F7Q") == AppPreferences.stagingURL)
        for invalid in ["https://example.com/KY-4F7Q", "keyala://connect/KY-4F7Q?server=evil", "keyala://connect/KY-4F7Q#secret", "keyala://user@connect/KY-4F7Q", "keyala://connect:80/KY-4F7Q", "keyala://connect/%4BY-4F7Q", "keyala://connect/KY-4F7Q/", "keyala://requests/KY-4F7Q", "KY-123", "1234", "KY-12345", "sk-secret", AppPreferences.productionURL + "/connect/KY-4F7Q?token=secret", AppPreferences.productionURL + ".evil/connect/KY-4F7Q", AppPreferences.productionURL + "/connect/%4BY-4F7Q", AppPreferences.productionURL + "/enroll/KY-4F7Q"] {
            precondition(PairingLink.code(from: invalid) == nil, "Unsafe pairing input must be rejected")
        }
        precondition(Provider.openAI.keyPlaceholder == "sk-..." && Provider.anthropic.keyPlaceholder == "sk-ant-...")
        let enrollID = "en_abcdefghijklmnopqrstuv"
        precondition(EnrollmentLink.id(from: "keyala://enroll/" + enrollID) == enrollID)
        precondition(EnrollmentLink.id(from: AppPreferences.productionURL + "/enroll/" + enrollID) == enrollID)
        precondition(EnrollmentLink.id(from: AppPreferences.stagingURL + "/enroll/" + enrollID) == enrollID)
        for invalid in ["en_abcdefghijklmnopqrstuv", "keyala://enroll/en_short", "keyala://enroll/" + enrollID + "/", "keyala://enroll/" + enrollID + "?key=secret", "keyala://enroll/" + enrollID + "#secret", "keyala://user@enroll/" + enrollID, "keyala://enroll:80/" + enrollID, "keyala://connect/" + enrollID, AppPreferences.productionURL + "/enroll/%65n_abcdefghijklmnopqrstuv", AppPreferences.productionURL + ".evil/enroll/" + enrollID, "http://masterkey-prod-vkgm.onrender.com/enroll/" + enrollID] {
            precondition(EnrollmentLink.id(from: invalid) == nil, "Unsafe enrollment input must be rejected")
        }
        let decoder = JSONDecoder()
        let enrolled = try decoder.decode(KeyEnrollment.self, from: Data(#"{"id":"en_abcdefghijklmnopqrstuv","vendor":"openai","status":"submitted","last4":"A123","current_last4":"B456","expires_at":"2099-01-01T00:00:00.000Z"}"#.utf8))
        precondition(enrolled.canConfirm && !enrolled.canClaim && !enrolled.shouldPoll && enrolled.currentLast4 == "B456")
        let awaiting = try decoder.decode(KeyEnrollment.self, from: Data(#"{"id":"en_abcdefghijklmnopqrstuv","vendor":"anthropic","status":"claimed","last4":null,"expires_at":"2099-01-01T00:00:00Z"}"#.utf8))
        precondition(awaiting.shouldPoll && !awaiting.canConfirm && !awaiting.canClaim)
        let expiredEnrollment = try decoder.decode(KeyEnrollment.self, from: Data(#"{"id":"en_abcdefghijklmnopqrstuv","vendor":"openai","status":"submitted","last4":"A123","expires_at":"2000-01-01T00:00:00Z"}"#.utf8))
        precondition(expiredEnrollment.isExpired && !expiredEnrollment.canConfirm)
        let invalidEnrollment = try decoder.decode(KeyEnrollment.self, from: Data(#"{"id":"en_abcdefghijklmnopqrstuv","vendor":"openai","status":"submitted","last4":null,"expires_at":"2099-01-01T00:00:00Z"}"#.utf8))
        precondition(!invalidEnrollment.canConfirm)
        let login = try decoder.decode(AuthenticatedSession.self, from: Data(#"{"session_token":"keyala_sess_test","user":{"id":"usr_test","email":"preview@example.invalid"}}"#.utf8))
        let passkeyChallenge = Data(repeating: 42, count: 32).base64URL
        precondition(Data(base64URL: passkeyChallenge) == Data(repeating: 42, count: 32))
        for invalid in ["", "ab+_", "ab==", "a", "Zh"] { precondition(Data(base64URL: invalid) == nil) }
        let validPasskeyChallenge = try validatedPasskeyData(rpID: "app.keyala.io", challenge: passkeyChallenge, serverURL: "https://app.keyala.io")
        precondition(validPasskeyChallenge.count == 32)
        for (rp, challenge, server) in [
            ("keyala.io", passkeyChallenge, "https://app.keyala.io"),
            ("staging.keyala.io", passkeyChallenge, "https://app.keyala.io"),
            ("app.keyala.io", passkeyChallenge, "http://app.keyala.io"),
            ("app.keyala.io", "AA", "https://app.keyala.io"),
            ("app.keyala.io", passkeyChallenge, "https://user@app.keyala.io")
        ] {
            do { _ = try validatedPasskeyData(rpID: rp, challenge: challenge, serverURL: server); preconditionFailure("Untrusted RP or malformed challenge accepted") }
            catch APIError.invalidResponse { }
        }
        let credential = PasskeyCredential(id: "dGVzdA", response: .init(clientDataJSON: "e30", authenticatorData: "AA", signature: "AA", userHandle: "dXNlcg"))
        let credentialJSON = try JSONSerialization.jsonObject(with: JSONEncoder().encode(PasskeyProof(challengeID: "pkc_test", credential: credential))) as! [String: Any]
        precondition(credentialJSON["credential_id"] as? String == "dGVzdA" && credentialJSON["challenge_id"] as? String == "pkc_test")
        precondition(credentialJSON["user_handle"] as? String == "dXNlcg" && credentialJSON["attestation_object"] == nil && credentialJSON["credential"] == nil)
        let creationOptions = try decoder.decode(PasskeyCreationOptions.self, from: Data(#"{"challenge_id":"pkc_test","challenge":"dGVzdA","rp_id":"app.keyala.io","user_id":"dXNlcg","user_name":"test@example.invalid","exclude_credentials":["dGVzdA"],"expires_at":"2099-01-01T00:00:00Z"}"#.utf8))
        precondition(creationOptions.rpID == "app.keyala.io" && creationOptions.excludeCredentials == ["dGVzdA"])
        let assertionOptions = try decoder.decode(PasskeyRequestOptions.self, from: Data(#"{"challenge_id":"pkc_test","challenge":"dGVzdA","rp_id":"app.keyala.io","expires_at":"2099-01-01T00:00:00Z"}"#.utf8))
        precondition(assertionOptions.challengeID == "pkc_test")
        let attempt = GoogleSignInAttempt(verifier: "verifier", state: "expected-state", nonce: "expected-nonce", challenge: "challenge")
        let googleURL = URLComponents(url: attempt.authorizationURL(reauthenticate: true), resolvingAgainstBaseURL: false)!
        precondition(googleURL.host == "accounts.google.com" && googleURL.queryItems!.contains(URLQueryItem(name: "max_age", value: "0")))
        let goodCallback = GoogleSignInAttempt.redirectURI + "?state=expected-state&code=test-code"
        let googleCode = try attempt.code(from: URL(string: goodCallback)!)
        precondition(googleCode == "test-code")
        for callback in [goodCallback.replacingOccurrences(of: "expected-state", with: "wrong-state"), goodCallback + "&code=second", "keyala:/oauthredirect?state=expected-state&code=test-code", GoogleSignInAttempt.callbackScheme + "://evil/oauthredirect?state=expected-state&code=test-code"] {
            do { _ = try attempt.code(from: URL(string: callback)!); preconditionFailure("OAuth callback must match scheme, path, state and unique fields") } catch APIError.invalidResponse { }
        }
        func idToken(nonce: String) throws -> String {
            "e30." + (try JSONSerialization.data(withJSONObject: ["nonce": nonce, "aud": GoogleSignInAttempt.clientID])).base64URL + ".signature"
        }
        try attempt.validateNonce(idToken: idToken(nonce: "expected-nonce"))
        do { try attempt.validateNonce(idToken: idToken(nonce: "wrong-nonce")); preconditionFailure("ID token nonce must match OAuth attempt") } catch APIError.invalidResponse { }
        let passkeyDeletion = try JSONSerialization.jsonObject(with: JSONEncoder().encode(AccountDeletionRequest(passkey: PasskeyProof(challengeID: "pkc_test", credential: credential)))) as! [String: Any]
        precondition((passkeyDeletion["passkey"] as! [String: Any])["credential_id"] as? String == "dGVzdA")
        let googleAccount = try decoder.decode(AccountProfile.self, from: Data(#"{"id":"usr_google","email":"test@example.invalid","auth_provider":"google"}"#.utf8))
        precondition(googleAccount.authProvider == "google")
        let accountWithProvider = try decoder.decode(AccountProfile.self, from: Data(#"{"id":"usr_apple","email":"relay@example.invalid","auth_provider":"apple"}"#.utf8))
        precondition(accountWithProvider.authProvider == "apple")
        let deletion = AccountDeletionRequest(idToken: "test-fresh-identity", authorizationCode: "test-authorization-code")
        let deletionJSON = try JSONSerialization.jsonObject(with: JSONEncoder().encode(deletion)) as! [String: Any]
        precondition(deletionJSON["confirm"] as? String == "DELETE")
        precondition(deletionJSON["id_token"] as? String == "test-fresh-identity")
        precondition(deletionJSON["authorization_code"] as? String == "test-authorization-code")
        precondition(deletionJSON["password"] == nil && deletionJSON["email"] == nil)
        let stopped = try decoder.decode(StopAllResult.self, from: Data(#"{"requests_in_flight":1,"grants_revoked":2,"requests_denied":3}"#.utf8))
        precondition(stopped.requestsInFlight == 1)
        let deleted = try decoder.decode(AccountDeletionResult.self, from: Data(#"{"deleted":true,"apple_revoked":false}"#.utf8))
        precondition(deleted.deleted && deleted.appleRevoked == false)
        let restored = try decoder.decode(AuthenticatedSession.self, from: JSONEncoder().encode(login))
        precondition(restored.user.id == "usr_test" && restored.user.displayEmail == "preview@example.invalid")
        precondition(AccountProfile(id: "usr_apple", email: "placeholder@apple.invalid").displayEmail == nil)
        let routerKey = try decoder.decode(KeyRecord.self, from: Data(#"{"vendor":"openrouter","last4":"O123","created_at":"2026-11-18T00:00:00Z"}"#.utf8))
        precondition(routerKey.provider == .openRouter && routerKey.provider.title == "OpenRouter")
        precondition(Provider.openRouter.keyPlaceholder == "sk-or-v1-...")
        let callbackURL = URL(string: "https://staging.keyala.io/oauth/openrouter/callback")!
        let oauthState = String(repeating: "a", count: 22)
        let oauthFlowID = "of_" + String(repeating: "c", count: 22)
        var authURL = URLComponents(string: "https://openrouter.ai/auth")!
        authURL.queryItems = [URLQueryItem(name: "callback_url", value: callbackURL.absoluteString), URLQueryItem(name: "state", value: oauthState), URLQueryItem(name: "code_challenge", value: String(repeating: "b", count: 43)), URLQueryItem(name: "code_challenge_method", value: "S256")]
        func authorization(_ url: URL) throws -> OpenRouterAuthorization {
            try decoder.decode(OpenRouterAuthorization.self, from: JSONSerialization.data(withJSONObject: ["flow_id": oauthFlowID, "auth_url": url.absoluteString, "expires_at": "2099-01-01T00:00:00Z"]))
        }
        let validatedAuthorization = try authorization(authURL.url!).validatedURL(callbackURL: callbackURL, allowMock: false)
        precondition(validatedAuthorization == authURL.url!)
        var wrongCallback = authURL
        wrongCallback.queryItems = authURL.queryItems!.map { $0.name == "callback_url" ? URLQueryItem(name: "callback_url", value: "https://evil.example/oauth/openrouter/callback") : $0 }
        for url in [URL(string: "https://evil.example/auth?state=" + oauthState)!, wrongCallback.url!, URL(string: "http://openrouter.ai/auth?state=" + oauthState)!] {
            do { _ = try authorization(url).validatedURL(callbackURL: callbackURL, allowMock: false); preconditionFailure("Untrusted provider sign-in URLs must fail closed") } catch APIError.invalidResponse { }
        }
        let mockURL = URL(string: "http://127.0.0.1:8093/oauth/openrouter/mock?state=" + oauthState)!
        let mockCallback = URL(string: "http://127.0.0.1:8093/oauth/openrouter/callback")!
        let validatedMock = try authorization(mockURL).validatedURL(callbackURL: mockCallback, allowMock: true)
        precondition(validatedMock == mockURL)
        do { _ = try authorization(mockURL).validatedURL(callbackURL: mockCallback, allowMock: false); preconditionFailure("Mock consent requires explicit developer options") } catch APIError.invalidResponse { }
        try OpenRouterCallback.validate(URL(string: "keyala://oauth/openrouter?status=ok&flow=" + oauthFlowID)!, expectedFlowID: oauthFlowID)
        try OpenRouterCallback.validate(URL(string: "keyala://oauth/openrouter?status=error&reason=cancelled")!, expectedFlowID: oauthFlowID)
        for callback in ["keyala://oauth/openrouter?status=ok&status=error", "keyala://oauth/openrouter?status=ok&flow=wrong", "keyala://oauth/openrouter?status=ok", "keyala://oauth/openrouter?status=ok&api_key=secret", "keyala://evil/openrouter?status=ok", "https://oauth/openrouter?status=ok"] {
            do { try OpenRouterCallback.validate(URL(string: callback)!, expectedFlowID: oauthFlowID); preconditionFailure("Untrusted provider callbacks must be rejected") } catch APIError.invalidResponse { }
        }
        let savedFlow = try decoder.decode(OpenRouterFlow.self, from: JSONSerialization.data(withJSONObject: ["flow_id": oauthFlowID, "provider": "openrouter", "status": "saved", "last4": "O123"]))
        precondition(savedFlow.confirmedKey(in: [routerKey], expectedFlowID: oauthFlowID)?.lastFour == "O123")
        precondition(savedFlow.confirmedKey(in: [routerKey], expectedFlowID: "of_other") == nil)
        precondition(savedFlow.confirmedKey(in: [], expectedFlowID: oauthFlowID) == nil)
        let pendingFlow = try decoder.decode(OpenRouterFlow.self, from: JSONSerialization.data(withJSONObject: ["flow_id": oauthFlowID, "provider": "openrouter", "status": "pending", "last4": "O123"]))
        precondition(pendingFlow.confirmedKey(in: [routerKey], expectedFlowID: oauthFlowID) == nil, "An existing key cannot confirm a pending authorization")
        var shortcut = makeRequest(id: "req_shortcut", status: "pending", createdAt: "2026-11-18T00:00:00Z", payload: .object(["messages": .array([.object(["role": .string("user"), "content": .string("繁體中文と日本語")])])]))
        shortcut.estimatedCostMicrousd = 50_000
        precondition(shortcut.canApproveWithSwipe && shortcut.readableMessages.first?.text == "繁體中文と日本語")
        shortcut.estimatedCostMicrousd = 50_001
        precondition(!shortcut.canApproveWithSwipe)
        shortcut.estimatedCostMicrousd = -1
        precondition(!shortcut.canApproveWithSwipe)
        shortcut.estimatedCostMicrousd = 28; shortcut.payload = nil
        precondition(!shortcut.canApproveWithSwipe)
        let browserPreview = try decoder.decode(ConnectPreview.self, from: Data(#"{"app":{"name":"Resume Tailor"},"vendor":"openai","origin":"https://example.invalid:8443","requires_ask":true}"#.utf8))
        precondition(browserPreview.sourceHost == "example.invalid:8443" && browserPreview.requiresAsk == true)
        let serverPreview = try decoder.decode(ConnectPreview.self, from: Data(#"{"app":{"name":"Example app"},"vendor":"anthropic","origin":null,"requires_ask":false}"#.utf8))
        precondition(serverPreview.sourceHost == nil && serverPreview.requiresAsk == false)
        let summary = #"{"id":"req-1","grant_id":"grant-1","app":{"id":"app-1","name":"Sketchpad"},"vendor":"openai","model":"gpt-4.1-mini","endpoint":"/v1/openai/chat/completions","status":"pending","estimated_usd":0.000028,"actual_usd":null,"created_at":"2026-11-14T08:22:00Z"}"#.data(using: .utf8)!
        let request = try decoder.decode(RequestRecord.self, from: summary)
        precondition(request.status == "pending" && request.payload == nil)
        precondition(request.estimatedCostMicrousd == 28)

        let detail = #"{"id":"req-1","grant_id":"grant-1","app":{"id":"app-1","name":"Sketchpad"},"vendor":"openai","model":"gpt-4.1-mini","status":"uncertain","estimated_usd":0.000028,"actual_usd":null,"created_at":"2026-11-14T08:22:00Z","payload":{"messages":[{"role":"user","content":"inspect exactly"}]}}"#.data(using: .utf8)!
        let full = try decoder.decode(RequestRecord.self, from: detail)
        precondition(full.status == "uncertain" && full.payload?.prettyPrinted.contains("inspect exactly") == true)
        let precisePayload = try decoder.decode(JSONValue.self, from: Data(#"{"seed":9007199254740993,"prompt":"繁體中文と日本語"}"#.utf8))
        precondition(precisePayload.prettyPrinted.contains("9007199254740993"))
        precondition(precisePayload.prettyPrinted.contains("繁體中文と日本語"))
        let oversizedCost = Data(String(decoding: summary, as: UTF8.self).replacingOccurrences(of: "0.000028", with: "9223372036854.7758").utf8)
        do {
            _ = try decoder.decode(RequestRecord.self, from: oversizedCost)
            preconditionFailure("Out-of-range costs must fail decoding without trapping.")
        } catch is DecodingError { }

        let key = try decoder.decode(KeyRecord.self, from: #"{"vendor":"anthropic","last4":"A123","created_at":"2026-11-14T00:00:00Z"}"#.data(using: .utf8)!)
        precondition(key.provider == .anthropic && key.lastFour == "A123")


        let uncheckedKey = try decoder.decode(KeyRecord.self, from: Data(#"{"vendor":"openai","last4":"ABCD","checked":false}"#.utf8))
        precondition(uncheckedKey.checked == false)

        let grant = try decoder.decode(GrantRecord.self, from: #"{"id":"grant-1","app":{"id":"app-1","name":"Sketchpad"},"vendor":"openai","policy":{"mode":"count","remaining_count":7,"expires_at":"2026-12-01T00:00:00Z"},"created_at":"2026-11-14T00:00:00Z","spent_usd":0.42,"request_count":3}"#.data(using: .utf8)!)
        precondition(grant.maxRequests == 7 && grant.expiresAt == "2026-12-01T00:00:00Z")

        let detailPayload = JSONValue.object(["prompt": .string("immutable payload")])
        let detailResponse = JSONValue.object(["answer": .string("saved response")])
        let existingPage = [
            makeRequest(id: "req_latest", status: "sending", createdAt: "2026-11-14T08:23:00Z"),
            makeRequest(id: "req_detail", status: "pending", createdAt: "2026-11-14T08:22:00.500Z", payload: detailPayload, response: detailResponse),
            makeRequest(id: "req_older", status: "completed", createdAt: "2026-11-14T08:21:00Z")
        ]
        let refreshedPage = [
            makeRequest(id: "req_latest", status: "completed", createdAt: "2026-11-14T08:23:00Z"),
            makeRequest(id: "req_detail", status: "uncertain", createdAt: "2026-11-14T08:22:00.500Z"),
            makeRequest(id: "req_tie_a", status: "denied", createdAt: "2026-11-14T08:22:00.500Z"),
            makeRequest(id: "req_tie_a", status: "expired", createdAt: "2026-11-14T08:22:00.500Z")
        ]
        let merged = RequestHistory.merging(existing: existingPage, incoming: refreshedPage)
        precondition(merged.map(\.id) == ["req_latest", "req_tie_a", "req_detail", "req_older"])
        precondition(merged[0].status == "completed" && merged[1].status == "expired")
        precondition(merged[2].status == "uncertain" && merged[2].payload == detailPayload && merged[2].response == detailResponse)

        var cursor = RequestHistoryCursor()
        let oldest = makeRequest(id: "req_old", status: "completed", createdAt: "2026-11-14T08:00:00.000Z")
        let fresh = [makeRequest(id: "req_new_a", status: "completed", createdAt: "2026-11-14T10:00:00.000Z"), makeRequest(id: "req_new_b", status: "completed", createdAt: "2026-11-14T09:00:00.000Z")]
        cursor.refresh(page: [oldest], existingIDs: [], pageSize: 2)
        precondition(!cursor.hasMore)
        cursor.refresh(page: fresh, existingIDs: [oldest.id], pageSize: 2)
        precondition(cursor.hasMore && cursor.beforeID == "req_new_b", "A new head must page into the gap, not skip to retained old records.")
        cursor.advance(page: [oldest], from: "req_new_b", pageSize: 2)
        precondition(!cursor.hasMore && cursor.beforeID == oldest.id)
        cursor.refresh(page: fresh, existingIDs: Set(fresh.map(\.id) + [oldest.id]), pageSize: 2)
        precondition(!cursor.hasMore, "An overlapping refresh must preserve the completed history state.")

        let client = APIClient(baseURL: "https://keyala.example", allowLocalHTTP: false, token: "session")
        let pagePath = APIClient.requestListPath(limit: 100, beforeID: "req_oldest")
        let pageURL = try client.resolvedURL(for: pagePath)
        let pageQuery = Dictionary(uniqueKeysWithValues: (URLComponents(url: pageURL, resolvingAgainstBaseURL: false)?.queryItems ?? []).map { ($0.name, $0.value ?? "") })
        precondition(pageURL.path == "/v1/me/requests" && pageQuery["limit"] == "100" && pageQuery["before_id"] == "req_oldest")

        let url = try client.resolvedURL(for: "/v1/me/requests?limit=100")
        precondition(url.path == "/v1/me/requests" && URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?.first?.value == "100")
        do {
            _ = try APIClient(baseURL: "http://remote.example", allowLocalHTTP: true, token: nil).resolvedURL(for: "/v1/me/keys")
            preconditionFailure("Remote HTTP must be rejected even when local HTTP is enabled.")
        } catch APIError.insecureServer { }
        let localURL = try APIClient(baseURL: "http://127.0.0.1:8080", allowLocalHTTP: true, token: nil).resolvedURL(for: "/v1/me/keys")
        precondition(localURL.path == "/v1/me/keys")
        print("Keyala contract checks passed")
    }

    static func makeRequest(id: String, status: String, createdAt: String, payload: JSONValue? = nil, response: JSONValue? = nil) -> RequestRecord {
        RequestRecord(id: id, appID: "app-1", appName: "Sketchpad", provider: .openAI, model: "gpt-4.1-mini", status: status, createdAt: createdAt, expiresAt: nil, payload: payload, estimatedCostMicrousd: 28, actualCostMicrousd: nil, response: response, error: nil)
    }
}
