import Foundation
import SwiftUI
import UserNotifications
import UIKit
import AuthenticationServices

@MainActor final class AppStore: ObservableObject {
    @Published var requests: [RequestRecord] = []
    @Published var hasOlderRequests = false
    @Published var isLoadingOlderRequests = false
    @Published var olderRequestsError: String?
    @Published var keys: [KeyRecord] = []
    @Published var grants: [GrantRecord] = []
    @Published var demoMode = UserDefaults.standard.object(forKey: "demoMode") as? Bool ?? true
    @Published var allowLocalHTTP = UserDefaults.standard.bool(forKey: "allowLocalHTTP")
    @Published var serverURL = UserDefaults.standard.string(forKey: "serverURL") ?? "https://localhost:8443"
    @Published var email = UserDefaults.standard.string(forKey: "email") ?? "joseph@example.com"
    @Published var connected = false
    @Published var busy = false
    @Published var showError = false
    @Published var errorMessage = ""
    @Published var infoMessage: String?
    @Published var pendingConnectCode: String?
    @Published var pendingRequestID: String?
    private let vault = SessionVault()
    private var sessionToken: String?
    private var demoLoaded = false
    private var contextGeneration = 0
    private let requestPageSize = 100
    private var requestHistoryCursor = RequestHistoryCursor()
    private var client: APIClient { APIClient(baseURL: serverURL, allowLocalHTTP: allowLocalHTTP, token: sessionToken) }
    private var origin: String {
        guard let url = URL(string: serverURL), let scheme = url.scheme, let host = url.host else { return serverURL }
        let port = url.port.map { ":\($0)" } ?? ""
        return "\(scheme.lowercased())://\(host.lowercased())\(port)"
    }

    func start() async {
        sessionToken = vault.read(origin: origin)
        if demoMode { loadDemo() } else if sessionToken != nil { await refresh() }
    }
    func saveSettings() {
        UserDefaults.standard.set(demoMode, forKey: "demoMode")
        UserDefaults.standard.set(allowLocalHTTP, forKey: "allowLocalHTTP")
        UserDefaults.standard.set(serverURL, forKey: "serverURL")
        UserDefaults.standard.set(email, forKey: "email")
    }
    func setDemo(_ value: Bool) async {
        contextGeneration += 1
        resetRequestPaging()
        demoMode = value; saveSettings()
        if value { sessionToken = nil; try? vault.write(nil, origin: origin); connected = false; demoLoaded = false; loadDemo() }
        else { keys = []; grants = []; connected = sessionToken != nil; if connected { await refresh() } }
    }
    func signIn() async {
        guard !demoMode else { infoMessage = "Demo workspace is local sample data. No server session was created."; return }
        contextGeneration += 1
        let generation = contextGeneration
        let loginOrigin = origin
        let api = client
        try? vault.write(nil, origin: loginOrigin); sessionToken = nil; connected = false; resetRequestPaging(); keys = []; grants = []
        busy = true; defer { busy = false }
        do {
            let token = try await api.devLogin(email: email)
            guard generation == contextGeneration, !demoMode, origin == loginOrigin else { return }
            try vault.write(token, origin: loginOrigin); sessionToken = token; connected = true
            infoMessage = "Connected. Session token saved to this device’s Keychain."; await refresh()
        }
        catch { report(error) }
    }
    func signInApple(idToken: String) async {
        guard !demoMode else { infoMessage = "Apple sign-in is unavailable in the local demo workspace."; return }
        contextGeneration += 1
        let generation = contextGeneration; let loginOrigin = origin; let api = client
        try? vault.write(nil, origin: loginOrigin); sessionToken = nil; connected = false; resetRequestPaging(); keys = []; grants = []
        do {
            let token = try await api.appleLogin(idToken: idToken)
            guard generation == contextGeneration, !demoMode, origin == loginOrigin else { return }
            try vault.write(token, origin: loginOrigin); sessionToken = token; connected = true; await refresh()
        }
        catch { report(error) }
    }
    func signOut() { contextGeneration += 1; try? vault.write(nil, origin: origin); sessionToken = nil; connected = false; resetRequestPaging(); keys = []; grants = [] }
    func serverChanged() { contextGeneration += 1; vault.clear(); sessionToken = nil; connected = false; resetRequestPaging(); keys = []; grants = []; infoMessage = "Server changed. Sign in again to load that server’s account." }
    func refresh() async {
        if demoMode { if !demoLoaded { loadDemo() }; return }
        guard sessionToken != nil else { return }
        let generation = contextGeneration; let api = client
        do {
            async let req = api.requests(limit: requestPageSize)
            async let keyRows = api.keys()
            async let grantRows = api.grants()
            let newRequests = try await req
            let newKeys = try await keyRows; let newGrants = try await grantRows
            guard generation == contextGeneration, !demoMode else { return }
            requestHistoryCursor.refresh(page: newRequests, existingIDs: Set(requests.map(\.id)), pageSize: requestPageSize)
            requests = RequestHistory.merging(existing: requests, incoming: newRequests)
            hasOlderRequests = requestHistoryCursor.hasMore
            keys = newKeys; grants = newGrants
            connected = true
        } catch { report(error) }
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
            if approve { requests[index].response = .object(["demo_note": .string("Simulated approval. No provider was contacted."), "choices": .array([.object(["message": .object(["role": .string("assistant"), "content": .string("This is a clearly labeled MasterKey demo response.")])])])]) }
            return
        }
        guard !approve || request.payload != nil else { report(APIError.http(0, "The full request payload has not loaded. Open the request details and refresh before deciding.")); return }
        let generation = contextGeneration; let api = client
        do { _ = try await api.decide(id: request.id, approve: approve, policy: policy); guard generation == contextGeneration else { return }; infoMessage = approve ? "Request approved." : "Request denied."; await refresh() }
        catch { report(error) }
    }
    func loadRequestDetails(id: String) async {
        guard !demoMode else { return }
        let generation = contextGeneration; let api = client
        do { let detail = try await api.request(id: id); guard generation == contextGeneration else { return }; if let index = requests.firstIndex(where: { $0.id == id }) { requests[index] = detail } }
        catch { report(error) }
    }
    func addKey(provider: Provider, value: String, label: String) async {
        guard !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { report(APIError.http(0, "Enter an API key.")); return }
        if demoMode { infoMessage = "Demo mode never saves or transmits entered keys. Switch to a live HTTPS server to add a key."; return }
        let generation = contextGeneration; let api = client
        do { let result = try await api.addKey(provider: provider, value: value, label: label); guard generation == contextGeneration else { return }; keys.removeAll { $0.provider == provider }; keys.append(result); infoMessage = "Key saved securely by the server. The key value was not returned." }
        catch { report(error) }
    }
    func removeKey(_ key: KeyRecord) async {
        if demoMode { keys.removeAll { $0.id == key.id }; infoMessage = "Demo key metadata removed from this device."; return }
        let generation = contextGeneration; let api = client
        do { try await api.deleteKey(provider: key.provider); guard generation == contextGeneration else { return }; keys.removeAll { $0.id == key.id }; infoMessage = "Key removed." }
        catch { report(error) }
    }
    func revoke(_ grant: GrantRecord) async {
        if demoMode { grants.removeAll { $0.id == grant.id }; infoMessage = "Demo grant revoked on this device only."; return }
        let generation = contextGeneration; let api = client
        do { try await api.revoke(id: grant.id); guard generation == contextGeneration else { return }; grants.removeAll { $0.id == grant.id }; infoMessage = "Grant revoked." }
        catch { report(error) }
    }
    func update(_ grant: GrantRecord, policy: Policy) async {
        if demoMode { infoMessage = "Demo policy changes are not sent to a server."; return }
        let generation = contextGeneration; let api = client
        do { _ = try await api.updatePolicy(id: grant.id, policy: policy); guard generation == contextGeneration else { return }; await refresh() }
        catch { report(error) }
    }
    func connectPreview(code: String) async -> ConnectPreview? {
        if demoMode { return ConnectPreview(app: .init(name: "Sketchpad AI (DEMO)", homepage: "https://example.invalid"), vendor: .openAI, suggestedPolicy: .init(mode: "ask", remainingCount: nil)) }
        let generation = contextGeneration; let api = client
        do { let preview = try await api.connectPreview(code: code); guard generation == contextGeneration else { return nil }; return preview } catch { if generation == contextGeneration { report(error) }; return nil }
    }
    func handleOpenURL(_ url: URL) {
        guard url.scheme?.lowercased() == "masterkey",
              url.query == nil, url.fragment == nil, url.pathComponents.count == 2 else { return }
        if url.host?.lowercased() == "requests" {
            let id = url.lastPathComponent
            guard id.count < 100, id.range(of: "^req_[A-Za-z0-9_-]+$", options: .regularExpression) != nil else { return }
            pendingRequestID = id
            Task { await refresh() }
            return
        }
        guard url.host?.lowercased() == "connect" else { return }
        let code = url.lastPathComponent.uppercased()
        guard code.range(of: "^[A-Z0-9-]{4,24}$", options: .regularExpression) != nil else { return }
        pendingConnectCode = code
    }
    func connectDecision(code: String, approve: Bool, policy: Policy) async {
        if demoMode { infoMessage = "Demo pairing action is local only. No grant token or connection was created."; return }
        let generation = contextGeneration; let api = client
        do { try await api.connectDecision(code: code, approve: approve, policy: policy); guard generation == contextGeneration else { return }; infoMessage = approve ? "App connected." : "Connection denied."; await refresh() }
        catch { report(error) }
    }
    func requestNotifications() async {
        let center = UNUserNotificationCenter.current()
        do {
            let granted = try await center.requestAuthorization(options: [.alert, .badge, .sound])
            guard granted else { infoMessage = "Notifications are off. MasterKey refreshes requests whenever the app opens."; return }
            UIApplication.shared.registerForRemoteNotifications()
            infoMessage = "Push registration requested. Foreground refresh remains available."
        } catch { report(error) }
    }
    func uploadDeviceToken(_ token: String) async {
        guard !demoMode, sessionToken != nil else { return }
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
        center.setNotificationCategories([UNNotificationCategory(identifier: "MK_REQUEST", actions: [], intentIdentifiers: [])])
        return true
    }
    func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        guard response.actionIdentifier == UNNotificationDefaultActionIdentifier,
              let id = response.notification.request.content.userInfo["request_id"] as? String,
              id.hasPrefix("req_"), id.count < 100 else { return }
        await MainActor.run { NotificationCenter.default.post(name: .masterKeyRequestTapped, object: id) }
    }
    func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        [.banner, .sound]
    }
    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        let token = deviceToken.map { String(format: "%02.2hhx", $0) }.joined()
        Task { @MainActor in NotificationCenter.default.post(name: .masterKeyAPNSToken, object: token) }
    }
    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
        NotificationCenter.default.post(name: .masterKeyAPNSError, object: error.localizedDescription)
    }
}
extension Notification.Name {
    static let masterKeyAPNSToken = Notification.Name("masterKeyAPNSToken")
    static let masterKeyAPNSError = Notification.Name("masterKeyAPNSError")
    static let masterKeyRequestTapped = Notification.Name("masterKeyRequestTapped")
}
