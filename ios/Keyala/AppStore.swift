import Foundation
import SwiftUI
import UserNotifications
import UIKit
import AuthenticationServices
import LocalAuthentication

@MainActor final class AppStore: ObservableObject {
    @Published var requests: [RequestRecord] = []
    @Published var hasOlderRequests = false
    @Published var isLoadingOlderRequests = false
    @Published var olderRequestsError: String?
    @Published var keys: [KeyRecord] = []
    @Published private(set) var routingPreferences = RoutingPreferences()
    @Published private(set) var routingPreferencesLoaded = false
    @Published private(set) var updatingRoutingPreferences = false
    private var routingRevision = 0
    @Published var grants: [GrantRecord] = []
    @Published var demoMode: Bool
    @Published var developerOptions: Bool
    @Published var allowLocalHTTP = UserDefaults.standard.bool(forKey: "allowLocalHTTP")
    @Published var serverURL: String
    @Published var email = UserDefaults.standard.string(forKey: "email") ?? "joseph@example.com"
    @Published var connected = false
    @Published var busy = false
    @Published private(set) var isConnectingOpenRouter = false
    @Published var showError = false
    @Published var errorMessage = ""
    @Published var infoMessage: String?
    @Published var pendingConnectCode: String?
    @Published var pendingEnrollmentID: String?
    @Published private(set) var keySetupReturnCode: String?
    @Published private(set) var linkGeneration = 0
    @Published var pendingRequestID: String?
    private let vault = SessionVault()
    private var approvalAuthentication: LAContext?
    private let providerBrowser = ProviderAuthenticationBrowser()
    @Published private var session: AuthenticatedSession?
    var account: AccountProfile? { session?.user }
    var supportsStagingPassword: Bool {
        (try? client.resolvedURL(for: "/v1/auth/password").absoluteString)
            == "https://masterkey-staging-vkgm.onrender.com/v1/auth/password"
    }
    private var demoLoaded = false
    @Published private(set) var contextGeneration = 0
    private let requestPageSize = 100
    private var requestHistoryCursor = RequestHistoryCursor()
    private var client: APIClient { APIClient(baseURL: serverURL, allowLocalHTTP: allowLocalHTTP, token: session?.sessionToken) }
    private var origin: String {
        guard let url = URL(string: serverURL), let scheme = url.scheme, let host = url.host else { return serverURL }
        let port = url.port.map { ":\($0)" } ?? ""
        return "\(scheme.lowercased())://\(host.lowercased())\(port)"
    }

    init(defaults: UserDefaults = .standard) {
        let preferences = AppPreferences.load(defaults)
        serverURL = preferences.serverURL
        demoMode = preferences.sampleData
        developerOptions = preferences.developerOptions
    }

    func start() async {
        if demoMode { loadDemo(); return }
        session = vault.read(origin: origin)
        if session != nil { await refresh() }
    }
    func saveSettings() {
        UserDefaults.standard.set(demoMode, forKey: "developerSampleData")
        UserDefaults.standard.set(developerOptions, forKey: "developerOptions")
        UserDefaults.standard.set(allowLocalHTTP, forKey: "allowLocalHTTP")
        UserDefaults.standard.set(serverURL, forKey: "serverURL")
        UserDefaults.standard.set(email, forKey: "email")
    }
    func setDemo(_ value: Bool) async {
        signOut()
        demoMode = value; saveSettings()
        if value { demoLoaded = false; loadDemo() }
    }
    func signIn() async {
        let address = email
        await signIn { try await $0.devLogin(email: address) }
    }
    func signInApple(idToken: String) async {
        await signIn { try await $0.appleLogin(idToken: idToken) }
    }
    func signInPassword(username: String, password: String) async {
        guard supportsStagingPassword else { return }
        await signIn { try await $0.passwordLogin(username: username, password: password) }
    }
    private func signIn(using authenticate: (APIClient) async throws -> AuthenticatedSession) async {
        guard !demoMode, !busy else { return }
        signOut()
        let generation = contextGeneration
        let loginOrigin = origin
        busy = true
        defer { busy = false }
        do {
            let result = try await authenticate(client)
            guard generation == contextGeneration, !demoMode, origin == loginOrigin else { return }
            try vault.write(result, origin: loginOrigin)
            session = result
            await refresh()
        }
        catch { if generation == contextGeneration { report(error) } }
    }
    func signOut() {
        providerBrowser.cancel()
        isConnectingOpenRouter = false
        approvalAuthentication?.invalidate()
        approvalAuthentication = nil
        contextGeneration += 1
        try? vault.write(nil, origin: origin)
        session = nil
        routingRevision += 1
        routingPreferences = RoutingPreferences()
        routingPreferencesLoaded = false
        updatingRoutingPreferences = false
        connected = false
        resetRequestPaging()
        keys = []
        grants = []
    }
    func signOutRemotely() async {
        guard !busy, !demoMode, account != nil else { return }
        let generation = contextGeneration; let api = client
        busy = true
        defer { busy = false }
        do {
            try await api.logout()
            guard generation == contextGeneration else { return }
            signOut()
            infoMessage = localized("Signed out on this iPhone. Other devices and app permissions are unchanged.")
        } catch {
            guard generation == contextGeneration else { return }
            if case APIError.unauthorized = error { signOut() }
            else { report(error) }
        }
    }
    func revokeOtherSessions() async {
        guard !busy, !demoMode, account != nil else { return }
        let generation = contextGeneration; let api = client
        busy = true
        defer { busy = false }
        do {
            try await api.revokeOtherSessions()
            guard generation == contextGeneration else { return }
            infoMessage = localized("Other devices have been signed out. App permissions are unchanged.")
        } catch { if generation == contextGeneration { report(error) } }
    }
    func stopAllPermissions() async {
        guard !busy, !demoMode, account != nil else { return }
        let generation = contextGeneration; let api = client
        busy = true
        defer { busy = false }
        do {
            let result = try await api.stopAll()
            guard generation == contextGeneration else { return }
            await refresh()
            guard generation == contextGeneration else { return }
            infoMessage = localized(result.requestsInFlight > 0
                ? "All app permissions stopped. A request already sent to a provider may still finish. Check Activity and provider usage."
                : "All app permissions stopped. Your keys are still stored. Connect again before using an app.")
        } catch { if generation == contextGeneration { report(error) } }
    }
    func deleteAccount(_ body: AccountDeletionRequest) async {
        guard !busy, !demoMode, account != nil else { return }
        let generation = contextGeneration; let api = client
        busy = true
        defer { busy = false }
        do {
            let result = try await api.deleteAccount(body)
            guard generation == contextGeneration else { return }
            guard result.deleted else { throw APIError.invalidResponse }
            signOut()
            pendingConnectCode = nil; pendingEnrollmentID = nil; pendingRequestID = nil; keySetupReturnCode = nil
            linkGeneration += 1
            infoMessage = localized(result.appleRevoked == false
                ? "Account deleted. You can also remove Keyala in iPhone Settings > Apple Account > Sign in with Apple."
                : "Account deleted. Your provider accounts and subscriptions are unchanged.")
        } catch { if generation == contextGeneration { report(error) } }
    }
    func useServer(_ value: String) {
        let value = value.trimmingCharacters(in: .whitespacesAndNewlines)
        do {
            _ = try APIClient(baseURL: value, allowLocalHTTP: allowLocalHTTP, token: nil).resolvedURL(for: "/healthz")
            guard value != serverURL else { return }
            signOut()
            pendingConnectCode = nil
            pendingEnrollmentID = nil
            keySetupReturnCode = nil
            linkGeneration += 1
            pendingRequestID = nil
            serverURL = value
            saveSettings()
            infoMessage = "Server changed. Sign in again to load that server’s account."
        } catch { report(error) }
    }
    func refresh() async {
        if demoMode { if !demoLoaded { loadDemo() }; return }
        guard session != nil else { return }
        let generation = contextGeneration; let api = client
        do {
            async let req = api.requests(limit: requestPageSize)
            async let keyRows = api.keys()
            async let grantRows = api.grants()
            let newRequests = try await req
            let newKeys = try await keyRows; let newGrants = try await grantRows
            guard generation == contextGeneration, !demoMode, !Task.isCancelled else { return }
            requestHistoryCursor.refresh(page: newRequests, existingIDs: Set(requests.map(\.id)), pageSize: requestPageSize)
            requests = RequestHistory.merging(existing: requests, incoming: newRequests)
            hasOlderRequests = requestHistoryCursor.hasMore
            keys = newKeys; grants = newGrants
            connected = true
        } catch {
            guard generation == contextGeneration, !Task.isCancelled else { return }
            if case APIError.unauthorized = error { signOut() }
            report(error)
        }
    }
    func loadRoutingPreferences() async {
        guard account != nil, !demoMode, !updatingRoutingPreferences else { return }
        let generation = contextGeneration, revision = routingRevision, api = client
        do {
            let result = try await api.routingPreferences()
            guard generation == contextGeneration, revision == routingRevision else { return }
            routingPreferences = result; routingPreferencesLoaded = true
        } catch {
            guard generation == contextGeneration, revision == routingRevision else { return }
            routingPreferencesLoaded = false
        }
    }
    func setOpenRouterRouting(_ enabled: Bool) async {
        guard account != nil, !demoMode, routingPreferencesLoaded, !updatingRoutingPreferences else { return }
        routingRevision += 1
        let generation = contextGeneration, revision = routingRevision, api = client
        updatingRoutingPreferences = true
        defer { if generation == contextGeneration, revision == routingRevision { updatingRoutingPreferences = false } }
        do {
            let result = try await api.updateRoutingPreferences(enabled)
            guard generation == contextGeneration, revision == routingRevision else { return }
            routingPreferences = result
        } catch { if generation == contextGeneration, revision == routingRevision { report(error) } }
    }
    func loadOlderRequests() async {
        guard !demoMode, !isLoadingOlderRequests, hasOlderRequests else { return }
        guard let cursor = requestHistoryCursor.beforeID else { hasOlderRequests = false; return }
        let generation = contextGeneration; let api = client
        isLoadingOlderRequests = true; olderRequestsError = nil
        defer { if generation == contextGeneration { isLoadingOlderRequests = false } }
        do {
            let page = try await api.requests(limit: requestPageSize, beforeID: cursor)
            guard generation == contextGeneration, !demoMode, requestHistoryCursor.beforeID == cursor else { return }
            requests = RequestHistory.merging(existing: requests, incoming: page)
            requestHistoryCursor.advance(page: page, from: cursor, pageSize: requestPageSize)
            hasOlderRequests = requestHistoryCursor.hasMore
            if page.count == requestPageSize && requestHistoryCursor.beforeID == cursor {
                olderRequestsError = "The server returned no older requests."
            }
        } catch {
            if generation == contextGeneration { olderRequestsError = error.localizedDescription }
        }
    }
    func decide(_ request: RequestRecord, approve: Bool, policy: Policy = .once) async {
        if demoMode {
            guard let index = requests.firstIndex(where: { $0.id == request.id }) else { return }
            requests[index].status = approve ? "completed" : "denied"
            infoMessage = approve ? "Demo only: request marked approved on this device. No provider call was sent." : "Demo only: request marked denied on this device."
            if approve { requests[index].response = .object(["demo_note": .string("Simulated approval. No provider was contacted."), "choices": .array([.object(["message": .object(["role": .string("assistant"), "content": .string("This is a clearly labeled Keyala demo response.")])])])]) }
            return
        }
        guard !approve || request.payload != nil else { report(APIError.http(0, "The full request payload has not loaded. Open the request details and refresh before deciding.")); return }
        let generation = contextGeneration; let api = client
        do {
            if approve && UserDefaults.standard.bool(forKey: "requireDeviceAuthentication") {
                guard approvalAuthentication == nil else { return }
                let context = LAContext()
                approvalAuthentication = context
                defer { if approvalAuthentication === context { approvalAuthentication = nil } }
                var authenticationError: NSError?
                guard context.canEvaluatePolicy(.deviceOwnerAuthentication, error: &authenticationError) else {
                    if let authenticationError { throw authenticationError }
                    throw APIError.http(0, "Device authentication is unavailable. Check the approval setting in Settings.")
                }
                let allowed = try await context.evaluatePolicy(.deviceOwnerAuthentication,
                    localizedReason: "Confirm this request from \(request.appName) to use your \(request.provider.title) account.")
                guard allowed, generation == contextGeneration, !Task.isCancelled,
                      requests.first(where: { $0.id == request.id })?.status == "pending" else { return }
            }
            let detail = try await api.decide(id: request.id, approve: approve, policy: policy)
            guard generation == contextGeneration else { return }
            requests = RequestHistory.merging(existing: requests, incoming: [detail])
            infoMessage = approve ? "Request approved." : "Request denied."
            await refresh()
        } catch {
            guard generation == contextGeneration else { return }
            if let error = error as? LAError, [.userCancel, .systemCancel, .appCancel].contains(error.code) { return }
            report(error)
        }
    }
    func loadRequestDetails(id: String) async {
        guard !demoMode else { return }
        let generation = contextGeneration; let api = client
        do {
            let detail = try await api.request(id: id)
            guard generation == contextGeneration, !Task.isCancelled else { return }
            requests = RequestHistory.merging(existing: requests, incoming: [detail])
        } catch { if generation == contextGeneration, !Task.isCancelled { report(error) } }
    }
    func addKey(provider: Provider, value: String, label: String) async -> Bool {
        let value = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty else { report(APIError.http(0, "Enter an API key.")); return false }
        if demoMode { infoMessage = "Sample data is offline. Turn it off in Developer options to save a key."; return false }
        let generation = contextGeneration; let api = client
        do { let result = try await api.addKey(provider: provider, value: value, label: label); guard generation == contextGeneration else { return false }; keys.removeAll { $0.provider == provider }; keys.append(result); infoMessage = result.checked == false ? "Key saved. The provider could not verify it; check the key’s permissions if requests fail." : "Key saved. Apps receive permission to use it, never the key itself."; return true }
        catch { if generation == contextGeneration { report(error) }; return false }
    }
    func connectOpenRouter(limitUSD: Double = 10) async {
        guard !demoMode, session != nil, !isConnectingOpenRouter else { return }
        let generation = contextGeneration; let api = client
        var flowID: String?
        isConnectingOpenRouter = true
        defer { if generation == contextGeneration { isConnectingOpenRouter = false } }
        do {
            let authorization = try await api.startOpenRouter(limitUSD: limitUSD)
            flowID = authorization.flowID
            guard generation == contextGeneration, !Task.isCancelled else {
                _ = try? await api.openRouterFlow(id: authorization.flowID, cancel: true)
                return
            }
            let callbackURL = try api.resolvedURL(for: "/oauth/openrouter/callback")
            let url = try authorization.validatedURL(callbackURL: callbackURL, allowMock: developerOptions)
            let returnedURL = try await providerBrowser.authenticate(url: url)
            guard generation == contextGeneration, !Task.isCancelled else { throw CancellationError() }
            try OpenRouterCallback.validate(returnedURL, expectedFlowID: authorization.flowID)
            let flow = try await api.openRouterFlow(id: authorization.flowID)
            guard flow.flowID == authorization.flowID, flow.provider == .openRouter else { throw APIError.invalidResponse }
            guard flow.status == "saved" else {
                _ = try? await api.openRouterFlow(id: authorization.flowID, cancel: true)
                throw APIError.http(0, flow.status == "exchanging" ? "OpenRouter is still finishing. Refresh Keys before starting another connection." : "OpenRouter was not connected. You can start again when ready.")
            }
            let updated = try await api.keys()
            guard generation == contextGeneration, !Task.isCancelled else { return }
            keys = updated
            guard flow.confirmedKey(in: updated, expectedFlowID: authorization.flowID) != nil else {
                throw APIError.http(0, "OpenRouter finished, but its saved key has since changed. Check Keys before trying again.")
            }
            infoMessage = "OpenRouter connected. Requests use your OpenRouter balance."
        } catch {
            let cancelled = error is CancellationError || (error as? ASWebAuthenticationSessionError)?.code == .canceledLogin
            let finalFlow: OpenRouterFlow?
            if let flowID { finalFlow = try? await api.openRouterFlow(id: flowID, cancel: true) }
            else { finalFlow = nil }
            guard generation == contextGeneration else { return }
            if cancelled {
                if let updated = try? await api.keys(), generation == contextGeneration { keys = updated }
                guard generation == contextGeneration else { return }
                switch finalFlow?.status {
                case "saved": infoMessage = "OpenRouter finished before the window closed. Check the connected provider in Keys."
                case "cancelled", "expired", "failed": infoMessage = "OpenRouter connection closed. No retry was sent."
                default: infoMessage = "Connection closed. Refresh Keys to check whether it finished before trying again."
                }
                return
            }
            report(error)
        }
    }
    func removeKey(_ key: KeyRecord) async {
        if demoMode { keys.removeAll { $0.id == key.id }; infoMessage = "Demo key metadata removed from this device."; return }
        let generation = contextGeneration; let api = client
        do { try await api.deleteKey(provider: key.provider); guard generation == contextGeneration else { return }; keys.removeAll { $0.id == key.id }; infoMessage = "Key removed." }
        catch { report(error) }
    }
    func revoke(_ grant: GrantRecord) async -> Bool {
        if demoMode { grants.removeAll { $0.id == grant.id }; infoMessage = "Demo grant revoked on this device only."; return true }
        let generation = contextGeneration; let api = client
        do {
            try await api.revoke(id: grant.id)
            guard generation == contextGeneration else { return false }
            grants.removeAll { $0.id == grant.id }; infoMessage = localized("App disconnected.")
            return true
        } catch { if generation == contextGeneration { report(error) }; return false }
    }
    func update(_ grant: GrantRecord, policy: Policy) async -> Bool {
        if demoMode { infoMessage = "Demo policy changes are not sent to a server."; return false }
        let generation = contextGeneration; let api = client
        do {
            let updated = try await api.updatePolicy(id: grant.id, policy: policy)
            guard generation == contextGeneration else { return false }
            grants = grants.map { $0.id == updated.id ? updated : $0 }
            return true
        } catch { if generation == contextGeneration { report(error) }; return false }
    }
    func connectPreview(code: String) async -> ConnectPreview? {
        if demoMode { return ConnectPreview(app: .init(name: "Sketchpad AI (DEMO)", homepage: nil), vendor: .openAI, suggestedPolicy: nil, origin: "https://example.invalid", requiresAsk: true) }
        let generation = contextGeneration; let api = client
        do { let preview = try await api.connectPreview(code: code); guard generation == contextGeneration else { return nil }; return preview } catch { if generation == contextGeneration { report(error) }; return nil }
    }
    @discardableResult func acceptPairing(_ value: String) -> Bool {
        guard let code = PairingLink.code(from: value) else { return false }
        if let server = PairingLink.serverURL(from: value), server != origin {
            errorMessage = "This connection belongs to a different Keyala server. Choose the matching server in Settings > Developer options, then scan again. Your server has not changed."
            showError = true
            return false
        }
        pendingEnrollmentID = nil
        keySetupReturnCode = nil
        pendingConnectCode = code
        linkGeneration += 1
        return true
    }
    @discardableResult func acceptEnrollment(_ value: String) -> Bool {
        guard let id = EnrollmentLink.id(from: value) else { return false }
        guard !demoMode else {
            errorMessage = "Turn off sample data in Developer options before adding a real key."
            showError = true
            return false
        }
        if let server = PairingLink.serverURL(from: value), server != origin {
            errorMessage = "This key setup belongs to a different Keyala server. Choose the matching server in Settings > Developer options, then scan again. Your server has not changed."
            showError = true
            return false
        }
        if keySetupReturnCode == nil { pendingConnectCode = nil }
        pendingEnrollmentID = id
        linkGeneration += 1
        return true
    }
    @discardableResult func handleOpenURL(_ url: URL) -> Bool {
        if EnrollmentLink.id(from: url.absoluteString) != nil { return acceptEnrollment(url.absoluteString) }
        if PairingLink.code(from: url.absoluteString) != nil { return acceptPairing(url.absoluteString) }
        guard url.scheme?.lowercased() == "keyala", url.user == nil, url.password == nil, url.port == nil,
              url.query == nil, url.fragment == nil, url.pathComponents.count == 2,
              url.host?.lowercased() == "requests" else { return false }
        let id = url.lastPathComponent
        guard id.count < 100, id.range(of: "^req_[A-Za-z0-9_-]+$", options: .regularExpression) != nil else { return false }
        pendingRequestID = id
        Task { await refresh() }
        return false
    }

    func beginComputerKeySetup(returnTo code: String?) {
        keySetupReturnCode = code.flatMap { PairingLink.code(from: $0) }
        if let keySetupReturnCode { pendingConnectCode = keySetupReturnCode }
    }
    var canResumePairing: Bool { keySetupReturnCode != nil && keySetupReturnCode == pendingConnectCode }
    func resumePairingAfterKeySetup() async {
        guard canResumePairing, let code = keySetupReturnCode else { return }
        let generation = contextGeneration; let link = linkGeneration; let api = client
        do {
            _ = try await api.connectPreview(code: code)
            guard generation == contextGeneration, link == linkGeneration, !Task.isCancelled,
                  canResumePairing, keySetupReturnCode == code else { return }
            _ = acceptPairing(code)
        } catch {
            guard generation == contextGeneration, link == linkGeneration, !Task.isCancelled else { return }
            if case APIError.http(let status, _) = error, [404, 410].contains(status) {
                keySetupReturnCode = nil
                if pendingConnectCode == code { pendingConnectCode = nil }
            }
            report(error)
        }
    }
    var computerSetupURL: URL? { try? client.resolvedURL(for: "/add-key") }
    var serverHost: String {
        guard let url = URL(string: serverURL), let host = url.host else { return serverURL }
        return url.port.map { "\(host):\($0)" } ?? host
    }

    func enrollment(id: String) async throws -> KeyEnrollment {
        guard !demoMode else { throw APIError.missingSession }
        let generation = contextGeneration; let api = client
        let result = try await api.enrollment(id: id)
        guard generation == contextGeneration, !Task.isCancelled else { throw CancellationError() }
        guard result.id == id else { throw APIError.invalidResponse }
        if result.status == "saved" {
            let loadedKeys = try await api.keys()
            guard generation == contextGeneration, !Task.isCancelled else { throw CancellationError() }
            keys = loadedKeys
        }
        return result
    }
    func claimEnrollment(id: String) async throws -> KeyEnrollment {
        guard !demoMode else { throw APIError.missingSession }
        let generation = contextGeneration; let api = client
        _ = try await api.claimEnrollment(id: id)
        guard generation == contextGeneration, !Task.isCancelled else { throw CancellationError() }
        return try await enrollment(id: id)
    }
    func confirmEnrollment(_ enrollment: KeyEnrollment) async throws {
        guard !demoMode, enrollment.canConfirm else { throw APIError.invalidResponse }
        let generation = contextGeneration; let api = client
        let key = try await api.confirmEnrollment(id: enrollment.id)
        guard generation == contextGeneration, !Task.isCancelled else { throw CancellationError() }
        keys.removeAll { $0.provider == key.provider }; keys.append(key)
        infoMessage = "\(key.provider.title) key saved. Connected apps never receive the key itself."
    }
    func cancelEnrollment(id: String) async throws {
        guard !demoMode else { throw APIError.missingSession }
        let generation = contextGeneration; let api = client
        try await api.cancelEnrollment(id: id)
        guard generation == contextGeneration, !Task.isCancelled else { throw CancellationError() }
        if pendingEnrollmentID == id { pendingEnrollmentID = nil }
    }
    func connectDecision(code: String, approve: Bool, policy: Policy, billingVendor: Provider? = nil) async -> Bool {
        if demoMode {
            infoMessage = "Demo pairing action is local only. No grant token or connection was created."
            return true
        }
        let generation = contextGeneration; let api = client
        do {
            try await api.connectDecision(code: code, approve: approve, policy: policy, billingVendor: billingVendor)
            guard generation == contextGeneration else { return false }
            infoMessage = approve ? "App connected." : "Connection denied."
            await refresh()
            return generation == contextGeneration
        } catch {
            if generation == contextGeneration { report(error) }
            return false
        }
    }
    func requestNotifications() async {
        guard !demoMode else { infoMessage = "Turn off sample data to enable live notifications."; return }
        let center = UNUserNotificationCenter.current()
        do {
            let granted = try await center.requestAuthorization(options: [.alert, .badge, .sound])
            guard granted else { infoMessage = "Notifications are off. Keyala refreshes requests whenever the app opens."; return }
            UIApplication.shared.registerForRemoteNotifications()
            infoMessage = "Push registration requested. Foreground refresh remains available."
        } catch { report(error) }
    }
    func uploadDeviceToken(_ token: String) async {
        guard !demoMode, session != nil else { return }
        let generation = contextGeneration; let api = client
        do { try await api.registerDevice(token: token); guard generation == contextGeneration else { return }; infoMessage = "This device is registered for push notifications." }
        catch { report(error) }
    }
    private func report(_ error: Error) { errorMessage = error.localizedDescription; showError = true }
    private func resetRequestPaging() {
        requests = []
        hasOlderRequests = false
        isLoadingOlderRequests = false
        olderRequestsError = nil
        requestHistoryCursor = RequestHistoryCursor()
    }
    private func loadDemo() {
        guard !demoLoaded else { return }
        demoLoaded = true
        connected = false
        requests = DemoFixtures.requests
        keys = [KeyRecord(id: "demo-key-1", provider: .openAI, label: "Personal", lastFour: "•••• 7K2P"), KeyRecord(id: "demo-key-2", provider: .anthropic, label: "Research", lastFour: "•••• 4M8Q")]
        grants = [GrantRecord(id: "demo-grant-1", appID: "demo-app-1", appName: "Sketchpad AI", provider: .openAI, model: "gpt-4.1-mini", maxRequests: 10, usedRequests: 3, budgetMicrousd: 2_000_000, spentMicrousd: 420_000, reservedMicrousd: 0, expiresAt: "2026-12-01T00:00:00Z", trust: false, alertThresholdMicrousd: nil), GrantRecord(id: "demo-grant-2", appID: "demo-app-2", appName: "Quiet Research", provider: .anthropic, model: "claude-sonnet-4", maxRequests: nil, usedRequests: 18, budgetMicrousd: nil, spentMicrousd: 1_340_000, reservedMicrousd: 0, expiresAt: "2026-12-14T00:00:00Z", trust: true, alertThresholdMicrousd: 500_000)]
    }
}

enum DemoFixtures {
    static let requests: [RequestRecord] = [
        RequestRecord(id: "demo-req-4821", appID: "demo-app-1", appName: "Sketchpad AI", provider: .openAI, model: "gpt-4.1-mini", status: "pending", createdAt: "2026-11-14T08:22:00Z", expiresAt: "2026-11-14T08:23:00Z", payload: .object(["model": .string("gpt-4.1-mini"), "messages": .array([.object(["role": .string("system"), "content": .string("You are a concise design assistant.")]), .object(["role": .string("user"), "content": .string("Suggest three calmer colors for this dashboard: #F05A28, #192B47. Keep the palette accessible and explain your choices.")])]), "max_tokens": .number(240)]), payloadSHA256: nil, estimatedCostMicrousd: 28, reservedCostMicrousd: 28, actualCostMicrousd: nil, response: nil, error: nil, isDemo: true),
        RequestRecord(id: "demo-req-4817", appID: "demo-app-2", appName: "Quiet Research", provider: .anthropic, model: "claude-sonnet-4", status: "completed", createdAt: "2026-11-14T07:58:00Z", expiresAt: nil, payload: .object(["model": .string("claude-sonnet-4"), "max_tokens": .number(160), "messages": .array([.object(["role": .string("user"), "content": .string("Summarize the key tradeoffs in this paragraph in two bullets.")])])]), payloadSHA256: nil, estimatedCostMicrousd: 61, reservedCostMicrousd: 61, actualCostMicrousd: 54, response: .object(["content": .array([.object(["type": .string("text"), "text": .string("• Simplicity improves adoption, while explicit permissions add friction.\n• Smaller models cut cost, while larger models may improve quality.")])]), "demo": .bool(true)]), error: nil, isDemo: true)
    ]
}

final class APNSDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        let center = UNUserNotificationCenter.current()
        center.delegate = self
        center.setNotificationCategories([UNNotificationCategory(identifier: "KEYALA_REQUEST", actions: [], intentIdentifiers: [])])
        return true
    }
    func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        guard response.actionIdentifier == UNNotificationDefaultActionIdentifier,
              let id = response.notification.request.content.userInfo["request_id"] as? String,
              id.hasPrefix("req_"), id.count < 100 else { return }
        await MainActor.run { NotificationCenter.default.post(name: .keyalaRequestTapped, object: id) }
    }
    func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        [.banner, .sound]
    }
    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        let token = deviceToken.map { String(format: "%02.2hhx", $0) }.joined()
        Task { @MainActor in NotificationCenter.default.post(name: .keyalaAPNSToken, object: token) }
    }
    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
        NotificationCenter.default.post(name: .keyalaAPNSError, object: error.localizedDescription)
    }
}
extension Notification.Name {
    static let keyalaAddFromComputer = Notification.Name("keyalaAddFromComputer")
    static let keyalaOpenKeys = Notification.Name("keyalaOpenKeys")
    static let keyalaConnectApp = Notification.Name("keyalaConnectApp")
    static let keyalaAPNSToken = Notification.Name("keyalaAPNSToken")
    static let keyalaAPNSError = Notification.Name("keyalaAPNSError")
    static let keyalaRequestTapped = Notification.Name("keyalaRequestTapped")
}

@MainActor private final class ProviderAuthenticationBrowser: NSObject, ASWebAuthenticationPresentationContextProviding {
    private var browserSession: ASWebAuthenticationSession?
    private var completion: CheckedContinuation<URL, Error>?
    private var attemptID: UUID?

    func authenticate(url: URL) async throws -> URL {
        guard completion == nil else { throw APIError.http(0, "Finish the current provider connection first.") }
        return try await withCheckedThrowingContinuation { continuation in
            let attempt = UUID()
            attemptID = attempt
            completion = continuation
            let session = ASWebAuthenticationSession(url: url, callbackURLScheme: "keyala") { [weak self] url, error in
                Task { @MainActor in
                    if let error { self?.finish(.failure(error), attempt: attempt) }
                    else if let url { self?.finish(.success(url), attempt: attempt) }
                    else { self?.finish(.failure(APIError.invalidResponse), attempt: attempt) }
                }
            }
            session.presentationContextProvider = self
            browserSession = session
            if !session.start() { finish(.failure(APIError.http(0, "Open Keyala in the foreground, then try connecting again.")), attempt: attempt) }
        }
    }
    func cancel() {
        guard let attempt = attemptID else { return }
        browserSession?.cancel()
        finish(.failure(CancellationError()), attempt: attempt)
    }
    private func finish(_ result: Result<URL, Error>, attempt: UUID) {
        guard attemptID == attempt, let continuation = completion else { return }
        completion = nil; browserSession = nil; attemptID = nil
        continuation.resume(with: result)
    }
    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
            .filter { $0.activationState == .foregroundActive }
            .flatMap(\.windows).first(where: \.isKeyWindow) ?? ASPresentationAnchor()
    }
}
