import Foundation

// Run only against a disposable local server with dev login and mock providers enabled.
@main enum WebAccessIntegrationChecks {
    static func main() async throws {
        let base = ProcessInfo.processInfo.environment["KEYALA_TEST_URL"] ?? "http://127.0.0.1:8097"
        guard let url = URL(string: base), url.scheme == "http", ["127.0.0.1", "localhost"].contains(url.host ?? "") else {
            fatalError("This test must use a disposable localhost server")
        }
        let app = APIClient(baseURL: base, allowLocalHTTP: true, token: nil)
        let account = try await app.devLogin(email: "web-check-\(UUID().uuidString.lowercased())@example.test")
        app.token = account.sessionToken
        let (id, secret) = try await start(base)
        let preview = try await app.webLoginPreview(id: id)
        precondition(preview.canApprove && preview.browser?.contains("KeyalaContractCheck") == true)
        try await app.decideWebLogin(id: id, approve: true)
        let approved = try await raw(base, path: "/v1/web/login/\(id)", bearer: secret)
        precondition(approved["status"] as? String == "approved")
        let webToken = approved["session_token"] as! String
        let used = try await raw(base, path: "/v1/web/login/\(id)", bearer: secret)
        precondition(used["session_token"] == nil, "A web session is returned once")
        let web = APIClient(baseURL: base, allowLocalHTTP: true, token: webToken)
        let keys = try await web.keys()
        precondition(keys.isEmpty)
        do { try await web.setWebPassword("not-a-real-password-123"); fatalError("Web sessions cannot change password") }
        catch APIError.http(let status, _) { precondition(status == 403) }
        let (deniedID, deniedSecret) = try await start(base)
        do { try await web.decideWebLogin(id: deniedID, approve: true); fatalError("Web sessions cannot approve a new browser") }
        catch APIError.http(let status, _) { precondition(status == 403) }
        try await app.decideWebLogin(id: deniedID, approve: false)
        let denied = try await raw(base, path: "/v1/web/login/\(deniedID)", bearer: deniedSecret)
        precondition(denied["status"] as? String == "denied" && denied["session_token"] == nil)
        let password = "test-only-" + UUID().uuidString
        try await app.setWebPassword(password)
        let passwordSession = try await app.passwordLogin(username: account.user.email, password: password)
        let passwordWeb = APIClient(baseURL: base, allowLocalHTTP: true, token: passwordSession.sessionToken)
        do { try await passwordWeb.setWebPassword(password); fatalError("Password sign-in must also be web scoped") }
        catch APIError.http(let status, _) { precondition(status == 403) }
        try await app.setWebPassword(nil)
        do { _ = try await app.passwordLogin(username: account.user.email, password: password); fatalError("Removed password must not work") }
        catch APIError.invalidCredentials {}
        print("Native web access integration checks passed: one-time sign-in, deny, scoped sessions, password save/remove")
    }
    static func start(_ base: String) async throws -> (String, String) {
        let response = try await raw(base, path: "/v1/web/login", method: "POST")
        return (response["login_id"] as! String, response["login_secret"] as! String)
    }
    static func raw(_ base: String, path: String, method: String = "GET", bearer: String? = nil) async throws -> [String: Any] {
        var request = URLRequest(url: URL(string: base + path)!)
        request.httpMethod = method
        request.setValue("KeyalaContractCheck", forHTTPHeaderField: "User-Agent")
        if let bearer { request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization") }
        let (data, response) = try await URLSession.shared.data(for: request)
        precondition((200..<300).contains((response as! HTTPURLResponse).statusCode))
        return try JSONSerialization.jsonObject(with: data) as! [String: Any]
    }
}
