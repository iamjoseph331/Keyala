use masterkey_server::{build_state, router, send_request, AppState, Config};
use reqwest::{Client, Response};
use serde_json::{json, Value};
use std::{net::SocketAddr, time::Duration};

struct T {
    base: String,
    c: Client,
    sess: String,
    state: AppState,
}

async fn server() -> (String, Client, AppState) {
    let db = std::env::temp_dir().join(format!("mk-test-{}.db", masterkey_server::rand_b64(8)));
    let cfg = Config {
        database_url: format!("sqlite://{}?mode=rwc", db.display()),
        master_key: [7u8; 32],
        dev_login: true,
        vendor_mock: true,
        public_url: "http://mk.test".into(),
        prices_verified: false,
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
        let g = json_of(self.user_post(&format!("/v1/me/connect/{code}/approve"), json!({"policy": policy})).await).await;
        assert_eq!(g["vendor"], vendor);
        let p = json_of(poll(&self.c).await.unwrap()).await;
        assert_eq!(p["status"], "approved");
        let tok = p["grant_token"].as_str().unwrap().to_string();
        assert!(tok.starts_with("mk_grant_"));
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
async fn unverified_prices_fail_closed_outside_mock() {
    let mut body = json!({"model": "gpt-4o", "messages": [{"role": "user", "content": "x"}]});
    let e = masterkey_server::pricing::validate("openai", &mut body, false).err().unwrap();
    assert_eq!(e.1, "model_not_priced");
    assert!(masterkey_server::pricing::validate("openai", &mut body, true).is_ok());
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
        master_key: [7u8; 32], dev_login: true, vendor_mock: true, public_url: "http://x".into(), prices_verified: false,
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
