import Foundation
@main struct AccountChecks {
    static func main() async throws {
        let baseURL = CommandLine.arguments.dropFirst().first ?? "http://127.0.0.1:8094"
        guard let url = URL(string: baseURL), url.scheme == "http", ["127.0.0.1", "localhost"].contains(url.host ?? "") else {
            fatalError("Run only against an isolated localhost mock with development login enabled")
        }
        let email = "native-account-qa-" + UUID().uuidString.lowercased() + "@example.com"
        let api = APIClient(baseURL: baseURL, allowLocalHTTP: true, token: nil)
        let first = try await api.devLogin(email: email)
        precondition(first.user.authProvider == "dev")
        api.token = first.sessionToken
        let second = try await api.devLogin(email: email)
        let other = APIClient(baseURL: api.baseURL, allowLocalHTTP: true, token: second.sessionToken)
        try await api.revokeOtherSessions()
        do { _ = try await other.keys(); preconditionFailure("Other session must be revoked") } catch APIError.unauthorized { }
        let stop = try await api.stopAll()
        precondition(stop.requestsInFlight == 0)
        try await api.logout()
        do { _ = try await api.keys(); preconditionFailure("Logout must end the server session") } catch APIError.unauthorized { }
        let fresh = try await api.devLogin(email: email)
        api.token = fresh.sessionToken
        do { _ = try await api.deleteAccount(AccountDeletionRequest(email: "wrong@example.com")); preconditionFailure("Wrong identity must not delete") } catch APIError.reauthenticationFailed { }
        _ = try await api.keys()
        let deletion = try await api.deleteAccount(AccountDeletionRequest(email: fresh.user.email))
        precondition(deletion.deleted && deletion.appleRevoked == nil)
        do { _ = try await api.keys(); preconditionFailure("Deleted session must die") } catch APIError.unauthorized { }
        let recreated = try await api.devLogin(email: fresh.user.email)
        precondition(recreated.user.id != fresh.user.id)
        api.token = recreated.sessionToken
        _ = try await api.deleteAccount(AccountDeletionRequest(email: email))
        print("Native API account lifecycle passed: auth provider, other-session revocation, stop, logout, failed reauth, deletion, fresh recreation")
    }
}
