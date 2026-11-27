import Foundation
import Security

private final class RedirectBlocker: NSObject, URLSessionTaskDelegate {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        completionHandler(nil)
    }
}

enum APIError: LocalizedError {
    case reauthenticationFailed, invalidURL, insecureServer, unauthorized, invalidCredentials, passwordLoginUnavailable, http(Int, String), invalidResponse, missingSession, coupon(String)
    var errorDescription: String? {
        switch self {
        case .reauthenticationFailed: localized("Identity verification failed. Verify the same account again; your account was not deleted.")
        case .invalidURL: "Enter a valid server URL."
        case .insecureServer: "Live connections require HTTPS. Enable the separate localhost development option to use local HTTP."
        case .unauthorized: "Your session expired. Sign in again."
        case .invalidCredentials: "Check your username and password and try again."
        case .passwordLoginUnavailable: "Password sign-in is unavailable on this server."
        case .http(let code, let message): "Server error (\(code)): \(message)"
        case .invalidResponse: "The server returned data this app could not read."
        case .missingSession: "Sign in to connect to your Keyala server."
        case .coupon(let message): localized(message)
        }
    }

    static func couponMessage(for type: String?) -> String? {
        switch type {
        case "coupon_expired": "This coupon has expired."
        case "coupon_revoked": "The sponsor revoked this coupon."
        case "coupon_exhausted": "This coupon has insufficient credit."
        case "coupon_claim_limit": "This account has already claimed this coupon."
        case "coupon_model_not_allowed": "This model is outside the coupon’s allowance."
        case "coupon_policy_required": "Coupon calls must be approved one at a time."
        case "coupon_not_found": "This coupon could not be found. Check the code and server."
        case "coupon_unusable", "coupon_unavailable", "coupon_mismatch": "This coupon is unavailable for this connection."
        default: nil
        }
    }
}

final class SessionVault {
    private let service = "app.keyala.session"
    func read(origin: String) -> AuthenticatedSession? {
        var query: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecReturnData as String: true, kSecMatchLimit as String: kSecMatchLimitOne]
        query[kSecAttrAccount as String] = origin
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data else { return nil }
        return try? JSONDecoder().decode(AuthenticatedSession.self, from: data)
    }
    func write(_ session: AuthenticatedSession?, origin: String) throws {
        let data = try session.map { try JSONEncoder().encode($0) }
        let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrAccount as String: origin]
        SecItemDelete(query as CFDictionary)
        guard let data else { return }
        var item = query; item[kSecValueData as String] = data; item[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        let status = SecItemAdd(item as CFDictionary, nil)
        guard status == errSecSuccess else { throw NSError(domain: NSOSStatusErrorDomain, code: Int(status)) }
    }
    func clear() { SecItemDelete([kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service] as CFDictionary) }
}

final class APIClient {
    var baseURL: String
    var allowLocalHTTP: Bool
    var token: String?
    private let encoder = JSONEncoder()
    private let decoder = JSONDecoder()
    private static let session = URLSession(configuration: .ephemeral, delegate: RedirectBlocker(), delegateQueue: nil)
    init(baseURL: String, allowLocalHTTP: Bool, token: String?) { self.baseURL = baseURL; self.allowLocalHTTP = allowLocalHTTP; self.token = token }

    private func url(_ path: String) throws -> URL {
        guard let base = URL(string: baseURL.trimmingCharacters(in: .whitespacesAndNewlines)),
              let scheme = base.scheme?.lowercased(), ["https", "http"].contains(scheme),
              let host = base.host, base.user == nil, base.password == nil,
              base.query == nil, base.fragment == nil else { throw APIError.invalidURL }
        let local = ["localhost", "127.0.0.1", "::1"].contains(host.lowercased())
        guard scheme == "https" || (allowLocalHTTP && local) else { throw APIError.insecureServer }
        let relative = path.split(separator: "?", maxSplits: 1, omittingEmptySubsequences: false)
        let route = String(relative[0]).trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        var components = URLComponents(url: base, resolvingAgainstBaseURL: false)
        let prefix = base.path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        components?.path = "/" + [prefix, route].filter { !$0.isEmpty }.joined(separator: "/")
        if relative.count > 1 {
            let query = String(relative[1])
            let queryParts = URLComponents(string: "https://query.invalid/?\(query)")
            components?.queryItems = queryParts?.queryItems
        }
        guard let result = components?.url else { throw APIError.invalidURL }
        return result
    }

    func resolvedURL(for path: String) throws -> URL { try url(path) }

    private func send<T: Decodable>(_ path: String, method: String = "GET", body: Encodable? = nil, authenticated: Bool = true, response: T.Type = T.self) async throws -> T {
        guard !authenticated || token != nil else { throw APIError.missingSession }
        var request = URLRequest(url: try url(path)); request.httpMethod = method
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        if let token { request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization") }
        if let body { request.setValue("application/json", forHTTPHeaderField: "Content-Type"); request.httpBody = try encoder.encode(AnyEncodable(body)) }
        let (data, result) = try await Self.session.data(for: request)
        guard let http = result as? HTTPURLResponse else { throw APIError.invalidResponse }
        guard (200..<300).contains(http.statusCode) else {
            let error = (try? JSONSerialization.jsonObject(with: data) as? [String: Any])?["error"] as? [String: Any]
            if http.statusCode == 401 {
                if error?["type"] as? String == "reauth_failed" { throw APIError.reauthenticationFailed }
                throw APIError.unauthorized
            }
            if let message = APIError.couponMessage(for: error?["type"] as? String) { throw APIError.coupon(message) }
            let message: String
            switch error?["type"] as? String {
            case "route_changed": message = localized("The billing route changed. Review the refreshed connection before approving again.")
            case "request_in_flight": message = localized("A request is being sent. Wait for it to finish, then try account deletion again.")
            case "upstream_paused": message = localized("The operator has paused provider requests. Check support for the service status.")
            default: message = error?["message"] as? String ?? "Request failed."
            }
            throw APIError.http(http.statusCode, message)
        }
        if T.self == EmptyResponse.self { return EmptyResponse() as! T }
        do { return try decoder.decode(T.self, from: data) } catch { throw APIError.invalidResponse }
    }

    func devLogin(email: String) async throws -> AuthenticatedSession {
        struct LoginBody: Encodable { let email: String }
        return try await send("/v1/auth/dev", method: "POST", body: LoginBody(email: email), authenticated: false)
    }
    func appleLogin(idToken: String) async throws -> AuthenticatedSession {
        struct LoginBody: Encodable { let idToken: String; enum CodingKeys: String, CodingKey { case idToken = "id_token" } }
        return try await send("/v1/auth/apple", method: "POST", body: LoginBody(idToken: idToken), authenticated: false)
    }
    func authenticationMethods() async throws -> AuthenticationMethods {
        try await send("/v1/auth/methods", authenticated: false)
    }
    static func exchangeGoogleCode(_ code: String, attempt: GoogleSignInAttempt) async throws -> String {
        var form = URLComponents()
        form.queryItems = [URLQueryItem(name: "client_id", value: GoogleSignInAttempt.clientID), URLQueryItem(name: "redirect_uri", value: GoogleSignInAttempt.redirectURI), URLQueryItem(name: "grant_type", value: "authorization_code"), URLQueryItem(name: "code", value: code), URLQueryItem(name: "code_verifier", value: attempt.verifier)]
        var request = URLRequest(url: URL(string: "https://oauth2.googleapis.com/token")!)
        request.httpMethod = "POST"
        request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        request.httpBody = form.percentEncodedQuery?.replacingOccurrences(of: "+", with: "%2B").data(using: .utf8)
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse, http.statusCode == 200,
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let token = object["id_token"] as? String, !token.isEmpty else {
            throw APIError.http(0, localized("Google sign-in could not finish. Please try again."))
        }
        return token
    }
    func googleLogin(idToken: String) async throws -> AuthenticatedSession {
        struct Body: Encodable { let idToken: String; enum CodingKeys: String, CodingKey { case idToken = "id_token" } }
        return try await send("/v1/auth/google", method: "POST", body: Body(idToken: idToken), authenticated: false)
    }
    func passkeyRegistrationOptions(proof: FreshIdentity) async throws -> PasskeyCreationOptions {
        try await send("/v1/me/passkeys/register/options", method: "POST", body: proof)
    }
    func registerPasskey(_ proof: PasskeyProof) async throws -> PasskeyRecord {
        try await send("/v1/me/passkeys/register", method: "POST", body: proof)
    }
    func passkeyOptions() async throws -> PasskeyRequestOptions {
        try await send("/v1/auth/passkey/options", method: "POST", authenticated: false)
    }
    func passkeyLogin(_ proof: PasskeyProof) async throws -> AuthenticatedSession {
        try await send("/v1/auth/passkey", method: "POST", body: proof, authenticated: false)
    }
    func passkeys() async throws -> [PasskeyRecord] { try await send("/v1/me/passkeys") }
    func removePasskey(id: String) async throws {
        guard !id.isEmpty, id.utf8.allSatisfy({ (65...90).contains($0) || (97...122).contains($0) || (48...57).contains($0) || $0 == 45 || $0 == 95 }) else { throw APIError.invalidResponse }
        let _: EmptyResponse = try await send("/v1/me/passkeys/\(id)", method: "DELETE")
    }
    func passwordLogin(username: String, password: String) async throws -> AuthenticatedSession {
        struct LoginBody: Encodable {
            let username: String
            let password: String
        }
        do {
            return try await send("/v1/auth/password", method: "POST", body: LoginBody(username: username, password: password), authenticated: false)
        } catch APIError.unauthorized {
            throw APIError.invalidCredentials
        } catch APIError.http(404, _) {
            throw APIError.passwordLoginUnavailable
        }
    }
    func webLoginPreview(id: String) async throws -> WebLoginPreview {
        try await send("/v1/me/web-login/\(id)")
    }
    func decideWebLogin(id: String, approve: Bool) async throws {
        let _: EmptyResponse = try await send("/v1/me/web-login/\(id)/\(approve ? "approve" : "deny")", method: "POST")
    }
    func setWebPassword(_ password: String?) async throws {
        struct Body: Encodable { let password: String }
        let _: EmptyResponse = try await send("/v1/me/password", method: password == nil ? "DELETE" : "PUT", body: password.map { Body(password: $0) })
    }
    func requests(status: String? = nil, limit: Int = 100, beforeID: String? = nil) async throws -> [RequestRecord] {
        return try await send(Self.requestListPath(status: status, limit: limit, beforeID: beforeID), response: [RequestRecord].self)
    }
    static func requestListPath(status: String? = nil, limit: Int = 100, beforeID: String? = nil) -> String {
        var query = [URLQueryItem(name: "limit", value: String(limit))]
        if let status { query.append(URLQueryItem(name: "status", value: status)) }
        if let beforeID { query.append(URLQueryItem(name: "before_id", value: beforeID)) }
        var components = URLComponents()
        components.queryItems = query
        return "/v1/me/requests?\(components.percentEncodedQuery ?? "limit=100")"
    }
    func request(id: String) async throws -> RequestRecord { try await send("/v1/me/requests/\(id)") }
    func decide(id: String, approve: Bool, policy: Policy? = nil) async throws -> RequestRecord {
        struct Body: Encodable { let policy: Policy? }
        let body: Encodable? = approve ? Body(policy: policy?.mode == "ask" ? nil : policy) : nil
        return try await send("/v1/me/requests/\(id)/\(approve ? "approve" : "deny")", method: "POST", body: body)
    }
    func routingPreferences() async throws -> RoutingPreferences { try await send("/v1/me/preferences") }
    func updateRoutingPreferences(_ enabled: Bool) async throws -> RoutingPreferences {
        try await send("/v1/me/preferences", method: "PATCH", body: RoutingPreferences(openRouterForDirectAPIs: enabled))
    }
    func keys() async throws -> [KeyRecord] { try await send("/v1/me/keys") }
    func coupons() async throws -> [Coupon] { try await send("/v1/me/coupons") }
    func couponPreview(code: String) async throws -> CouponPreview {
        guard CouponLink.isCode(code) else { throw APIError.invalidURL }
        return try await send("/v1/me/coupons/\(code)")
    }
    func claimCoupon(code: String) async throws -> Coupon {
        guard CouponLink.isCode(code) else { throw APIError.invalidURL }
        return try await send("/v1/me/coupons/\(code)/claim", method: "POST")
    }
    func startOpenRouter(limitUSD: Double) async throws -> OpenRouterAuthorization {
        struct Body: Encodable { let limitUSD: Double; enum CodingKeys: String, CodingKey { case limitUSD = "limit_usd" } }
        return try await send("/v1/me/oauth/openrouter", method: "POST", body: Body(limitUSD: limitUSD))
    }
    func openRouterFlow(id: String, cancel: Bool = false) async throws -> OpenRouterFlow {
        try await send("/v1/me/oauth/openrouter/flows/\(id)" + (cancel ? "/cancel" : ""), method: cancel ? "POST" : "GET")
    }

    func addKey(provider: Provider, value: String, label: String) async throws -> KeyRecord {
        struct Body: Encodable { let apiKey: String; enum CodingKeys: String, CodingKey { case apiKey = "api_key" } }
        return try await send("/v1/me/keys/\(provider.rawValue)", method: "PUT", body: Body(apiKey: value))
    }
    func deleteKey(provider: Provider) async throws { let _: EmptyResponse = try await send("/v1/me/keys/\(provider.rawValue)", method: "DELETE") }
    func logout() async throws { let _: EmptyResponse = try await send("/v1/me/logout", method: "POST") }
    func revokeOtherSessions() async throws {
        struct Body: Encodable { let keep_current = true }
        let _: EmptyResponse = try await send("/v1/me/sessions/revoke-all", method: "POST", body: Body())
    }
    func stopAll() async throws -> StopAllResult { try await send("/v1/me/stop-all", method: "POST") }
    func deleteAccount(_ body: AccountDeletionRequest) async throws -> AccountDeletionResult {
        try await send("/v1/me/delete", method: "POST", body: body)
    }
    func grants() async throws -> [GrantRecord] { try await send("/v1/me/grants") }
    func revoke(id: String) async throws { let _: EmptyResponse = try await send("/v1/me/grants/\(id)", method: "DELETE") }
    func updatePolicy(id: String, policy: Policy) async throws -> GrantRecord {
        struct Body: Encodable { let policy: Policy }
        return try await send("/v1/me/grants/\(id)", method: "PATCH", body: Body(policy: policy))
    }
    func connectPreview(code: String) async throws -> ConnectPreview { try await send("/v1/me/connect/\(code)") }
    func connectDecision(code: String, approve: Bool, policy: Policy? = nil, billingVendor: Provider? = nil) async throws {
        struct Body: Encodable { let policy: Policy?; let billing_vendor: Provider? }
        let _: EmptyResponse = try await send("/v1/me/connect/\(code)/\(approve ? "approve" : "deny")", method: "POST", body: approve ? Body(policy: policy, billing_vendor: billingVendor) : nil)
    }
    func enrollment(id: String) async throws -> KeyEnrollment { try await send("/v1/me/enroll/\(id)") }
    func claimEnrollment(id: String) async throws -> KeyEnrollment { try await send("/v1/me/enroll/\(id)/claim", method: "POST") }
    func confirmEnrollment(id: String) async throws -> KeyRecord {
        try await send("/v1/me/enroll/\(id)/confirm", method: "POST")
    }
    func cancelEnrollment(id: String) async throws {
        let _: EmptyResponse = try await send("/v1/me/enroll/\(id)/cancel", method: "POST")
    }
    func registerDevice(token: String) async throws {
        struct Body: Encodable { let apnsToken: String; enum CodingKeys: String, CodingKey { case apnsToken = "apns_token" } }
        let _: EmptyResponse = try await send("/v1/me/devices", method: "POST", body: Body(apnsToken: token))
    }
}

private struct AnyEncodable: Encodable {
    private let encodeValue: (Encoder) throws -> Void
    init(_ value: Encodable) { encodeValue = value.encode }
    func encode(to encoder: Encoder) throws { try encodeValue(encoder) }
}
struct EmptyResponse: Decodable { init() {} }
struct ConnectPreview: Decodable {
    struct App: Decodable { let name: String; let homepage: String? }
    let app: App; let vendor: Provider; let suggestedPolicy: PolicyPreview?
    var origin: String?
    var requiresAsk: Bool?
    var billingVendor: Provider? = nil
    var coupon: CouponPreview? = nil
    var keyProvider: Provider { billingVendor ?? vendor }

    var sourceHost: String? {
        guard let origin, let url = URLComponents(string: origin), let host = url.host else { return nil }
        return url.port.map { "\(host):\($0)" } ?? host
    }

    enum CodingKeys: String, CodingKey {
        case app, vendor, origin, coupon
        case billingVendor = "billing_vendor", suggestedPolicy = "suggested_policy", requiresAsk = "requires_ask"
    }
}
struct PolicyPreview: Decodable { let mode: String?; let remainingCount: Int?; enum CodingKeys: String, CodingKey { case mode; case remainingCount = "remaining_count" } }
