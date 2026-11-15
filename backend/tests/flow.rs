use masterkey_server::{build_state, origin_ok, router, send_request, AppState, Config, PublicApp};
use reqwest::{Client, Response};
use serde_json::{json, Value};
use std::{net::SocketAddr, time::Duration};

/// apple/apns defaults for struct-update syntax
fn extra() -> Config {
    Config {
        database_url: String::new(), master_key: [0; 32], dev_login: false, vendor_mock: false, public_url: String::new(), prices_verified: false,
        apple_bundle_id: "app.masterkey.ios".into(), apple_jwks_url: "http://127.0.0.1:1/none".into(), apns: None, public_apps: vec![], country_header: None,
    }
}

struct T {
    base: String,
    c: Client,
    sess: String,
    state: AppState,
}

async fn server() -> (String, Client, AppState) {
    server_with(vec![]).await
}

async fn server_with(public_apps: Vec<PublicApp>) -> (String, Client, AppState) {
    let db = std::env::temp_dir().join(format!("mk-test-{}.db", masterkey_server::rand_b64(8)));
    let cfg = Config {
        database_url: format!("sqlite://{}?mode=rwc", db.display()),
        master_key: [7u8; 32],
        dev_login: true,
        vendor_mock: true,
        public_url: "http://mk.test".into(),
        prices_verified: false,
        public_apps,
        country_header: Some("cf-ipcountry".into()),
        ..extra()
    };
    let state = build_state(cfg).await.unwrap();
    let app = router(state.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap() });
    (base, Client::new(), state)
}

async fn json_of(r: Response) -> Value {
    r.json().await.unwrap()
}

impl T {
    /// server + logged-in user with an OpenAI key
    async fn new() -> T {
        let (base, c, state) = server().await;
        let r = json_of(c.post(format!("{base}/v1/auth/dev")).json(&json!({"email": "a@b.co"})).send().await.unwrap()).await;
        let sess = r["session_token"].as_str().unwrap().to_string();
        let t = T { base, c, sess, state };
        let r = t.user_put("/v1/me/keys/openai", json!({"api_key": "sk-test-SECRET1234"})).await;
        assert_eq!(r.status(), 200);
        t
    }
    async fn user_put(&self, p: &str, b: Value) -> Response {
        self.c.put(format!("{}{p}", self.base)).bearer_auth(&self.sess).json(&b).send().await.unwrap()
    }
    async fn user_post(&self, p: &str, b: Value) -> Response {
        self.c.post(format!("{}{p}", self.base)).bearer_auth(&self.sess).json(&b).send().await.unwrap()
    }
    async fn user_get(&self, p: &str) -> Value {
        json_of(self.c.get(format!("{}{p}", self.base)).bearer_auth(&self.sess).send().await.unwrap()).await
    }
    /// register app, connect, approve with `policy`, return grant token
    async fn grant(&self, vendor: &str, policy: Value) -> String {
        let app = json_of(self.c.post(format!("{}/v1/apps", self.base)).json(&json!({"name": "Demo"})).send().await.unwrap()).await;
        let secret = app["app_secret"].as_str().unwrap();
        let conn = json_of(self.c.post(format!("{}/v1/connect", self.base)).bearer_auth(secret).json(&json!({"vendor": vendor})).send().await.unwrap()).await;
        let code = conn["connect_code"].as_str().unwrap();
        assert!(conn["connect_url"].as_str().unwrap().ends_with(&format!("/connect/{code}")));
        let poll = |c: &Client| c.get(format!("{}/v1/connect/{code}", self.base)).bearer_auth(secret).send();
        assert_eq!(json_of(poll(&self.c).await.unwrap()).await["status"], "pending");
        let prev = self.user_get(&format!("/v1/me/connect/{code}")).await;
        assert_eq!(prev["app"]["name"], "Demo");
        // Each helper-created app is new. Model a user deliberately widening the
        // first ask-only grant through the Grants screen when a test needs a broad policy.
        let approved_policy = if policy["mode"] == "ask" { policy.clone() } else { json!({"mode": "ask"}) };
        let g = json_of(self.user_post(&format!("/v1/me/connect/{code}/approve"), json!({"policy": approved_policy})).await).await;
        assert_eq!(g["vendor"], vendor);
        let p = json_of(poll(&self.c).await.unwrap()).await;
        assert_eq!(p["status"], "approved");
        let tok = p["grant_token"].as_str().unwrap().to_string();
        assert!(tok.starts_with("mk_grant_"));
        if policy["mode"] != "ask" {
            let r = self.c.patch(format!("{}/v1/me/grants/{}", self.base, g["id"].as_str().unwrap()))
                .bearer_auth(&self.sess).json(&json!({"policy": policy})).send().await.unwrap();
            assert_eq!(r.status(), 200);
        }
        // token is delivered once
        assert!(json_of(poll(&self.c).await.unwrap()).await.get("grant_token").is_none());
        tok
    }
    async fn chat(&self, tok: &str, wait: u32, body: Value) -> Response {
        self.c.post(format!("{}/v1/openai/chat/completions", self.base)).bearer_auth(tok).header("X-MasterKey-Wait", wait.to_string()).json(&body).send().await.unwrap()
    }
    async fn pending(&self) -> String {
        for _ in 0..100 {
            let l = self.user_get("/v1/me/requests?status=pending").await;
            if let Some(r) = l.as_array().unwrap().first() {
                return r["id"].as_str().unwrap().to_string();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("no pending request");
    }
    async fn wait_status(&self, id: &str, want: &str) -> Value {
        for _ in 0..100 {
            let r = self.user_get(&format!("/v1/me/requests/{id}")).await;
            if r["status"] == want {
                return r;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("request never reached {want}");
    }
}

fn msg() -> Value {
    json!({"model": "gpt-4o-mini", "messages": [{"role": "user", "content": "hi"}], "max_tokens": 50})
}

#[tokio::test]
async fn first_grant_is_ask_only_per_user_and_app_even_after_revoke() {
    let t = T::new().await;
    assert_eq!(t.user_put("/v1/me/keys/anthropic", json!({"api_key":"sk-ant-test-SECRET1234"})).await.status(), 200);
    let app = json_of(t.c.post(format!("{}/v1/apps", t.base)).json(&json!({"name": "Same app"})).send().await.unwrap()).await;
    let secret = app["app_secret"].as_str().unwrap();
    async fn new_code(t: &T, secret: &str, vendor: &str) -> String {
        let resp = t.c.post(format!("{}/v1/connect", t.base)).bearer_auth(secret)
            .json(&json!({"vendor": vendor})).send().await.unwrap();
        json_of(resp).await["connect_code"].as_str().unwrap().to_owned()
    }

    let first_code = new_code(&t, secret, "openai").await;
    let first_preview = t.user_get(&format!("/v1/me/connect/{first_code}")).await;
    assert_eq!(first_preview["requires_ask"], true);
    let rejected = t.user_post(&format!("/v1/me/connect/{first_code}/approve"), json!({"policy":{"mode":"budget","remaining_usd":2.0}})).await;
    assert_eq!(rejected.status(), 400);
    assert_eq!(json_of(rejected).await["error"]["type"], "policy_not_allowed_yet");
    // Rejection rolled back the connect status, so ask can still be approved.
    let first = json_of(t.user_post(&format!("/v1/me/connect/{first_code}/approve"), json!({"policy":{"mode":"ask"}})).await).await;
    assert_eq!(first["policy"]["mode"], "ask");
    let first_id = first["id"].as_str().unwrap();
    assert_eq!(t.c.delete(format!("{}/v1/me/grants/{first_id}", t.base)).bearer_auth(&t.sess).send().await.unwrap().status(), 204);

    // Any grant history counts, including revoked grants and grants for another vendor.
    let later_code = new_code(&t, secret, "anthropic").await;
    let later_preview = t.user_get(&format!("/v1/me/connect/{later_code}")).await;
    assert_eq!(later_preview["requires_ask"], false);
    let later = t.user_post(&format!("/v1/me/connect/{later_code}/approve"), json!({"policy":{"mode":"trust"}})).await;
    assert_eq!(later.status(), 200);

    // History is scoped to the authenticated user, even for the same app.
    let other = json_of(t.c.post(format!("{}/v1/auth/dev", t.base)).json(&json!({"email":"other@b.co"})).send().await.unwrap()).await;
    let other_session = other["session_token"].as_str().unwrap();
    let key = t.c.put(format!("{}/v1/me/keys/openai", t.base)).bearer_auth(other_session).json(&json!({"api_key":"sk-other-SECRET1234"})).send().await.unwrap();
    assert_eq!(key.status(), 200);
    let other_code = new_code(&t, secret, "openai").await;
    let preview = t.c.get(format!("{}/v1/me/connect/{other_code}", t.base)).bearer_auth(other_session).send().await.unwrap();
    let preview = json_of(preview).await;
    assert_eq!(preview["requires_ask"], true);
    let rejected = t.c.post(format!("{}/v1/me/connect/{other_code}/approve", t.base)).bearer_auth(other_session)
        .json(&json!({"policy":{"mode":"count","remaining_count":3}})).send().await.unwrap();
    assert_eq!(rejected.status(), 400);
    assert_eq!(json_of(rejected).await["error"]["type"], "policy_not_allowed_yet");
}

#[tokio::test]
async fn full_connect_flow_and_proxy_call() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    let r = t.chat(&tok, 0, msg()).await;
    assert_eq!(r.status(), 200);
    let rid = r.headers()["x-masterkey-request-id"].to_str().unwrap().to_string();
    let b = json_of(r).await;
    assert_eq!(b["object"], "chat.completion");
    assert!(b["usage"]["prompt_tokens"].as_i64().unwrap() > 0);
    let d = t.user_get(&format!("/v1/me/requests/{rid}")).await;
    assert_eq!(d["status"], "completed");
    assert!(d["actual_usd"].as_f64().unwrap() > 0.0);
    assert_eq!(d["payload"]["model"], "gpt-4o-mini");
    let grants = t.user_get("/v1/me/grants").await;
    assert_eq!(grants[0]["request_count"], 1);
    // grant poll endpoint
    let p = json_of(t.c.get(format!("{}/v1/requests/{rid}", t.base)).bearer_auth(&tok).send().await.unwrap()).await;
    assert_eq!(p["status"], "completed");
    assert_eq!(p["response"]["object"], "chat.completion");
}

#[tokio::test]
async fn ask_held_then_approved() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let (t2, tok2) = (t.base.clone(), tok.clone());
    let call = tokio::spawn(async move {
        Client::new().post(format!("{t2}/v1/openai/chat/completions")).bearer_auth(tok2).header("X-MasterKey-Wait", "10").json(&msg()).send().await.unwrap()
    });
    let id = t.pending().await;
    let detail = t.user_get(&format!("/v1/me/requests/{id}")).await;
    assert_eq!(detail["payload"]["messages"][0]["content"], "hi");
    assert!(detail["estimated_usd"].as_f64().unwrap() > 0.0);
    assert_eq!(t.user_post(&format!("/v1/me/requests/{id}/approve"), json!({})).await.status(), 200);
    let r = call.await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["x-masterkey-request-id"], id.as_str());
    assert_eq!(json_of(r).await["object"], "chat.completion");
}

#[tokio::test]
async fn deny_gives_403() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let (t2, tok2) = (t.base.clone(), tok.clone());
    let call = tokio::spawn(async move {
        Client::new().post(format!("{t2}/v1/openai/chat/completions")).bearer_auth(tok2).header("X-MasterKey-Wait", "10").json(&msg()).send().await.unwrap()
    });
    let id = t.pending().await;
    assert_eq!(t.user_post(&format!("/v1/me/requests/{id}/deny"), json!({})).await.status(), 200);
    let r = call.await.unwrap();
    assert_eq!(r.status(), 403);
    assert_eq!(json_of(r).await["error"]["type"], "masterkey_denied");
}

#[tokio::test]
async fn wait_zero_202_then_approve_completes() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let r = t.chat(&tok, 0, msg()).await;
    assert_eq!(r.status(), 202);
    let b = json_of(r).await;
    assert_eq!(b["status"], "pending");
    let id = b["request_id"].as_str().unwrap().to_string();
    t.user_post(&format!("/v1/me/requests/{id}/approve"), json!({})).await;
    let d = t.wait_status(&id, "completed").await;
    assert_eq!(d["response"]["object"], "chat.completion");
    let p = json_of(t.c.get(format!("{}/v1/requests/{id}", t.base)).bearer_auth(&tok).send().await.unwrap()).await;
    assert_eq!(p["status"], "completed");
    assert!(p["response"].is_object());
}

#[tokio::test]
async fn count_policy_auto_approves_n_then_asks() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "count", "remaining_count": 2})).await;
    assert_eq!(t.chat(&tok, 0, msg()).await.status(), 200);
    assert_eq!(t.chat(&tok, 0, msg()).await.status(), 200);
    assert_eq!(t.chat(&tok, 0, msg()).await.status(), 202);
    let g = t.user_get("/v1/me/grants").await;
    assert_eq!(g[0]["policy"]["remaining_count"], 0);
}

#[tokio::test]
async fn approve_with_count_policy_means_next_n() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let id = json_of(t.chat(&tok, 0, msg()).await).await["request_id"].as_str().unwrap().to_string();
    t.user_post(&format!("/v1/me/requests/{id}/approve"), json!({"policy": {"mode": "count", "remaining_count": 3}})).await;
    t.wait_status(&id, "completed").await;
    assert_eq!(t.chat(&tok, 0, msg()).await.status(), 200);
    assert_eq!(t.chat(&tok, 0, msg()).await.status(), 200);
    assert_eq!(t.chat(&tok, 0, msg()).await.status(), 202);
}

#[tokio::test]
async fn budget_exhausts_and_failure_refunds() {
    let t = T::new().await;
    // gpt-4o-mini, max_tokens 50: estimate is a few hundredths of a cent; budget fits ~2 calls at 1000 max_tokens
    let big = json!({"model": "gpt-4o", "messages": [{"role": "user", "content": "hi"}], "max_tokens": 1000}); // est ~ $0.010
    let tok = t.grant("openai", json!({"mode": "budget", "remaining_usd": 0.025})).await;
    assert_eq!(t.chat(&tok, 0, big.clone()).await.status(), 200);
    // reservation (~$0.010) was settled to the tiny actual cost: nearly all budget is back
    let left = t.user_get("/v1/me/grants").await[0]["policy"]["remaining_usd"].as_f64().unwrap();
    assert!(left > 0.0245 && left < 0.025, "settled to actual cost: {left}");
    // budget below the next reservation -> needs approval
    let r = t.user_get("/v1/me/grants").await;
    let gid = r[0]["id"].as_str().unwrap();
    let patch = t.c.patch(format!("{}/v1/me/grants/{gid}", t.base)).bearer_auth(&t.sess).json(&json!({"policy": {"mode": "budget", "remaining_usd": 0.005}})).send().await.unwrap();
    assert_eq!(patch.status(), 200);
    assert_eq!(t.chat(&tok, 0, big.clone()).await.status(), 202);
    // upstream failure refunds the reservation
    let t2 = T::new().await;
    let tok2 = t2.grant("openai", json!({"mode": "budget", "remaining_usd": 1.0})).await;
    let mut fail = big.clone();
    fail["messages"][0]["content"] = json!("MK_MOCK_FAIL");
    let r = t2.chat(&tok2, 0, fail).await;
    assert_eq!(r.status(), 500);
    let left = t2.user_get("/v1/me/grants").await[0]["policy"]["remaining_usd"].as_f64().unwrap();
    assert!((left - 1.0).abs() < 1e-9, "reservation refunded: {left}");
}

#[tokio::test]
async fn validation_rejections() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    let cases = [
        (json!({"model": "gpt-4o", "messages": [{"role":"user","content":"x"}], "stream": true}), "streaming_unsupported"),
        (json!({"model": "gpt-99", "messages": [{"role":"user","content":"x"}]}), "model_not_allowed"),
        (json!({"model": "gpt-4o", "messages": [{"role":"user","content":"x"}], "n": 2}), "unsupported_option"),
        (json!({"model": "gpt-4o", "messages": [{"role":"user","content":"x"}], "tools": []}), "unsupported_option"),
        (json!({"model": "gpt-4o", "messages": [{"role":"user","content":"x"}], "logit_bias": {}}), "unsupported_option"),
        (json!({"model": "gpt-4o", "messages": [{"role":"user","content":[{"type":"text","text":"x","cache_control":{}}]}]}), "unsupported_option"),
        (json!({"model": "gpt-4o", "messages": [{"role":"user","content":"x","tool_calls":[]}]}), "unsupported_option"),
        (json!({"model": "gpt-4o", "messages": [{"role":"user","content":"x"}], "response_format": {"type":"json_schema"}}), "unsupported_option"),
        (json!({"model": "gpt-4o", "messages": [{"role":"tool","content":"x"}]}), "invalid_request"),
        (json!({"model": "gpt-4o", "messages": [{"role":"user","content":[{"type":"image_url","image_url":{"url":"http://x/y.png"}}]}]}), "unsupported_content"),
        (json!({"model": "gpt-4o", "messages": [{"role":"user","content":"x"}], "max_tokens": 999999}), "max_tokens_too_large"),
    ];
    for (body, ty) in cases {
        let r = t.chat(&tok, 0, body).await;
        assert_eq!(r.status(), 400, "{ty}");
        assert_eq!(json_of(r).await["error"]["type"], ty);
    }
    // missing max_tokens gets the default injected and the call works
    let r = t.chat(&tok, 0, json!({"model": "gpt-4o", "messages": [{"role":"user","content":"x"}]})).await;
    assert_eq!(r.status(), 200);
    let id = r.headers()["x-masterkey-request-id"].to_str().unwrap().to_string();
    assert_eq!(t.user_get(&format!("/v1/me/requests/{id}")).await["payload"]["max_tokens"], 1024);
}

#[tokio::test]
async fn keys_never_returned() {
    let t = T::new().await;
    let list = t.c.get(format!("{}/v1/me/keys", t.base)).bearer_auth(&t.sess).send().await.unwrap().text().await.unwrap();
    assert!(list.contains("\"last4\":\"1234\""));
    assert!(!list.contains("SECRET") && !list.contains("sk-test"));
    let put = t.user_put("/v1/me/keys/openai", json!({"api_key": "sk-test-OTHER5678"})).await.text().await.unwrap();
    assert!(!put.contains("OTHER") && put.contains("5678"));
    // also absent from request detail / grants after a call
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    let r = t.chat(&tok, 0, msg()).await.text().await.unwrap();
    assert!(!r.contains("OTHER"));
}

#[tokio::test]
async fn grant_cannot_cross_vendors() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    let r = t.c.post(format!("{}/v1/anthropic/messages", t.base)).bearer_auth(&tok)
        .json(&json!({"model": "claude-haiku-5-5", "max_tokens": 10, "messages": [{"role":"user","content":"x"}]})).send().await.unwrap();
    assert_eq!(r.status(), 403);
    assert_eq!(json_of(r).await["error"]["type"], "vendor_mismatch");
}

#[tokio::test]
async fn anthropic_mock_shape() {
    let t = T::new().await;
    t.user_put("/v1/me/keys/anthropic", json!({"api_key": "sk-ant-test-0000"})).await;
    let tok = t.grant("anthropic", json!({"mode": "trust"})).await;
    let r = t.c.post(format!("{}/v1/anthropic/messages", t.base)).bearer_auth(&tok)
        .json(&json!({"model": "claude-haiku-5-5", "system": "be brief", "messages": [{"role":"user","content":[{"type":"text","text":"hi"}]}]})).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let b = json_of(r).await;
    assert_eq!(b["type"], "message");
    assert!(b["usage"]["input_tokens"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn double_approve_sends_once() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let id = json_of(t.chat(&tok, 0, msg()).await).await["request_id"].as_str().unwrap().to_string();
    let path = format!("/v1/me/requests/{id}/approve");
    let (a, b) = tokio::join!(t.user_post(&path, json!({})), t.user_post(&path, json!({})));
    let mut codes = [a.status().as_u16(), b.status().as_u16()];
    codes.sort();
    assert_eq!(codes, [200, 409]);
    t.wait_status(&id, "completed").await;
    assert_eq!(t.user_post(&path, json!({})).await.status(), 409);
    assert_eq!(t.user_get("/v1/me/grants").await[0]["request_count"], 1);
}

#[tokio::test]
async fn misc_auth_and_pages() {
    let t = T::new().await;
    assert_eq!(t.c.get(format!("{}/healthz", t.base)).send().await.unwrap().text().await.unwrap(), "ok");
    let r = t.c.get(format!("{}/v1/me/keys", t.base)).send().await.unwrap();
    assert_eq!(r.status(), 401);
    assert_eq!(json_of(r).await["error"]["type"], "unauthorized");
    let page = t.c.get(format!("{}/connect/MK-ABCD", t.base)).send().await.unwrap().text().await.unwrap();
    assert!(page.contains("masterkey://connect/MK-ABCD") && page.contains("Open in the MasterKey app"));
    assert_eq!(t.c.get(format!("{}/connect/<script>", t.base)).send().await.unwrap().status(), 404);
    assert_eq!(t.user_post("/v1/me/devices", json!({"apns_token": "abc"})).await.status(), 204);
    // app registration is rate limited
    let mut last = 0;
    for _ in 0..11 {
        last = t.c.post(format!("{}/v1/apps", t.base)).json(&json!({"name": "x"})).send().await.unwrap().status().as_u16();
    }
    assert_eq!(last, 429);
}

#[tokio::test]
async fn sse_events_filtered_by_user() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let mut ev = t.c.get(format!("{}/v1/me/events", t.base)).bearer_auth(&t.sess).send().await.unwrap();
    assert_eq!(ev.status(), 200);
    t.chat(&tok, 0, msg()).await;
    let chunk = tokio::time::timeout(Duration::from_secs(5), ev.chunk()).await.unwrap().unwrap().unwrap();
    let s = String::from_utf8_lossy(&chunk);
    assert!(s.contains("event: request.created"), "{s}");
}

#[tokio::test]
async fn request_history_cursor_is_stable_and_owner_scoped() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    for _ in 0..5 {
        assert_eq!(t.chat(&tok, 0, msg()).await.status(), 200);
    }
    let ask_tok = t.grant("openai", json!({"mode": "ask"})).await;
    assert_eq!(t.chat(&ask_tok, 0, msg()).await.status(), 202);

    // Force a timestamp tie so the ID tie-breaker is exercised on every page.
    let uid: String = sqlx::query_scalar("SELECT user_id FROM sessions WHERE token_hash=?")
        .bind(masterkey_server::hash_token(&t.sess)).fetch_one(&t.state.db).await.unwrap();
    sqlx::query("UPDATE requests SET created_at='2026-11-14T00:00:00.000Z' WHERE user_id=?")
        .bind(&uid).execute(&t.state.db).await.unwrap();

    let all = t.user_get("/v1/me/requests?status=completed&limit=200").await;
    let expected: Vec<String> = all.as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap().to_string()).collect();
    assert_eq!(expected.len(), 5);
    let mut found = Vec::new();
    let mut before = None;
    loop {
        let path = match &before {
            Some(id) => format!("/v1/me/requests?status=completed&limit=2&before_id={id}"),
            None => "/v1/me/requests?status=completed&limit=2".to_string(),
        };
        let page = t.user_get(&path).await;
        let rows = page.as_array().unwrap();
        assert!(rows.len() <= 2);
        for row in rows {
            assert!(row.get("payload").is_none());
            assert!(row.get("response").is_none());
            found.push(row["id"].as_str().unwrap().to_string());
        }
        if rows.len() < 2 { break; }
        before = Some(rows.last().unwrap()["id"].as_str().unwrap().to_string());
    }
    assert_eq!(found, expected, "pages must cover the ordered history without gaps or duplicates");

    // A cursor is valid only for the authenticated owner; missing and foreign IDs are both 404.
    let second = json_of(t.c.post(format!("{}/v1/auth/dev", t.base)).json(&json!({"email":"other@b.co"})).send().await.unwrap()).await;
    let other_sess = second["session_token"].as_str().unwrap();
    let foreign = t.c.get(format!("{}/v1/me/requests?before_id={}", t.base, expected[0]))
        .bearer_auth(other_sess).send().await.unwrap();
    assert_eq!(foreign.status(), 404);
    let missing = t.c.get(format!("{}/v1/me/requests?before_id=missing-request", t.base))
        .bearer_auth(&t.sess).send().await.unwrap();
    assert_eq!(missing.status(), 404);
}

async fn user_b(t: &T) -> String {
    let r = json_of(t.c.post(format!("{}/v1/auth/dev", t.base)).json(&json!({"email": "b@b.co"})).send().await.unwrap()).await;
    r["session_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn auth_isolation() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let tok2 = t.grant("openai", json!({"mode": "ask"})).await;
    let id = json_of(t.chat(&tok, 0, msg()).await).await["request_id"].as_str().unwrap().to_string();
    let gid = t.user_get("/v1/me/grants").await[0]["id"].as_str().unwrap().to_string();
    let b = user_b(&t).await;
    let bget = |p: String| t.c.get(format!("{}{p}", t.base)).bearer_auth(&b).send();
    assert_eq!(bget("/v1/me/requests".into()).await.unwrap().json::<Value>().await.unwrap(), json!([]));
    assert_eq!(bget("/v1/me/grants".into()).await.unwrap().json::<Value>().await.unwrap(), json!([]));
    assert_eq!(bget(format!("/v1/me/requests/{id}")).await.unwrap().status(), 404);
    for act in ["approve", "deny"] {
        let r = t.c.post(format!("{}/v1/me/requests/{id}/{act}", t.base)).bearer_auth(&b).json(&json!({})).send().await.unwrap();
        assert_eq!(r.status(), 404);
    }
    assert_eq!(t.c.patch(format!("{}/v1/me/grants/{gid}", t.base)).bearer_auth(&b).json(&json!({"policy":{"mode":"trust"}})).send().await.unwrap().status(), 404);
    assert_eq!(t.c.delete(format!("{}/v1/me/grants/{gid}", t.base)).bearer_auth(&b).send().await.unwrap().status(), 404);
    // still pending for A
    assert_eq!(t.user_get(&format!("/v1/me/requests/{id}")).await["status"], "pending");
    // another grant cannot poll this request
    assert_eq!(t.c.get(format!("{}/v1/requests/{id}", t.base)).bearer_auth(&tok2).send().await.unwrap().status(), 404);
    // token kinds are not interchangeable
    assert_eq!(t.chat(&t.sess, 0, msg()).await.status(), 401);
    assert_eq!(t.c.get(format!("{}/v1/me/keys", t.base)).bearer_auth(&tok).send().await.unwrap().status(), 401);
    // x-api-key is only for anthropic proxy routes
    let r = t.c.post(format!("{}/v1/openai/chat/completions", t.base)).header("x-api-key", &tok).json(&msg()).send().await.unwrap();
    assert_eq!(r.status(), 401);
}

#[tokio::test]
async fn revoke_denies_pending() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let id = json_of(t.chat(&tok, 0, msg()).await).await["request_id"].as_str().unwrap().to_string();
    let gid = t.user_get("/v1/me/grants").await[0]["id"].as_str().unwrap().to_string();
    assert_eq!(t.c.delete(format!("{}/v1/me/grants/{gid}", t.base)).bearer_auth(&t.sess).send().await.unwrap().status(), 204);
    assert_eq!(t.user_get(&format!("/v1/me/requests/{id}")).await["status"], "denied");
    assert_eq!(t.chat(&tok, 0, msg()).await.status(), 401);
}

#[tokio::test]
async fn deleted_key_fails_request_and_refunds() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let id = json_of(t.chat(&tok, 0, msg()).await).await["request_id"].as_str().unwrap().to_string();
    t.c.delete(format!("{}/v1/me/keys/openai", t.base)).bearer_auth(&t.sess).send().await.unwrap();
    t.user_post(&format!("/v1/me/requests/{id}/approve"), json!({"policy": {"mode": "budget", "remaining_usd": 1.0}})).await;
    let d = t.wait_status(&id, "failed").await;
    assert_eq!(d["response"]["error"]["type"], "no_key");
    let left = t.user_get("/v1/me/grants").await[0]["policy"]["remaining_usd"].as_f64().unwrap();
    assert!((left - 1.0).abs() < 1e-9, "{left}");
}

#[tokio::test]
async fn replaced_key_is_used_at_send_time() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let id = json_of(t.chat(&tok, 0, msg()).await).await["request_id"].as_str().unwrap().to_string();
    t.user_put("/v1/me/keys/openai", json!({"api_key": "sk-test-NEWKEY9999"})).await;
    t.user_post(&format!("/v1/me/requests/{id}/approve"), json!({})).await;
    assert_eq!(t.wait_status(&id, "completed").await["response"]["mock_key_last4"], "9999");
}

#[tokio::test]
async fn approve_needs_sufficient_allowance() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let big = json!({"model": "gpt-4o", "messages": [{"role": "user", "content": "hi"}], "max_tokens": 1000}); // est ~ $0.010
    let id = json_of(t.chat(&tok, 0, big).await).await["request_id"].as_str().unwrap().to_string();
    let path = format!("/v1/me/requests/{id}/approve");
    for pol in [json!({"mode": "budget", "remaining_usd": 0.001}), json!({"mode": "count", "remaining_count": 0})] {
        let r = t.user_post(&path, json!({"policy": pol})).await;
        assert_eq!(r.status(), 400);
        assert_eq!(json_of(r).await["error"]["type"], "insufficient_allowance");
        assert_eq!(t.user_get(&format!("/v1/me/requests/{id}")).await["status"], "pending");
    }
    assert_eq!(t.user_get("/v1/me/grants").await[0]["policy"]["mode"], "ask");
    assert_eq!(t.user_post(&path, json!({"policy": {"mode": "budget", "remaining_usd": 0.05}})).await.status(), 200);
}

#[tokio::test]
async fn policy_validation() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let _ = tok;
    let gid = t.user_get("/v1/me/grants").await[0]["id"].as_str().unwrap().to_string();
    for pol in [
        json!({"mode": "budget", "remaining_usd": -1.0}),
        json!({"mode": "budget", "remaining_usd": 1e300}),
        json!({"mode": "budget", "remaining_usd": 10001.0}),
        json!({"mode": "count", "remaining_count": -5}),
        json!({"mode": "count", "remaining_count": 9_000_000_000_000_i64}),
        json!({"mode": "trust", "alert_usd_per_day": -0.5}),
        json!({"mode": "trust", "alert_usd_per_day": 1e30}),
        json!({"mode": "ask", "expires_at": "tomorrow"}),
    ] {
        let r = t.c.patch(format!("{}/v1/me/grants/{gid}", t.base)).bearer_auth(&t.sess).json(&json!({"policy": pol})).send().await.unwrap();
        assert_eq!(r.status(), 400, "{pol}");
    }
}

#[tokio::test]
async fn verified_prices_work_outside_mock_and_haiku_long_prompt_tier() {
    use masterkey_server::pricing::{actual_cost, lookup, validate};
    let mut body = json!({"model": "gpt-4o", "messages": [{"role": "user", "content": "x"}]});
    assert!(validate("openai", &mut body, false).is_ok(), "verified models must not need MK_PRICES_VERIFIED");
    let h = lookup("anthropic", "claude-haiku-5-5").unwrap();
    // 100K input tokens: short tier $0.10 -> 10_000 micro; 100_001: long tier $0.50 -> 50_001 micro (ceil).
    assert_eq!(actual_cost("anthropic", h, &json!({"usage": {"input_tokens": 100_000, "output_tokens": 0}})), Some(10_000));
    assert_eq!(actual_cost("anthropic", h, &json!({"usage": {"input_tokens": 100_001, "output_tokens": 0}})), Some(50_001));
    assert_eq!(actual_cost("anthropic", h, &json!({"usage": {"input_tokens": 200_000, "output_tokens": 1_000_000}})), Some(100_000 + 2_500_000));
}

/// helper: make a pending request on an ask grant and force it to `approved` in the DB (simulates a
/// queued send), returning (request id, grant id).
async fn queued(t: &T, tok: &str) -> (String, String) {
    let id = json_of(t.chat(tok, 0, msg()).await).await["request_id"].as_str().unwrap().to_string();
    let gid = t.user_get("/v1/me/grants").await[0]["id"].as_str().unwrap().to_string();
    sqlx_exec(t, &format!("UPDATE requests SET status='approved', reserved_micro=5000 WHERE id='{id}'")).await;
    (id, gid)
}
async fn sqlx_exec(t: &T, q: &str) {
    // AppState exposes the pool
    sqlx_query(&t.state, q).await;
}
async fn sqlx_query(s: &AppState, q: &str) {
    s.db.execute(q).await.unwrap();
}
use sqlx::Executor;

#[tokio::test]
async fn revoked_grant_cannot_send_and_refunds() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let (id, gid) = queued(&t, &tok).await;
    t.c.patch(format!("{}/v1/me/grants/{gid}", t.base)).bearer_auth(&t.sess).json(&json!({"policy": {"mode": "budget", "remaining_usd": 0.5}})).send().await.unwrap();
    sqlx_exec(&t, &format!("UPDATE requests SET policy_gen=(SELECT policy_gen FROM grants WHERE id='{gid}') WHERE id='{id}'")).await;
    assert_eq!(t.c.delete(format!("{}/v1/me/grants/{gid}", t.base)).bearer_auth(&t.sess).send().await.unwrap().status(), 204);
    send_request(t.state.clone(), id.clone()).await;
    assert_eq!(t.user_get(&format!("/v1/me/requests/{id}")).await["status"], "denied");
    let rem: i64 = sqlx::query_scalar("SELECT remaining_micro FROM grants WHERE id=?").bind(&gid).fetch_one(&t.state.db).await.unwrap();
    assert_eq!(rem, 505_000);
}

#[tokio::test]
async fn settlement_ignores_replaced_policy() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let (id, gid) = queued(&t, &tok).await; // request reserved under the old policy generation
    t.c.patch(format!("{}/v1/me/grants/{gid}", t.base)).bearer_auth(&t.sess).json(&json!({"policy": {"mode": "budget", "remaining_usd": 0.5}})).send().await.unwrap();
    send_request(t.state.clone(), id.clone()).await;
    assert_eq!(t.user_get(&format!("/v1/me/requests/{id}")).await["status"], "completed");
    let rem: i64 = sqlx::query_scalar("SELECT remaining_micro FROM grants WHERE id=?").bind(&gid).fetch_one(&t.state.db).await.unwrap();
    assert_eq!(rem, 500_000, "new allowance untouched by the old reservation's settlement");
}

#[tokio::test]
async fn startup_sends_approved_and_marks_sending_uncertain() {
    let db = std::env::temp_dir().join(format!("mk-test-{}.db", masterkey_server::rand_b64(8)));
    let cfg = || Config {
        database_url: format!("sqlite://{}?mode=rwc", db.display()),
        master_key: [7u8; 32], dev_login: true, vendor_mock: true, public_url: "http://x".into(), prices_verified: false, ..extra()
    };
    let s1 = build_state(cfg()).await.unwrap();
    let r = |email: &str| format!("INSERT INTO users (id,email,created_at) VALUES ('u','{email}','t')");
    s1.db.execute(r("a@b.co").as_str()).await.unwrap();
    for q in [
        "INSERT INTO apps (id,name,secret_hash,created_at) VALUES ('a','A','h','t')",
        "INSERT INTO grants (id,user_id,app_id,vendor,mode,created_at) VALUES ('g','u','a','openai','trust','t')",
    ] { s1.db.execute(q).await.unwrap(); }
    let (ct, nonce) = masterkey_server::encrypt(&[7u8; 32], &masterkey_server::key_aad("u", "openai"), "sk-test-ABCD1234");
    sqlx::query("INSERT INTO api_keys (user_id,vendor,ciphertext,nonce,last4,created_at) VALUES ('u','openai',?,?,'1234','t')").bind(ct).bind(nonce).execute(&s1.db).await.unwrap();
    for (id, st) in [("r1", "approved"), ("r2", "sending")] {
        sqlx::query("INSERT INTO requests (id,grant_id,user_id,app_id,vendor,model,endpoint,status,estimated_micro,payload,created_at,expires_at) VALUES (?, 'g','u','a','openai','gpt-4o-mini','/x',?,10,?,'t','9999')")
            .bind(id).bind(st).bind(msg().to_string()).execute(&s1.db).await.unwrap();
    }
    let s2 = build_state(cfg()).await.unwrap();
    for _ in 0..100 {
        let st: String = sqlx::query_scalar("SELECT status FROM requests WHERE id='r1'").fetch_one(&s2.db).await.unwrap();
        if st == "completed" { break; }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let st = |id: &'static str| sqlx::query_scalar::<_, String>("SELECT status FROM requests WHERE id=?").bind(id).fetch_one(&s2.db);
    assert_eq!(st("r1").await.unwrap(), "completed");
    assert_eq!(st("r2").await.unwrap(), "uncertain");
}

#[tokio::test]
async fn anthropic_x_api_key_and_sdk_path() {
    let t = T::new().await;
    t.user_put("/v1/me/keys/anthropic", json!({"api_key": "sk-ant-test-0000"})).await;
    let tok = t.grant("anthropic", json!({"mode": "trust"})).await;
    let body = json!({"model": "claude-haiku-5-5", "max_tokens": 10, "messages": [{"role":"user","content":"x"}]});
    for path in ["/v1/anthropic/messages", "/v1/anthropic/v1/messages"] {
        let r = t.c.post(format!("{}{path}", t.base)).header("x-api-key", &tok).json(&body).send().await.unwrap();
        assert_eq!(r.status(), 200, "{path}");
    }
}

#[tokio::test]
async fn missing_usage_keeps_actual_null_and_charges_estimate() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "budget", "remaining_usd": 1.0})).await;
    let body = json!({"model": "gpt-4o", "messages": [{"role": "user", "content": "MK_MOCK_NOUSAGE"}], "max_tokens": 1000});
    let r = t.chat(&tok, 0, body).await;
    assert_eq!(r.status(), 200);
    let id = r.headers()["x-masterkey-request-id"].to_str().unwrap().to_string();
    let d = t.user_get(&format!("/v1/me/requests/{id}")).await;
    assert!(d["actual_usd"].is_null());
    let est = d["estimated_usd"].as_f64().unwrap();
    let g = t.user_get("/v1/me/grants").await;
    assert!((g[0]["spent_usd"].as_f64().unwrap() - est).abs() < 1e-9);
    assert!((g[0]["policy"]["remaining_usd"].as_f64().unwrap() - (1.0 - est)).abs() < 1e-9);
}

// ---------- Sign in with Apple ----------
const RSA_PEM: &str = include_str!("fixtures/test_rsa_private.pem");
const RSA_N: &str = "4THxC83HyIW-SGwL1LK6E4ZjnFFjoBoYqWXnPOOXxmjYyUvEmxqb68lVvcbz0K-fpdTLTQCL6dSZkuutLYwzgMMwL1-9Z0HJHjnWTVk-Xq4WtAiZ-Gmeg-gj8n4C9DgpnRN0VOkOqXFMxWnQGiRdaH5AL45Yvyab0ENumIO4Hc6alaSfuEozHGI3uTbtcP2u2_Bad67DH5fDbQV8CJMRlh0UIpjUbShc4xnEkhU5RYty_83VKr76Mm4uNEVjd76OKg1wAAXSfHSxxX7j7hebk74Vk2-99vz6z3-hUNShZmQGpQNFExRjfx3ucyano5_N_2rKGD-IAWZCBXwfs1Gz_w";

/// server whose Apple JWKS is served locally (counting fetches); returns (base, client, jwks fetch count)
async fn apple_server(kid_served: &'static str) -> (String, Client, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let h2 = hits.clone();
    let jwks = json!({"keys": [{"kty": "RSA", "kid": kid_served, "use": "sig", "alg": "RS256", "n": RSA_N, "e": "AQAB"}]});
    let app = axum::Router::new().route("/keys", axum::routing::get(move || {
        h2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let j = jwks.clone();
        async move { axum::Json(j) }
    }));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let jurl = format!("http://{}/keys", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    let db = std::env::temp_dir().join(format!("mk-test-{}.db", masterkey_server::rand_b64(8)));
    let cfg = Config {
        database_url: format!("sqlite://{}?mode=rwc", db.display()), master_key: [7u8; 32], vendor_mock: true,
        apple_jwks_url: jurl, ..extra()
    };
    let state = build_state(cfg).await.unwrap();
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    let app = router(state);
    tokio::spawn(async move { axum::serve(l, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap() });
    (base, Client::new(), hits)
}

fn apple_token(kid: &str, sub: &str, aud: &str, iss: &str, exp_delta: i64, email: Option<&str>) -> String {
    let mut h = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    h.kid = Some(kid.into());
    let exp = chrono_now() + exp_delta;
    let mut c = json!({"iss": iss, "aud": aud, "sub": sub, "exp": exp, "iat": exp - 100});
    if let Some(e) = email { c["email"] = e.into(); }
    jsonwebtoken::encode(&h, &c, &jsonwebtoken::EncodingKey::from_rsa_pem(RSA_PEM.as_bytes()).unwrap()).unwrap()
}
fn chrono_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
}

#[tokio::test]
async fn apple_login_verifies_token() {
    let (base, c, hits) = apple_server("k1").await;
    let iss = "https://appleid.apple.com";
    let login = |tok: String| {
        let (c, base) = (c.clone(), base.clone());
        async move { c.post(format!("{base}/v1/auth/apple")).json(&json!({"id_token": tok})).send().await.unwrap() }
    };
    let ok = login(apple_token("k1", "001.abc", "app.masterkey.ios", iss, 600, Some("me@icloud.com"))).await;
    assert_eq!(ok.status(), 200);
    let j = json_of(ok).await;
    assert_eq!(j["user"]["email"], "me@icloud.com");
    let sess = j["session_token"].as_str().unwrap();
    assert_eq!(c.get(format!("{base}/v1/me/keys")).bearer_auth(sess).send().await.unwrap().status(), 200);
    // same sub => same user, even if the email claim changes
    let j2 = json_of(login(apple_token("k1", "001.abc", "app.masterkey.ios", iss, 600, Some("other@icloud.com"))).await).await;
    assert_eq!(j2["user"]["id"], j["user"]["id"]);
    // JWKS cached across logins
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
    // a different sub with a taken email must not attach to the existing account
    let j3 = json_of(login(apple_token("k1", "001.zzz", "app.masterkey.ios", iss, 600, Some("me@icloud.com"))).await).await;
    assert_ne!(j3["user"]["id"], j["user"]["id"]);
    // rejections
    for (name, tok) in [
        ("wrong aud", apple_token("k1", "s", "com.evil.app", iss, 600, None)),
        ("wrong iss", apple_token("k1", "s", "app.masterkey.ios", "https://evil.example", 600, None)),
        ("expired", apple_token("k1", "s", "app.masterkey.ios", iss, -3600, None)),
        ("unknown kid", apple_token("nope", "s", "app.masterkey.ios", iss, 600, None)),
        ("garbage", "not.a.jwt".to_string()),
    ] {
        let r = login(tok).await;
        assert_eq!(r.status(), 401, "{name}");
    }
}

// ---------- reservation ceiling & idempotency ----------

#[tokio::test]
async fn reservation_is_a_real_upper_bound() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    let bodies = [
        r##"!@#$%^&*()_+-=[]{}|;':",./<>?~`\"##.repeat(100),
        "你好世界こんにちは안녕하세요😀🎉🔥".repeat(80),
        "x".to_string(),
    ];
    for (i, text) in bodies.iter().enumerate() {
        let mut m = json!({"role": "user", "content": text});
        if i == 2 {
            m["name"] = json!("n".repeat(4000)); // long OpenAI `name`
        }
        let body = json!({"model": "gpt-4o", "messages": [m], "max_tokens": 50});
        let mut norm = body.clone();
        let chk = masterkey_server::pricing::validate("openai", &mut norm, true).unwrap();
        let sent_bytes = norm.to_string().len() as i64;
        let (ip, op) = chk.model.rates(sent_bytes);
        let ceiling = masterkey_server::pricing::cost(sent_bytes, ip) + masterkey_server::pricing::cost(50, op);
        assert!(chk.estimate_micro >= ceiling, "case {i}: {} < {ceiling}", chk.estimate_micro);
        // mock reports prompt_tokens = payload bytes (worst case): actual must never exceed the reservation
        let r = t.chat(&tok, 0, body).await;
        assert_eq!(r.status(), 200);
        let rid = r.headers()["x-masterkey-request-id"].to_str().unwrap().to_string();
        let b = json_of(r).await;
        assert_eq!(b["usage"]["prompt_tokens"], sent_bytes);
        let d = t.user_get(&format!("/v1/me/requests/{rid}")).await;
        assert!(d["actual_usd"].as_f64().unwrap() <= d["estimated_usd"].as_f64().unwrap(), "case {i}");
    }
}

#[tokio::test]
async fn idempotency_key_dedupes_and_conflicts() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    let post = |key: &'static str, body: Value| {
        t.c.post(format!("{}/v1/openai/chat/completions", t.base)).bearer_auth(&tok).header("Idempotency-Key", key).header("X-MasterKey-Wait", "0").json(&body).send()
    };
    let r1 = post("k1", msg()).await.unwrap();
    assert_eq!(r1.status(), 200);
    let id1 = r1.headers()["x-masterkey-request-id"].to_str().unwrap().to_string();
    let r2 = post("k1", msg()).await.unwrap();
    assert_eq!(r2.status(), 200);
    assert_eq!(r2.headers()["x-masterkey-request-id"].to_str().unwrap(), id1);
    assert_eq!(json_of(r2).await["object"], "chat.completion");
    // same key, different body
    let mut other = msg();
    other["messages"][0]["content"] = json!("different");
    let r3 = post("k1", other).await.unwrap();
    assert_eq!(r3.status(), 409);
    assert_eq!(json_of(r3).await["error"]["type"], "idempotency_conflict");
    // bad keys
    let long: &'static str = Box::leak("k".repeat(129).into_boxed_str());
    assert_eq!(post(long, msg()).await.unwrap().status(), 400);
    // another key is a new request
    assert_eq!(post("k2", msg()).await.unwrap().status(), 200);
    assert_eq!(t.user_get("/v1/me/grants").await[0]["request_count"], 2);
    assert_eq!(t.user_get("/v1/me/requests").await.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn concurrent_duplicates_send_once() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    let calls = (0..5).map(|_| t.c.post(format!("{}/v1/openai/chat/completions", t.base)).bearer_auth(&tok).header("Idempotency-Key", "same").json(&msg()).send());
    let rs = futures_join(calls).await;
    let mut ids = std::collections::HashSet::new();
    for r in rs {
        let r = r.unwrap();
        assert_eq!(r.status(), 200);
        ids.insert(r.headers()["x-masterkey-request-id"].to_str().unwrap().to_string());
    }
    assert_eq!(ids.len(), 1);
    assert_eq!(t.user_get("/v1/me/grants").await[0]["request_count"], 1);
    assert_eq!(t.user_get("/v1/me/requests").await.as_array().unwrap().len(), 1);
}

async fn futures_join<F: std::future::Future + Send + 'static>(fs: impl Iterator<Item = F>) -> Vec<F::Output>
where
    F::Output: Send + 'static,
{
    let hs: Vec<_> = fs.map(tokio::spawn).collect();
    let mut out = vec![];
    for h in hs {
        out.push(h.await.unwrap());
    }
    out
}

#[tokio::test]
async fn idempotent_repeat_while_pending_creates_one_request() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let post = || t.c.post(format!("{}/v1/openai/chat/completions", t.base)).bearer_auth(&tok).header("Idempotency-Key", "p1").header("X-MasterKey-Wait", "0").json(&msg()).send();
    let a = json_of(post().await.unwrap()).await;
    let b = json_of(post().await.unwrap()).await;
    assert_eq!(a["status"], "pending");
    assert_eq!(a["request_id"], b["request_id"]);
    assert_eq!(t.user_get("/v1/me/requests?status=pending").await.as_array().unwrap().len(), 1);
}

// ---------- public (browser) clients ----------

const ORIGIN: &str = "https://demo.example.com";

#[test]
fn origin_validation() {
    for ok in ["https://a.github.io", "https://x.com:8443", "http://localhost:3001", "http://127.0.0.1:3001", "http://[::1]:5000"] {
        assert!(origin_ok(ok), "{ok}");
    }
    for bad in ["http://example.com", "https://a.io/", "https://a.io/path", "https://A.io", "ftp://a.io", "https://", "http://localhost.evil.com", "null", ""] {
        assert!(!origin_ok(bad), "{bad}");
    }
}

#[tokio::test]
async fn public_client_connect_and_cors() {
    let t = T::new().await;
    let b = &t.base;
    // invalid origins rejected at registration
    for o in [json!([]), json!(["http://example.com"]), json!(["https://a.io/x"])] {
        let r = t.c.post(format!("{b}/v1/apps")).json(&json!({"name": "X", "allowed_origins": o})).send().await.unwrap();
        assert_eq!(r.status(), 400, "{o}");
    }
    let app = json_of(t.c.post(format!("{b}/v1/apps")).json(&json!({"name": "Static", "allowed_origins": [ORIGIN]})).send().await.unwrap()).await;
    let app_id = app["app_id"].as_str().unwrap();
    let connect = |origin: Option<&str>, id: &str| {
        let mut r = t.c.post(format!("{b}/v1/connect")).json(&json!({"vendor": "openai", "app_id": id}));
        if let Some(o) = origin { r = r.header("origin", o); }
        r.send()
    };
    // wrong or missing origin, unknown app
    for (o, id) in [(Some("https://evil.example"), app_id), (None, app_id), (Some(ORIGIN), "app_nope")] {
        let r = connect(o, id).await.unwrap();
        assert_eq!(r.status(), 403);
        assert_eq!(json_of(r).await["error"]["type"], "origin_not_allowed");
    }
    let r = connect(Some(ORIGIN), app_id).await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["access-control-allow-origin"], "*");
    let c = json_of(r).await;
    let (code, cs) = (c["connect_code"].as_str().unwrap(), c["connect_secret"].as_str().unwrap());
    assert!(cs.starts_with("mk_cs_"));
    let poll = |tok: &str| t.c.get(format!("{b}/v1/connect/{code}")).bearer_auth(tok).send();
    // the code alone (or another code's secret) never claims the grant
    assert_eq!(t.c.get(format!("{b}/v1/connect/{code}")).send().await.unwrap().status(), 401);
    assert_eq!(poll("mk_cs_wrong").await.unwrap().status(), 404);
    assert_eq!(json_of(poll(cs).await.unwrap()).await["status"], "pending");
    // the phone sees where the connect came from
    let prev = t.user_get(&format!("/v1/me/connect/{code}")).await;
    assert_eq!(prev["origin"], ORIGIN);
    assert_eq!(prev["app"]["name"], "Static");
    assert_eq!(prev["requires_ask"], true);
    let denied_wide = t.user_post(&format!("/v1/me/connect/{code}/approve"), json!({"policy": {"mode": "trust"}})).await;
    assert_eq!(denied_wide.status(), 400);
    assert_eq!(json_of(denied_wide).await["error"]["type"], "policy_not_allowed_yet");
    let first_grant = json_of(t.user_post(&format!("/v1/me/connect/{code}/approve"), json!({"policy": {"mode": "ask"}})).await).await;
    assert_eq!(first_grant["policy"]["mode"], "ask");
    let r = t.c.patch(format!("{b}/v1/me/grants/{}", first_grant["id"].as_str().unwrap())).bearer_auth(&t.sess)
        .json(&json!({"policy": {"mode": "trust"}})).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let p = json_of(poll(cs).await.unwrap()).await;
    let grant = p["grant_token"].as_str().unwrap();
    assert!(json_of(poll(cs).await.unwrap()).await.get("grant_token").is_none(), "token is shown once");
    // the browser can call the proxy and read the request id
    let r = t.c.post(format!("{b}/v1/openai/chat/completions")).header("origin", ORIGIN).bearer_auth(grant)
        .json(&json!({"model": "gpt-4o-mini", "messages": [{"role": "user", "content": "hi"}]})).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["access-control-allow-origin"], "*");
    assert_eq!(r.headers()["access-control-expose-headers"], "x-masterkey-request-id");
    // preflight on developer routes; none on user routes
    let pre = |p: &str| t.c.request(reqwest::Method::OPTIONS, format!("{b}{p}")).header("origin", ORIGIN)
        .header("access-control-request-method", "POST").header("access-control-request-headers", "authorization,content-type,idempotency-key,x-masterkey-wait").send();
    let r = pre("/v1/openai/chat/completions").await.unwrap();
    assert_eq!(r.status(), 204);
    assert_eq!(r.headers()["access-control-allow-headers"], "authorization,content-type,idempotency-key,x-masterkey-wait");
    assert_eq!(pre("/v1/connect").await.unwrap().status(), 204);
    let r = pre("/v1/me/keys").await.unwrap();
    assert!(r.headers().get("access-control-allow-origin").is_none());
    // server apps are unchanged: no connect_secret, and no Origin needed
    let srv = json_of(t.c.post(format!("{b}/v1/apps")).json(&json!({"name": "Server"})).send().await.unwrap()).await;
    let c = json_of(t.c.post(format!("{b}/v1/connect")).bearer_auth(srv["app_secret"].as_str().unwrap()).json(&json!({"vendor": "openai"})).send().await.unwrap()).await;
    assert!(c.get("connect_secret").is_none() && c["connect_code"].is_string());
    // a server app cannot be used from a browser by id
    let r = connect(Some(ORIGIN), srv["app_id"].as_str().unwrap()).await.unwrap();
    assert_eq!(r.status(), 403);
}

#[tokio::test]
async fn declared_public_apps_are_seeded_and_updated() {
    let app = |name: &str| PublicApp { id: "app_resume_tailor".into(), name: name.into(), homepage: None, allowed_origins: vec![ORIGIN.into()] };
    let (b, c, state) = server_with(vec![app("Resume Tailor")]).await;
    let r = c.post(format!("{b}/v1/connect")).header("origin", ORIGIN).json(&json!({"vendor": "anthropic", "app_id": "app_resume_tailor"})).send().await.unwrap();
    assert_eq!(r.status(), 200);
    // a restart with a changed declaration updates the row instead of failing
    let mut cfg = Config { database_url: state.cfg.database_url.clone(), master_key: [7u8; 32], public_apps: vec![app("Renamed")], ..extra() };
    cfg.vendor_mock = true;
    let s2 = build_state(cfg).await.unwrap();
    let name: String = sqlx::query_scalar("SELECT name FROM apps WHERE id='app_resume_tailor'").fetch_one(&s2.db).await.unwrap();
    assert_eq!(name, "Renamed");
}

#[tokio::test]
async fn public_connect_is_rate_limited() {
    let app = PublicApp { id: "app_rl".into(), name: "RL".into(), homepage: None, allowed_origins: vec![ORIGIN.into()] };
    let (b, c, _) = server_with(vec![app]).await;
    let mut last = 0;
    for _ in 0..21 {
        last = c.post(format!("{b}/v1/connect")).header("origin", ORIGIN).json(&json!({"vendor": "openai", "app_id": "app_rl"})).send().await.unwrap().status().as_u16();
    }
    assert_eq!(last, 429);
}

#[tokio::test]
async fn developer_app_info_rotation_and_report() {
    let t = T::new().await;
    let r = t.c.request(reqwest::Method::OPTIONS, format!("{}/v1/apps", t.base)).header("origin", "https://x.dev").header("access-control-request-method", "POST").send().await.unwrap();
    assert_eq!(r.headers()["access-control-allow-origin"], "*");
    let app = json_of(t.c.post(format!("{}/v1/apps", t.base)).json(&json!({"name": "Reports", "allowed_origins": ["https://x.dev"]})).send().await.unwrap()).await;
    let secret = app["app_secret"].as_str().unwrap().to_string();
    let info = json_of(t.c.get(format!("{}/v1/app", t.base)).bearer_auth(&secret).send().await.unwrap()).await;
    assert_eq!((info["name"].as_str(), info["allowed_origins"][0].as_str()), (Some("Reports"), Some("https://x.dev")));
    // empty report: every day present, zeros
    let rep = json_of(t.c.get(format!("{}/v1/app/report?days=7", t.base)).bearer_auth(&secret).send().await.unwrap()).await;
    assert_eq!(rep["daily"].as_array().unwrap().len(), 7);
    assert_eq!(rep["totals"]["requests"], 0);
    assert_eq!(t.c.get(format!("{}/v1/app/report?days=0", t.base)).bearer_auth(&secret).send().await.unwrap().status(), 400);

    // connect approved from a phone in Japan, then 2 completed calls and 1 denied
    let conn = json_of(t.c.post(format!("{}/v1/connect", t.base)).bearer_auth(&secret).json(&json!({"vendor": "openai"})).send().await.unwrap()).await;
    let code = conn["connect_code"].as_str().unwrap();
    let r = t.c.post(format!("{}/v1/me/connect/{code}/approve", t.base)).bearer_auth(&t.sess).header("cf-ipcountry", "jp").json(&json!({"policy": {"mode": "ask"}})).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let tok = json_of(t.c.get(format!("{}/v1/connect/{code}", t.base)).bearer_auth(&secret).send().await.unwrap()).await["grant_token"].as_str().unwrap().to_string();
    for decision in ["approve", "approve", "deny"] {
        assert_eq!(t.chat(&tok, 0, msg()).await.status(), 202);
        let id = t.pending().await;
        assert_eq!(t.user_post(&format!("/v1/me/requests/{id}/{decision}"), json!({})).await.status(), 200);
        t.wait_status(&id, if decision == "approve" { "completed" } else { "denied" }).await;
    }

    // rotation: old secret stops working
    let new = json_of(t.c.post(format!("{}/v1/app/secret", t.base)).bearer_auth(&secret).send().await.unwrap()).await["app_secret"].as_str().unwrap().to_string();
    assert_eq!(t.c.get(format!("{}/v1/app", t.base)).bearer_auth(&secret).send().await.unwrap().status(), 401);

    let rep = json_of(t.c.get(format!("{}/v1/app/report", t.base)).bearer_auth(&new).send().await.unwrap()).await;
    let tot = &rep["totals"];
    assert_eq!((tot["requests"].as_i64(), tot["completed"].as_i64(), tot["denied"].as_i64(), tot["users"].as_i64(), tot["new_users"].as_i64()), (Some(3), Some(2), Some(1), Some(1), Some(1)));
    assert!(tot["cost_usd"].as_f64().unwrap() > 0.0 && tot["input_tokens"].as_i64().unwrap() > 0 && tot["output_tokens"].as_i64().unwrap() > 0);
    assert_eq!(tot["unmetered"], 0);
    assert_eq!(rep["daily"].as_array().unwrap().len(), 30);
    assert_eq!(rep["daily"][29]["requests"], 3);
    assert_eq!(rep["models"][0]["model"], "gpt-4o-mini");
    assert_eq!(rep["models"][0]["completed"], 2);
    assert_eq!(rep["countries"], json!([{"country": "JP", "users": 1, "requests": 3, "cost_usd": tot["cost_usd"]}]));
    let s = rep.to_string();
    assert!(!s.contains(&t.sess) && !s.contains("a@b.co") && !s.contains("\"hi\""), "report leaks user data");

    // another app sees none of it
    let other = json_of(t.c.post(format!("{}/v1/apps", t.base)).json(&json!({"name": "Other"})).send().await.unwrap()).await;
    let rep = json_of(t.c.get(format!("{}/v1/app/report", t.base)).bearer_auth(other["app_secret"].as_str().unwrap()).send().await.unwrap()).await;
    assert_eq!(rep["totals"]["requests"], 0);
    assert_eq!(rep["countries"], json!([]));
}
