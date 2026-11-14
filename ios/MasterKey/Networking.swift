import Foundation
import Security

private final class RedirectBlocker: NSObject, URLSessionTaskDelegate {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        completionHandler(nil)
    }
}

enum APIError: LocalizedError {
    case invalidURL, insecureServer, unauthorized, http(Int, String), invalidResponse, missingSession
    var errorDescription: String? {
        switch self {
        case .invalidURL: "Enter a valid server URL."
        case .insecureServer: "Live connections require HTTPS. Enable the separate localhost development option to use local HTTP."
        case .unauthorized: "Your session expired. Sign in again."
        case .http(let code, let message): "Server error (\(code)): \(message)"
        case .invalidResponse: "The server returned data this app could not read."
        case .missingSession: "Sign in to connect to your MasterKey server."
        }
    }
}

final class SessionVault {
    private let service = "app.masterkey.session"
    func read(origin: String) -> String? {
        var query: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecReturnData as String: true, kSecMatchLimit as String: kSecMatchLimitOne]
        query[kSecAttrAccount as String] = origin
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }
    func write(_ token: String?, origin: String) throws {
        let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrAccount as String: origin]
        SecItemDelete(query as CFDictionary)
        guard let token else { return }
        var item = query; item[kSecValueData as String] = Data(token.utf8); item[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
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
            if http.statusCode == 401 { throw APIError.unauthorized }
            let message = (try? JSONSerialization.jsonObject(with: data) as? [String: Any])
                .flatMap { ($0["error"] as? [String: Any])?["message"] as? String } ?? "Request failed."
            throw APIError.http(http.statusCode, message)
        }
        if T.self == EmptyResponse.self { return EmptyResponse() as! T }
        do { return try decoder.decode(T.self, from: data) } catch { throw APIError.invalidResponse }
    }

    func devLogin(email: String) async throws -> String {
        struct LoginBody: Encodable { let email: String }
        return try await send("/v1/auth/dev", method: "POST", body: LoginBody(email: email), authenticated: false, response: DemoAuthResponse.self).sessionToken
    }
    func appleLogin(idToken: String) async throws -> String {
        struct LoginBody: Encodable { let idToken: String; enum CodingKeys: String, CodingKey { case idToken = "id_token" } }
        return try await send("/v1/auth/apple", method: "POST", body: LoginBody(idToken: idToken), authenticated: false, response: DemoAuthResponse.self).sessionToken
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
    func keys() async throws -> [KeyRecord] { try await send("/v1/me/keys") }
    func addKey(provider: Provider, value: String, label: String) async throws -> KeyRecord {
        struct Body: Encodable { let apiKey: String; enum CodingKeys: String, CodingKey { case apiKey = "api_key" } }
        return try await send("/v1/me/keys/\(provider.rawValue)", method: "PUT", body: Body(apiKey: value))
    }
    func deleteKey(provider: Provider) async throws { let _: EmptyResponse = try await send("/v1/me/keys/\(provider.rawValue)", method: "DELETE") }
    func grants() async throws -> [GrantRecord] { try await send("/v1/me/grants") }
    func revoke(id: String) async throws { let _: EmptyResponse = try await send("/v1/me/grants/\(id)", method: "DELETE") }
    func updatePolicy(id: String, policy: Policy) async throws -> GrantRecord {
        struct Body: Encodable { let policy: Policy }
        return try await send("/v1/me/grants/\(id)", method: "PATCH", body: Body(policy: policy))
    }
    func connectPreview(code: String) async throws -> ConnectPreview { try await send("/v1/me/connect/\(code)") }
    func connectDecision(code: String, approve: Bool, policy: Policy? = nil) async throws {
        struct Body: Encodable { let policy: Policy? }
        let _: EmptyResponse = try await send("/v1/me/connect/\(code)/\(approve ? "approve" : "deny")", method: "POST", body: approve ? Body(policy: policy) : nil)
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
    enum CodingKeys: String, CodingKey { case app, vendor; case suggestedPolicy = "suggested_policy" }
}
struct PolicyPreview: Decodable { let mode: String?; let remainingCount: Int?; enum CodingKeys: String, CodingKey { case mode; case remainingCount = "remaining_count" } }
