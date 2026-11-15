import Foundation

@main
enum ContractTests {
    static func main() throws {
        let decoder = JSONDecoder()
        let login = try decoder.decode(AuthenticatedSession.self, from: Data(#"{"session_token":"mk_sess_test","user":{"id":"usr_test","email":"preview@example.invalid"}}"#.utf8))
        let restored = try decoder.decode(AuthenticatedSession.self, from: JSONEncoder().encode(login))
        precondition(restored.user.id == "usr_test" && restored.user.displayEmail == "preview@example.invalid")
        precondition(AccountProfile(id: "usr_apple", email: "placeholder@apple.invalid").displayEmail == nil)
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

        let client = APIClient(baseURL: "https://masterkey.example", allowLocalHTTP: false, token: "session")
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
        print("MasterKey contract checks passed")
    }

    static func makeRequest(id: String, status: String, createdAt: String, payload: JSONValue? = nil, response: JSONValue? = nil) -> RequestRecord {
        RequestRecord(id: id, appID: "app-1", appName: "Sketchpad", provider: .openAI, model: "gpt-4.1-mini", status: status, createdAt: createdAt, expiresAt: nil, payload: payload, estimatedCostMicrousd: 28, actualCostMicrousd: nil, response: response, error: nil)
    }
}
