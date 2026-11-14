import Foundation

@main
enum ContractTests {
    static func main() throws {
        let decoder = JSONDecoder()
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

        let client = APIClient(baseURL: "https://masterkey.example", allowLocalHTTP: false, token: "session")
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
}
