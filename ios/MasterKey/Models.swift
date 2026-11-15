import Foundation

enum Provider: String, Codable, CaseIterable, Identifiable {
    case openAI = "openai"
    case anthropic
    var id: String { rawValue }
    var title: String { self == .openAI ? "OpenAI" : "Anthropic" }
}

struct RequestRecord: Codable, Identifiable {
    var id: String
    var appID: String
    var appName: String
    var provider: Provider
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
    var estimatedUSD: String { String(format: "$%.6f", Double(estimatedCostMicrousd) / 1_000_000) }

    enum CodingKeys: String, CodingKey { case id, status, model, payload, response, error; case grantID = "grant_id", app, vendor, provider, createdAt = "created_at", expiresAt = "expires_at", actualUSD = "actual_usd", estimatedUSD = "estimated_usd" }
    init(id: String, appID: String, appName: String, provider: Provider, model: String, status: String, createdAt: String, expiresAt: String?, payload: JSONValue?, payloadSHA256: String? = nil, estimatedCostMicrousd: Int64, reservedCostMicrousd: Int64 = 0, actualCostMicrousd: Int64?, response: JSONValue?, error: String?, isDemo: Bool = false) {
        self.id = id; self.appID = appID; self.appName = appName; self.provider = provider; self.model = model; self.status = status; self.createdAt = createdAt; self.expiresAt = expiresAt; self.payload = payload; self.estimatedCostMicrousd = estimatedCostMicrousd; self.actualCostMicrousd = actualCostMicrousd; self.response = response; self.error = error; self.isDemo = isDemo
    }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let app = try c.decodeIfPresent(WireApp.self, forKey: .app)
        id = try c.decode(String.self, forKey: .id); appID = try app?.id ?? c.decodeIfPresent(String.self, forKey: .grantID) ?? ""
        appName = app?.name ?? "App"; provider = try c.decodeIfPresent(Provider.self, forKey: .vendor) ?? c.decode(Provider.self, forKey: .provider)
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
    enum CodingKeys: String, CodingKey { case vendor, last4, label }
    init(id: String, provider: Provider, label: String, lastFour: String) { self.id = id; self.provider = provider; self.label = label; self.lastFour = lastFour }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self); provider = try c.decode(Provider.self, forKey: .vendor)
        id = provider.rawValue; label = try c.decodeIfPresent(String.self, forKey: .label) ?? provider.title; lastFour = try c.decode(String.self, forKey: .last4)
    }
    func encode(to encoder: Encoder) throws { var c = encoder.container(keyedBy: CodingKeys.self); try c.encode(provider, forKey: .vendor); try c.encode(lastFour, forKey: .last4); try c.encode(label, forKey: .label) }
}

struct GrantRecord: Codable, Identifiable {
    var id: String
    var appID: String
    var appName: String
    var provider: Provider
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
    enum CodingKeys: String, CodingKey { case id, app, vendor, policy, createdAt = "created_at", lastUsedAt = "last_used_at", spentUSD = "spent_usd", requestCount = "request_count" }
    init(id: String, appID: String, appName: String, provider: Provider, model: String, maxRequests: Int?, usedRequests: Int, budgetMicrousd: Int64?, spentMicrousd: Int64, reservedMicrousd: Int64, expiresAt: String, trust: Bool, alertThresholdMicrousd: Int64?) {
        self.id = id; self.appID = appID; self.appName = appName; self.provider = provider; self.model = model; self.maxRequests = maxRequests; self.usedRequests = usedRequests; self.budgetMicrousd = budgetMicrousd; self.spentMicrousd = spentMicrousd; self.reservedMicrousd = reservedMicrousd; self.expiresAt = expiresAt; self.trust = trust; self.alertThresholdMicrousd = alertThresholdMicrousd
    }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self); let app = try c.decode(WireApp.self, forKey: .app); let policy = try c.decode(WirePolicy.self, forKey: .policy)
        id = try c.decode(String.self, forKey: .id); appID = app.id ?? ""; appName = app.name; provider = try c.decode(Provider.self, forKey: .vendor); model = ""
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

    var displayEmail: String? { email.hasSuffix("@apple.invalid") ? nil : email }
}

struct AuthenticatedSession: Codable {
    let sessionToken: String
    let user: AccountProfile

    enum CodingKeys: String, CodingKey { case user; case sessionToken = "session_token" }
}
