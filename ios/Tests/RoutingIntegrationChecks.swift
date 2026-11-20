import Foundation

@main struct RoutingIntegrationChecks {
    static func main() async throws {
        let base = CommandLine.arguments.dropFirst().first ?? "http://127.0.0.1:8095"
        guard let url = URL(string: base), url.scheme == "http", url.host == "127.0.0.1" else { fatalError("Isolated loopback mock only") }
        let api = APIClient(baseURL: base, allowLocalHTTP: true, token: nil)
        let email = "routing-" + UUID().uuidString + "@example.com"
        let login = try await api.devLogin(email: email); api.token = login.sessionToken
        let initial = try await api.routingPreferences(); precondition(!initial.openRouterForDirectAPIs)
        _ = try await api.addKey(provider: .openRouter, value: "sk-or-v1-fake-native-routing-check-only", label: "QA")
        let enabled = try await api.updateRoutingPreferences(true); precondition(enabled.openRouterForDirectAPIs)
        let app = try await wire(base, "/v1/apps", body: ["name":"Native routing QA"])
        let secret = app["app_secret"] as! String
        for (vendor, model) in [(Provider.openAI, "gpt-4o-mini"), (.anthropic, "claude-haiku-5-5")] {
            let connection = try await wire(base, "/v1/connect", token: secret, body: ["vendor":vendor.rawValue])
            let code = connection["connect_code"] as! String
            let preview = try await api.connectPreview(code: code)
            precondition(preview.vendor == vendor && preview.keyProvider == .openRouter)
            try await api.connectDecision(code: code, approve: true, policy: .once, billingVendor: preview.keyProvider)
            let approved = try await wire(base, "/v1/connect/" + code, token: secret)
            let grant = approved["grant_token"] as! String
            let pending = try await wire(base, "/v1/" + vendor.rawValue + (vendor == .anthropic ? "/messages" : "/chat/completions"), token: grant,
                body: ["model":model, "messages":[["role":"user","content":"Local mock check"]], "max_tokens":16])
            let id = pending["request_id"] as! String
            let request = try await api.request(id: id)
            precondition(request.provider == vendor && request.billingVendor == .openRouter && request.upstreamModel != nil)
            _ = try await api.decide(id: id, approve: true)
            var completed = try await api.request(id: id)
            for _ in 0..<30 where completed.status != "completed" {
                try await Task.sleep(for: .milliseconds(100)); completed = try await api.request(id: id)
            }
            precondition(completed.status == "completed" && completed.actualCostMicrousd != nil)
            let stored = try await api.grants()
            precondition(stored.contains { $0.provider == vendor && $0.billingVendor == .openRouter })
        }
        let disabled = try await api.updateRoutingPreferences(false); precondition(!disabled.openRouterForDirectAPIs)
        let frozen = try await api.grants(); precondition(frozen.allSatisfy { $0.billingVendor == .openRouter })
        _ = try await api.deleteAccount(AccountDeletionRequest(email: email))
        print("Native routing passed: default off, server preference, OpenRouter-only OpenAI/Anthropic pairing, billing disclosure, approval, mock response and cost, frozen routes, cleanup")
    }
    static func wire(_ base: String, _ path: String, token: String? = nil, body: [String:Any]? = nil) async throws -> [String:Any] {
        var request = URLRequest(url: URL(string: base + path)!)
        if let body { request.httpMethod = "POST"; request.httpBody = try JSONSerialization.data(withJSONObject: body); request.setValue("application/json", forHTTPHeaderField:"Content-Type") }
        if let token { request.setValue("Bearer " + token, forHTTPHeaderField:"Authorization") }
        request.setValue("0", forHTTPHeaderField:"X-Keyala-Wait")
        request.setValue(UUID().uuidString, forHTTPHeaderField:"Idempotency-Key")
        let (data,response) = try await URLSession.shared.data(for: request)
        precondition((200..<300).contains((response as! HTTPURLResponse).statusCode), "QA endpoint failed: " + path)
        return try JSONSerialization.jsonObject(with:data) as! [String:Any]
    }
}
