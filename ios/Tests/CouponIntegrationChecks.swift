import Foundation

@main enum CouponIntegrationChecks {
    static func main() async throws {
        let base = CommandLine.arguments.dropFirst().first ?? "http://127.0.0.1:8096"
        guard let url = URL(string: base), url.scheme == "http", url.host == "127.0.0.1",
              url.user == nil, url.password == nil, url.path.isEmpty, url.query == nil, url.fragment == nil,
              ProcessInfo.processInfo.environment["KEYALA_VENDOR_MOCK"] == "1" else {
            fatalError("Use a disposable loopback server with KEYALA_VENDOR_MOCK=1")
        }

        let issuer = try await testAccount(base)
        let recipient = try await testAccount(base)
        let app = try await wire(base, "/v1/me/apps", token: issuer.token!, body: ["name": "Native Coupon QA"])
        let appID = app["app_id"] as! String
        let appSecret = app["app_secret"] as! String
        let batch = try await wire(base, "/v1/me/apps/\(appID)/coupons", token: issuer.token!, body: [
            "vendor": "openai", "models": ["gpt-4o-mini"], "api_key": "sk-fake-native-coupon-check-only",
            "per_coupon_usd": 1, "total_usd": 2,
            "claim_deadline": ISO8601DateFormatter().string(from: Date().addingTimeInterval(3600)),
        ])
        let code = batch["coupon_code"] as! String
        let batchID = batch["id"] as! String
        precondition(CouponLink.code(from: "keyala://coupon/" + code) == code)
        let offer = try await recipient.couponPreview(code: code)
        precondition(offer.claimable && offer.coupon == nil && offer.app.id == appID)
        let claimed = try await recipient.claimCoupon(code: code)
        let retried = try await recipient.claimCoupon(code: code)
        let listed = try await recipient.coupons()
        precondition(claimed.id == retried.id && claimed.status == .active && claimed.remainingUSD == 1)
        precondition(listed.map(\.id) == [claimed.id])

        let spender = try await testAccount(base)
        let keys = try await spender.keys()
        precondition(keys.isEmpty)
        let pending = try await wire(base, "/v1/connect", token: appSecret, body: ["vendor": "openai", "coupon_code": code])
        let pairingCode = pending["connect_code"] as! String
        let pairing = try await spender.connectPreview(code: pairingCode)
        precondition(pairing.requiresAsk == true && pairing.coupon?.batchID == batchID && pairing.coupon?.coupon == nil)
        try await spender.connectDecision(code: pairingCode, approve: true, policy: .once, billingVendor: pairing.keyProvider)
        let paired = try await wire(base, "/v1/connect/\(pairingCode)", token: appSecret)
        let grantToken = paired["grant_token"] as! String
        let grants = try await spender.grants()
        let grant = grants.first!
        let funded = grant.coupon!
        precondition(grants.count == 1 && !grant.trust && funded.batchID == batchID && funded.id != claimed.id)

        let requestPath = "/v1/openai/chat/completions"
        let payload: [String: Any] = [
            "model": "gpt-4o-mini", "max_tokens": 16,
            "messages": [["role": "system", "content": "Fictional system prompt"], ["role": "user", "content": "Fictional user prompt"]],
        ]
        let queued = try await wire(base, requestPath, token: grantToken, body: payload)
        let requestID = queued["request_id"] as! String
        let inbox = try await spender.requests(status: "pending")
        precondition(inbox.first?.id == requestID && inbox.first?.coupon?.id == funded.id && inbox.first?.payload == nil)
        let request = try await spender.request(id: requestID)
        precondition(request.status == "pending" && request.coupon?.id == funded.id && !request.canApproveWithSwipe)
        precondition(request.readableMessages.map(\.text) == ["Fictional system prompt", "Fictional user prompt"])
        do {
            _ = try await spender.decide(id: requestID, approve: true, policy: Policy(mode: "trust"))
            preconditionFailure("Sponsored calls must reject widening")
        } catch APIError.coupon(let message) {
            precondition(message == APIError.couponMessage(for: "coupon_policy_required"))
        }
        _ = try await spender.decide(id: requestID, approve: true, policy: .once)
        var receipt = try await spender.request(id: requestID)
        for _ in 0..<30 where receipt.status != "completed" {
            try await Task.sleep(for: .milliseconds(100))
            receipt = try await spender.request(id: requestID)
        }
        precondition(receipt.status == "completed" && receipt.actualCostMicrousd != nil && receipt.coupon?.id == funded.id)

        _ = try await spender.addKey(provider: .openAI, value: "sk-fake-personal-key-for-fallback-check", label: "QA")
        _ = try await wire(base, "/v1/me/apps/\(appID)/coupons/\(batchID)", token: issuer.token!, method: "DELETE")
        let revoked = try await spender.coupons()
        precondition(revoked.first?.status == .revoked)
        let blocked = try await wire(base, requestPath, token: grantToken, body: payload, succeeds: false)
        precondition((blocked["error"] as? [String: Any])?["type"] as? String == "coupon_revoked")
        print("Native coupon checks passed: preview, idempotent claim, key-free sponsored pairing, full prompt review, only-once approval, mock cost, revoke, no fallback")
    }

    private static func testAccount(_ base: String) async throws -> APIClient {
        let api = APIClient(baseURL: base, allowLocalHTTP: true, token: nil)
        api.token = try await api.devLogin(email: "coupon-" + UUID().uuidString + "@example.invalid").sessionToken
        return api
    }

    private static func wire(_ base: String, _ path: String, token: String, body: [String: Any]? = nil, method: String? = nil, succeeds: Bool = true) async throws -> [String: Any] {
        var request = URLRequest(url: URL(string: base + path)!)
        request.httpMethod = method ?? (body == nil ? "GET" : "POST")
        request.setValue("Bearer " + token, forHTTPHeaderField: "Authorization")
        request.setValue("0", forHTTPHeaderField: "X-Keyala-Wait")
        request.setValue(UUID().uuidString, forHTTPHeaderField: "Idempotency-Key")
        if let body {
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        let (data, response) = try await URLSession.shared.data(for: request)
        let status = (response as! HTTPURLResponse).statusCode
        precondition((200..<300).contains(status) == succeeds, "Unexpected HTTP \(status) for \(path)")
        return try JSONSerialization.jsonObject(with: data) as! [String: Any]
    }
}
