import Foundation

enum Provider: String, Codable, CaseIterable, Identifiable {
    case openAI = "openai"
    case anthropic
    case openRouter = "openrouter"
    var id: String { rawValue }
    var title: String {
        switch self {
        case .openAI: "OpenAI"
        case .anthropic: "Anthropic"
        case .openRouter: "OpenRouter"
        }
    }
    var keyPlaceholder: String {
        switch self {
        case .openAI: "sk-..."
        case .anthropic: "sk-ant-..."
        case .openRouter: "sk-or-v1-..."
        }
    }
    var keysURL: URL {
        switch self {
        case .openAI: URL(string: "https://platform.openai.com/api-keys")!
        case .anthropic: URL(string: "https://platform.claude.com/settings/keys")!
        case .openRouter: URL(string: "https://openrouter.ai/settings/keys")!
        }
    }
}

struct RoutingPreferences: Codable {
    var openRouterForDirectAPIs = false
    enum CodingKeys: String, CodingKey { case openRouterForDirectAPIs = "openrouter_for_direct_apis" }
}

struct RequestRecord: Codable, Identifiable {
    var id: String
    var appID: String
    var appName: String
    var provider: Provider
    var billingVendor: Provider? = nil
    var upstreamModel: String? = nil
    var model: String
    var status: String
    var createdAt: String
    var expiresAt: String?
    var payload: JSONValue?
    var actualCostMicrousd: Int64?
    var response: JSONValue?
    var error: String?
    var isDemo: Bool = false
    var estimatedCostMicrousd: Int64
    var actualUSD: String { actualCostMicrousd.map { String(format: "$%.6f", Double($0) / 1_000_000) } ?? "Not reported" }
    var estimatedUSD: String { String(format: "$%.6f", Double(estimatedCostMicrousd) / 1_000_000) }

    enum CodingKeys: String, CodingKey { case id, status, model, payload, response, error; case billingVendor = "billing_vendor", upstreamModel = "upstream_model", grantID = "grant_id", app, vendor, provider, createdAt = "created_at", expiresAt = "expires_at", actualUSD = "actual_usd", estimatedUSD = "estimated_usd" }
    init(id: String, appID: String, appName: String, provider: Provider, model: String, status: String, createdAt: String, expiresAt: String?, payload: JSONValue?, payloadSHA256: String? = nil, estimatedCostMicrousd: Int64, reservedCostMicrousd: Int64 = 0, actualCostMicrousd: Int64?, response: JSONValue?, error: String?, isDemo: Bool = false) {
        self.id = id; self.appID = appID; self.appName = appName; self.provider = provider; self.model = model; self.status = status; self.createdAt = createdAt; self.expiresAt = expiresAt; self.payload = payload; self.estimatedCostMicrousd = estimatedCostMicrousd; self.actualCostMicrousd = actualCostMicrousd; self.response = response; self.error = error; self.isDemo = isDemo
    }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let app = try c.decodeIfPresent(WireApp.self, forKey: .app)
        id = try c.decode(String.self, forKey: .id); appID = try app?.id ?? c.decodeIfPresent(String.self, forKey: .grantID) ?? ""
        appName = app?.name ?? "App"; provider = try c.decodeIfPresent(Provider.self, forKey: .vendor) ?? c.decode(Provider.self, forKey: .provider)
        billingVendor = try c.decodeIfPresent(Provider.self, forKey: .billingVendor); upstreamModel = try c.decodeIfPresent(String.self, forKey: .upstreamModel)
        model = try c.decode(String.self, forKey: .model); status = try c.decode(String.self, forKey: .status); createdAt = try c.decode(String.self, forKey: .createdAt)
        expiresAt = try c.decodeIfPresent(String.self, forKey: .expiresAt); payload = try c.decodeIfPresent(JSONValue.self, forKey: .payload)
        estimatedCostMicrousd = try microUSD(c.decodeIfPresent(Double.self, forKey: .estimatedUSD), decoder: decoder) ?? 0
        actualCostMicrousd = try microUSD(c.decodeIfPresent(Double.self, forKey: .actualUSD), decoder: decoder)
        response = try c.decodeIfPresent(JSONValue.self, forKey: .response); error = try c.decodeIfPresent(String.self, forKey: .error); isDemo = false
    }
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self); try c.encode(id, forKey: .id); try c.encode(appID, forKey: .grantID); try c.encode(provider, forKey: .vendor)
        try c.encode(model, forKey: .model); try c.encode(status, forKey: .status); try c.encode(createdAt, forKey: .createdAt); try c.encodeIfPresent(expiresAt, forKey: .expiresAt)
        try c.encodeIfPresent(payload, forKey: .payload); try c.encodeIfPresent(response, forKey: .response); try c.encodeIfPresent(error, forKey: .error)
        try c.encode(Double(estimatedCostMicrousd) / 1_000_000, forKey: .estimatedUSD); try c.encodeIfPresent(actualCostMicrousd.map { Double($0) / 1_000_000 }, forKey: .actualUSD)
    }
}

enum RequestHistory {
    static func merging(existing: [RequestRecord], incoming: [RequestRecord]) -> [RequestRecord] {
        var byID = existing.reduce(into: [String: RequestRecord]()) { $0[$1.id] = $1 }
        for received in incoming {
            var current = received
            if let old = byID[received.id] {
                if current.payload == nil { current.payload = old.payload }
                if current.response == nil { current.response = old.response }
            }
            byID[received.id] = current
        }
        // The server emits fixed-width UTC millisecond timestamps, matching its cursor order.
        return byID.values.sorted { $0.createdAt == $1.createdAt ? $0.id > $1.id : $0.createdAt > $1.createdAt }
    }
}

struct RequestHistoryCursor {
    private(set) var beforeID: String?
    private(set) var hasMore = false

    mutating func refresh(page: [RequestRecord], existingIDs: Set<String>, pageSize: Int) {
        if page.count < pageSize {
            beforeID = page.last?.id
            hasMore = false
        } else if beforeID == nil || !page.contains(where: { existingIDs.contains($0.id) }) {
            // A full new head can leave a gap ahead of retained older records.
            beforeID = page.last?.id
            hasMore = true
        }
    }

    mutating func advance(page: [RequestRecord], from cursor: String, pageSize: Int) {
        beforeID = page.last?.id ?? cursor
        hasMore = page.count == pageSize && beforeID != cursor
    }
}

struct KeyRecord: Codable, Identifiable {
    var id: String
    var provider: Provider
    var label: String
    var lastFour: String
    var createdAt: String?
    var checked: Bool? = nil
    enum CodingKeys: String, CodingKey { case vendor, last4, label, checked; case createdAt = "created_at" }
    init(id: String, provider: Provider, label: String, lastFour: String, createdAt: String? = nil) { self.id = id; self.provider = provider; self.label = label; self.lastFour = lastFour; self.createdAt = createdAt }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self); provider = try c.decode(Provider.self, forKey: .vendor)
        id = provider.rawValue; label = try c.decodeIfPresent(String.self, forKey: .label) ?? provider.title; lastFour = try c.decode(String.self, forKey: .last4)
        createdAt = try c.decodeIfPresent(String.self, forKey: .createdAt)
        checked = try c.decodeIfPresent(Bool.self, forKey: .checked)
    }
    func encode(to encoder: Encoder) throws { var c = encoder.container(keyedBy: CodingKeys.self); try c.encode(provider, forKey: .vendor); try c.encode(lastFour, forKey: .last4); try c.encode(label, forKey: .label); try c.encodeIfPresent(createdAt, forKey: .createdAt); try c.encodeIfPresent(checked, forKey: .checked) }
}

struct GrantRecord: Codable, Identifiable {
    var id: String
    var appID: String
    var appName: String
    var provider: Provider
    var billingVendor: Provider? = nil
    var upstreamModel: String? = nil
    var model: String
    var maxRequests: Int?
    var usedRequests: Int
    var budgetMicrousd: Int64?
    var spentMicrousd: Int64
    var reservedMicrousd: Int64
    var expiresAt: String
    var trust: Bool
    var alertThresholdMicrousd: Int64?
    var spentUSD: String { String(format: "$%.2f", Double(spentMicrousd) / 1_000_000) }
    enum CodingKeys: String, CodingKey { case id, app, vendor, policy; case billingVendor = "billing_vendor", upstreamModel = "upstream_model", createdAt = "created_at", lastUsedAt = "last_used_at", spentUSD = "spent_usd", requestCount = "request_count" }
    init(id: String, appID: String, appName: String, provider: Provider, model: String, maxRequests: Int?, usedRequests: Int, budgetMicrousd: Int64?, spentMicrousd: Int64, reservedMicrousd: Int64, expiresAt: String, trust: Bool, alertThresholdMicrousd: Int64?) {
        self.id = id; self.appID = appID; self.appName = appName; self.provider = provider; self.model = model; self.maxRequests = maxRequests; self.usedRequests = usedRequests; self.budgetMicrousd = budgetMicrousd; self.spentMicrousd = spentMicrousd; self.reservedMicrousd = reservedMicrousd; self.expiresAt = expiresAt; self.trust = trust; self.alertThresholdMicrousd = alertThresholdMicrousd
    }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self); let app = try c.decode(WireApp.self, forKey: .app); let policy = try c.decode(WirePolicy.self, forKey: .policy)
        id = try c.decode(String.self, forKey: .id); appID = app.id ?? ""; appName = app.name; provider = try c.decode(Provider.self, forKey: .vendor); model = ""
        billingVendor = try c.decodeIfPresent(Provider.self, forKey: .billingVendor)
        trust = policy.mode == "trust"; maxRequests = policy.remainingCount; usedRequests = try c.decodeIfPresent(Int.self, forKey: .requestCount) ?? 0
        budgetMicrousd = try microUSD(policy.remainingUSD, decoder: decoder); spentMicrousd = try microUSD(c.decodeIfPresent(Double.self, forKey: .spentUSD), decoder: decoder) ?? 0
        reservedMicrousd = 0; expiresAt = policy.expiresAt ?? ""; alertThresholdMicrousd = try microUSD(policy.alertUSDPerDay, decoder: decoder)
    }
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self); try c.encode(id, forKey: .id); try c.encode(provider, forKey: .vendor); try c.encode(appName, forKey: .app); try c.encode(spentUSD, forKey: .spentUSD); try c.encode(usedRequests, forKey: .requestCount)
    }
}

private struct WireApp: Decodable { let id: String?; let name: String }
private struct WirePolicy: Decodable {
    let mode: String
    let remainingCount: Int?
    let remainingUSD: Double?
    let alertUSDPerDay: Double?
    let expiresAt: String?
    enum CodingKeys: String, CodingKey { case mode, remainingCount = "remaining_count", remainingUSD = "remaining_usd", alertUSDPerDay = "alert_usd_per_day", expiresAt = "expires_at" }
}

private func microUSD(_ amount: Double?, decoder: Decoder) throws -> Int64? {
    guard let amount else { return nil }
    let scaled = (amount * 1_000_000).rounded()
    guard scaled.isFinite, scaled >= 0, scaled < Double(Int64.max) else {
        throw DecodingError.dataCorrupted(.init(codingPath: decoder.codingPath, debugDescription: "USD amount is outside the supported range."))
    }
    return Int64(scaled)
}

indirect enum JSONValue: Codable, Equatable {
    case object([String: JSONValue]), array([JSONValue]), string(String), number(Decimal), bool(Bool), null
    init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() { self = .null }
        else if let v = try? c.decode(Bool.self) { self = .bool(v) }
        else if let v = try? c.decode(Decimal.self) { self = .number(v) }
        else if let v = try? c.decode(String.self) { self = .string(v) }
        else if let v = try? c.decode([String: JSONValue].self) { self = .object(v) }
        else { self = .array(try c.decode([JSONValue].self)) }
    }
    func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self { case .object(let v): try c.encode(v); case .array(let v): try c.encode(v); case .string(let v): try c.encode(v); case .number(let v): try c.encode(v); case .bool(let v): try c.encode(v); case .null: try c.encodeNil() }
    }
    var prettyPrinted: String {
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        guard let data = try? encoder.encode(self), let text = String(data: data, encoding: .utf8) else { return "{}" }
        return text
    }
}

struct Policy: Encodable {
    let mode: String
    var remainingCount: Int?
    var remainingUSD: Double?
    var alertUSDPerDay: Double?
    var expiresAt: String? = nil
    enum CodingKeys: String, CodingKey { case mode, expiresAt = "expires_at", remainingCount = "remaining_count", remainingUSD = "remaining_usd", alertUSDPerDay = "alert_usd_per_day" }
    static let once = Policy(mode: "ask")
}
struct AccountProfile: Codable {
    let id: String
    let email: String
    var authProvider: String? = nil

    enum CodingKeys: String, CodingKey { case id, email; case authProvider = "auth_provider" }

    var displayEmail: String? { email.hasSuffix("@apple.invalid") ? nil : email }
}

struct AccountDeletionRequest: Encodable {
    let confirm = "DELETE"
    var idToken: String? = nil
    var authorizationCode: String? = nil
    var password: String? = nil
    var email: String? = nil
    enum CodingKeys: String, CodingKey {
        case confirm, password, email
        case idToken = "id_token", authorizationCode = "authorization_code"
    }
}

struct AccountDeletionResult: Decodable {
    let deleted: Bool
    let appleRevoked: Bool?
    enum CodingKeys: String, CodingKey { case deleted; case appleRevoked = "apple_revoked" }
}

struct StopAllResult: Decodable {
    let requestsInFlight: Int
    enum CodingKeys: String, CodingKey { case requestsInFlight = "requests_in_flight" }
}

struct AuthenticatedSession: Codable {
    let sessionToken: String
    let user: AccountProfile

    enum CodingKeys: String, CodingKey { case user; case sessionToken = "session_token" }
}


struct AppPreferences {
    static let productionURL = "https://masterkey-prod-vkgm.onrender.com"
    static let stagingURL = "https://masterkey-staging-vkgm.onrender.com"
    let serverURL: String
    let sampleData: Bool
    let developerOptions: Bool

    static func load(_ defaults: UserDefaults) -> AppPreferences {
        let saved = defaults.string(forKey: "serverURL")?.trimmingCharacters(in: .whitespacesAndNewlines)
        let usable = saved.flatMap { value -> String? in
            guard value != "https://localhost:8443", let url = URLComponents(string: value),
                  let host = url.host, !host.isEmpty, url.user == nil, url.password == nil,
                  url.query == nil, url.fragment == nil,
                  url.scheme?.lowercased() == "https" || (url.scheme?.lowercased() == "http" && ["localhost", "127.0.0.1", "::1"].contains(host.lowercased())) else { return nil }
            return value
        }
        let server = usable ?? productionURL
        // The old demoMode default was on; only the new explicit setting enables sample data.
        let sample = defaults.bool(forKey: "developerSampleData")
        return AppPreferences(serverURL: server, sampleData: sample,
                              developerOptions: defaults.bool(forKey: "developerOptions") || server != productionURL || sample)
    }
}

enum PairingLink {
    static func code(from text: String) -> String? {
        let input = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let normalized = input.uppercased()
        if normalized.range(of: "^KY-[A-Z0-9]{4}$", options: .regularExpression) != nil { return normalized }
        guard let components = URLComponents(string: input),
              components.user == nil, components.password == nil, components.port == nil,
              components.query == nil, components.fragment == nil,
              components.percentEncodedPath == components.path else { return nil }
        let path: String
        if components.scheme?.lowercased() == "keyala", components.host?.lowercased() == "connect" {
            path = components.path
        } else if serverURL(from: input) != nil, components.path.hasPrefix("/connect/") {
            path = String(components.path.dropFirst("/connect".count))
        } else { return nil }
        guard path.uppercased().range(of: "^/KY-[A-Z0-9]{4}$", options: .regularExpression) != nil else { return nil }
        return String(path.dropFirst()).uppercased()
    }

    static func serverURL(from text: String) -> String? {
        guard let url = URLComponents(string: text), url.scheme?.lowercased() == "https",
              url.user == nil, url.password == nil, url.port == nil, let host = url.host?.lowercased() else { return nil }
        let server = "https://" + host
        return [AppPreferences.productionURL, AppPreferences.stagingURL].contains(server) ? server : nil
    }
}


struct KeyEnrollment: Decodable {
    let id: String
    let vendor: Provider
    let status: String
    let last4: String?
    let currentLast4: String?
    let expiresAt: String

    enum CodingKeys: String, CodingKey {
        case id, vendor, status, last4
        case currentLast4 = "current_last4", expiresAt = "expires_at"
    }

    var isExpired: Bool {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let fractional = formatter.date(from: expiresAt)
        formatter.formatOptions = [.withInternetDateTime]
        guard let date = fractional ?? formatter.date(from: expiresAt) else { return true }
        return date <= Date()
    }
    var canClaim: Bool { status == "pending" && !isExpired }
    var canConfirm: Bool { status == "submitted" && last4?.count == 4 && !isExpired }
    var shouldPoll: Bool { status == "claimed" && !isExpired }
}

enum EnrollmentLink {
    static func id(from text: String) -> String? {
        guard let url = URLComponents(string: text.trimmingCharacters(in: .whitespacesAndNewlines)),
              url.user == nil, url.password == nil, url.port == nil, url.query == nil, url.fragment == nil,
              url.percentEncodedPath == url.path else { return nil }
        let path: String
        if url.scheme?.lowercased() == "keyala", url.host?.lowercased() == "enroll" {
            path = url.path
        } else if PairingLink.serverURL(from: text) != nil, url.path.hasPrefix("/enroll/") {
            path = String(url.path.dropFirst("/enroll".count))
        } else { return nil }
        guard path.range(of: "^/en_[A-Za-z0-9_-]{22}$", options: .regularExpression) != nil else { return nil }
        return String(path.dropFirst())
    }
}

struct OpenRouterAuthorization: Decodable {
    let flowID: String
    let authURL: URL
    let expiresAt: String
    enum CodingKeys: String, CodingKey { case flowID = "flow_id", authURL = "auth_url", expiresAt = "expires_at" }

    func validatedURL(callbackURL: URL, allowMock: Bool) throws -> URL {
        guard flowID.range(of: "^of_[A-Za-z0-9_-]{22}$", options: .regularExpression) != nil,
              let url = URLComponents(url: authURL, resolvingAgainstBaseURL: false),
              url.user == nil, url.password == nil, url.fragment == nil,
              let items = url.queryItems, items.allSatisfy({ $0.value != nil }),
              Set(items.map(\.name)).count == items.count else { throw APIError.invalidResponse }
        let query = Dictionary(uniqueKeysWithValues: items.map { ($0.name, $0.value!) })
        guard let state = query["state"], state.range(of: "^[A-Za-z0-9_-]{22}$", options: .regularExpression) != nil else { throw APIError.invalidResponse }
        if url.scheme == "https", url.host == "openrouter.ai", url.port == nil, url.path == "/auth" {
            guard query["callback_url"] == callbackURL.absoluteString,
                  query["code_challenge_method"] == "S256",
                  query["code_challenge"]?.range(of: "^[A-Za-z0-9_-]{43}$", options: .regularExpression) != nil,
                  Set(query.keys).isSubset(of: ["callback_url", "code_challenge", "code_challenge_method", "state", "key_label", "limit", "expiration"]) else { throw APIError.invalidResponse }
            return authURL
        }
        guard allowMock, url.scheme == callbackURL.scheme, url.host == callbackURL.host, url.port == callbackURL.port,
              url.path == callbackURL.deletingLastPathComponent().appendingPathComponent("mock").path,
              Set(query.keys) == ["state"] else { throw APIError.invalidResponse }
        return authURL
    }

}

struct OpenRouterFlow: Decodable {
    let flowID: String
    let provider: Provider
    let status: String
    let reason: String?
    let lastFour: String?
    enum CodingKeys: String, CodingKey { case flowID = "flow_id", provider, status, reason; case lastFour = "last4" }

    func confirmedKey(in keys: [KeyRecord], expectedFlowID: String) -> KeyRecord? {
        guard flowID == expectedFlowID, provider == .openRouter, status == "saved", let lastFour else { return nil }
        return keys.first(where: { $0.provider == .openRouter && $0.lastFour == lastFour })
    }
}

enum OpenRouterCallback {
    static func validate(_ url: URL, expectedFlowID: String) throws {
        guard let parts = URLComponents(url: url, resolvingAgainstBaseURL: false),
              parts.scheme == "keyala", parts.host == "oauth", parts.path == "/openrouter",
              parts.percentEncodedPath == parts.path, parts.user == nil, parts.password == nil,
              parts.port == nil, parts.fragment == nil, let items = parts.queryItems,
              items.allSatisfy({ $0.value != nil }), Set(items.map(\.name)).count == items.count else { throw APIError.invalidResponse }
        let query = Dictionary(uniqueKeysWithValues: items.map { ($0.name, $0.value!) })
        guard Set(query.keys).isSubset(of: ["status", "reason", "flow"]),
              (query["status"] == "ok" || query["status"] == "error"),
              query["flow"] == expectedFlowID || (query["flow"] == nil && query["status"] == "error") else { throw APIError.invalidResponse }
    }
}

struct RequestMessage: Identifiable {
    let id: Int
    let role: String
    let text: String
}

extension RequestRecord {
    var canApproveWithSwipe: Bool {
        status == "pending" && payload != nil && (0...50_000).contains(estimatedCostMicrousd)
    }
    var readableMessages: [RequestMessage] {
        guard case .object(let body)? = payload else { return [] }
        func content(_ value: JSONValue) -> String {
            if case .string(let text) = value { return text }
            if case .array(let parts) = value {
                return parts.map { part in
                    if case .object(let block) = part, case .string(let text)? = block["text"] { return text }
                    return part.prettyPrinted
                }.joined(separator: "\n\n")
            }
            return value.prettyPrinted
        }
        var messages: [RequestMessage] = []
        if let system = body["system"] { messages.append(.init(id: messages.count, role: "System", text: content(system))) }
        if case .array(let rows)? = body["messages"] {
            for row in rows {
                guard case .object(let message) = row else {
                    messages.append(.init(id: messages.count, role: "Message", text: row.prettyPrinted)); continue
                }
                var role = "Message"
                if case .string(let value)? = message["role"] { role = value.capitalized }
                if case .string(let name)? = message["name"] { role += " / " + name }
                messages.append(.init(id: messages.count, role: role, text: message["content"].map(content) ?? row.prettyPrinted))
            }
        }
        return messages
    }
}

enum SupportContact {
    static let email = "josephchen637@gmail.com"
    static let mailURL = URL(string: "mailto:josephchen637@gmail.com?subject=Keyala%20support")!
}

enum AppLanguage: String, CaseIterable, Identifiable {
    case system, english = "en", traditionalChinese = "zh-Hant", japanese = "ja"
    var id: String { rawValue }
    var title: String {
        switch self {
        case .system: localized("Follow system")
        case .english: "English"
        case .traditionalChinese: "繁體中文"
        case .japanese: "日本語"
        }
    }
    static var selected: Self {
        Self(rawValue: UserDefaults.standard.string(forKey: "appLanguage") ?? "system") ?? .system
    }
    var localization: String {
        self == .system
            ? Bundle.preferredLocalizations(from: ["en", "zh-Hant", "ja"], forPreferences: Locale.preferredLanguages).first ?? "en"
            : rawValue
    }
    var locale: Locale { self == .system ? .current : Locale(identifier: rawValue) }
}

func localized(_ key: String) -> String {
    guard let path = Bundle.main.path(forResource: AppLanguage.selected.localization, ofType: "lproj"),
          let bundle = Bundle(path: path) else { return key }
    return bundle.localizedString(forKey: key, value: key, table: nil)
}

func localized(_ key: String, locale: Locale) -> String {
    let language = Bundle.preferredLocalizations(from: ["en", "zh-Hant", "ja"], forPreferences: [locale.identifier]).first ?? "en"
    guard let path = Bundle.main.path(forResource: language, ofType: "lproj"), let bundle = Bundle(path: path) else { return key }
    return bundle.localizedString(forKey: key, value: key, table: nil)
}
