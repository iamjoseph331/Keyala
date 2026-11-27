use keyala_server::{build_state, origin_ok, router, send_request, AppState, Config, PublicApp};
use reqwest::{Client, Response};
use serde_json::{json, Value};
use std::{net::SocketAddr, time::Duration};

/// apple/apns defaults for struct-update syntax
fn extra() -> Config {
    Config {
        database_url: String::new(),
        master_key: [0; 32],
        dev_login: false,
        vendor_mock: false,
        public_url: String::new(),
        prices_verified: false,
        apple_bundle_id: "app.keyala.ios".into(),
        apple_team_id: "TEAMID".into(),
        apple_jwks_url: "http://127.0.0.1:1/none".into(),
        google_client_ids: vec![],
        google_jwks_url: "http://127.0.0.1:1/none".into(),
        passkey_rp_id: "localhost".into(),
        passkey_origins: vec!["https://localhost".into()],
        android_app: None,
        apns: None,
        public_apps: vec![],
        country_header: None,
        password_login: None,
        apple_signin_key: None,
        apple_auth_url: "http://127.0.0.1:1".into(),
        pause_upstream: false,
        backup_dir: None,
        backup_keep: 7,
        backup_interval: Duration::from_secs(86_400),
        plaza_enabled: false,
        plaza_reviewers: vec![],
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
    serve(Config {
        public_apps,
        ..test_cfg()
    })
    .await
}

fn test_cfg() -> Config {
    // one directory per test server: PAUSE_UPSTREAM and deleted-accounts.log live beside the database
    let dir = std::env::temp_dir().join(format!("mk-test-{}", keyala_server::rand_b64(8)));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("masterkey.db");
    Config {
        database_url: format!("sqlite://{}?mode=rwc", db.display()),
        master_key: [7u8; 32],
        dev_login: true,
        vendor_mock: true,
        public_url: "http://mk.test".into(),
        prices_verified: false,
        country_header: Some("cf-ipcountry".into()),
        ..extra()
    }
}

async fn serve(cfg: Config) -> (String, Client, AppState) {
    let state = build_state(cfg).await.unwrap();
    let app = router(state.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(l, app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .unwrap()
    });
    (base, Client::new(), state)
}

async fn json_of(r: Response) -> Value {
    r.json().await.unwrap()
}

impl T {
    /// server + logged-in user with an OpenAI key
    async fn new() -> T {
        let (base, c, state) = server().await;
        let r = json_of(
            c.post(format!("{base}/v1/auth/dev"))
                .json(&json!({"email": "a@b.co"}))
                .send()
                .await
                .unwrap(),
        )
        .await;
        let sess = r["session_token"].as_str().unwrap().to_string();
        let t = T {
            base,
            c,
            sess,
            state,
        };
        let r = t
            .user_put(
                "/v1/me/keys/openai",
                json!({"api_key": "sk-test-SECRET1234"}),
            )
            .await;
        assert_eq!(r.status(), 200);
        t
    }
    async fn user_put(&self, p: &str, b: Value) -> Response {
        self.c
            .put(format!("{}{p}", self.base))
            .bearer_auth(&self.sess)
            .json(&b)
            .send()
            .await
            .unwrap()
    }
    async fn user_post(&self, p: &str, b: Value) -> Response {
        self.c
            .post(format!("{}{p}", self.base))
            .bearer_auth(&self.sess)
            .json(&b)
            .send()
            .await
            .unwrap()
    }
    async fn user_get(&self, p: &str) -> Value {
        json_of(
            self.c
                .get(format!("{}{p}", self.base))
                .bearer_auth(&self.sess)
                .send()
                .await
                .unwrap(),
        )
        .await
    }
    /// register app, connect, approve with `policy`, return grant token
    async fn grant(&self, vendor: &str, policy: Value) -> String {
        let app = json_of(
            self.c
                .post(format!("{}/v1/apps", self.base))
                .json(&json!({"name": "Demo"}))
                .send()
                .await
                .unwrap(),
        )
        .await;
        let secret = app["app_secret"].as_str().unwrap();
        let conn = json_of(
            self.c
                .post(format!("{}/v1/connect", self.base))
                .bearer_auth(secret)
                .json(&json!({"vendor": vendor}))
                .send()
                .await
                .unwrap(),
        )
        .await;
        let code = conn["connect_code"].as_str().unwrap();
        assert!(conn["connect_url"]
            .as_str()
            .unwrap()
            .ends_with(&format!("/connect/{code}")));
        let poll = |c: &Client| {
            c.get(format!("{}/v1/connect/{code}", self.base))
                .bearer_auth(secret)
                .send()
        };
        assert_eq!(
            json_of(poll(&self.c).await.unwrap()).await["status"],
            "pending"
        );
        let prev = self.user_get(&format!("/v1/me/connect/{code}")).await;
        assert_eq!(prev["app"]["name"], "Demo");
        // Each helper-created app is new. Model a user deliberately widening the
        // first ask-only grant through the Grants screen when a test needs a broad policy.
        let approved_policy = if policy["mode"] == "ask" {
            policy.clone()
        } else {
            json!({"mode": "ask"})
        };
        let g = json_of(
            self.user_post(
                &format!("/v1/me/connect/{code}/approve"),
                json!({"policy": approved_policy}),
            )
            .await,
        )
        .await;
        assert_eq!(g["vendor"], vendor);
        let p = json_of(poll(&self.c).await.unwrap()).await;
        assert_eq!(p["status"], "approved");
        let tok = p["grant_token"].as_str().unwrap().to_string();
        assert!(tok.starts_with("keyala_grant_"));
        if policy["mode"] != "ask" {
            let r = self
                .c
                .patch(format!(
                    "{}/v1/me/grants/{}",
                    self.base,
                    g["id"].as_str().unwrap()
                ))
                .bearer_auth(&self.sess)
                .json(&json!({"policy": policy}))
                .send()
                .await
                .unwrap();
            assert_eq!(r.status(), 200);
        }
        // token is delivered once
        assert!(json_of(poll(&self.c).await.unwrap())
            .await
            .get("grant_token")
            .is_none());
        tok
    }
    async fn chat(&self, tok: &str, wait: u32, body: Value) -> Response {
        self.c
            .post(format!("{}/v1/openai/chat/completions", self.base))
            .bearer_auth(tok)
            .header("X-Keyala-Wait", wait.to_string())
            .json(&body)
            .send()
            .await
            .unwrap()
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
    assert_eq!(
        t.user_put(
            "/v1/me/keys/anthropic",
            json!({"api_key":"sk-ant-test-SECRET1234"})
        )
        .await
        .status(),
        200
    );
    let app = json_of(
        t.c.post(format!("{}/v1/apps", t.base))
            .json(&json!({"name": "Same app"}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let secret = app["app_secret"].as_str().unwrap();
    async fn new_code(t: &T, secret: &str, vendor: &str) -> String {
        let resp =
            t.c.post(format!("{}/v1/connect", t.base))
                .bearer_auth(secret)
                .json(&json!({"vendor": vendor}))
                .send()
                .await
                .unwrap();
        json_of(resp).await["connect_code"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    let first_code = new_code(&t, secret, "openai").await;
    let first_preview = t.user_get(&format!("/v1/me/connect/{first_code}")).await;
    assert_eq!(first_preview["requires_ask"], true);
    let rejected = t
        .user_post(
            &format!("/v1/me/connect/{first_code}/approve"),
            json!({"policy":{"mode":"budget","remaining_usd":2.0}}),
        )
        .await;
    assert_eq!(rejected.status(), 400);
    assert_eq!(
        json_of(rejected).await["error"]["type"],
        "policy_not_allowed_yet"
    );
    // Rejection rolled back the connect status, so ask can still be approved.
    let first = json_of(
        t.user_post(
            &format!("/v1/me/connect/{first_code}/approve"),
            json!({"policy":{"mode":"ask"}}),
        )
        .await,
    )
    .await;
    assert_eq!(first["policy"]["mode"], "ask");
    let first_id = first["id"].as_str().unwrap();
    assert_eq!(
        t.c.delete(format!("{}/v1/me/grants/{first_id}", t.base))
            .bearer_auth(&t.sess)
            .send()
            .await
            .unwrap()
            .status(),
        204
    );

    // Any grant history counts, including revoked grants and grants for another vendor.
    let later_code = new_code(&t, secret, "anthropic").await;
    let later_preview = t.user_get(&format!("/v1/me/connect/{later_code}")).await;
    assert_eq!(later_preview["requires_ask"], false);
    let later = t
        .user_post(
            &format!("/v1/me/connect/{later_code}/approve"),
            json!({"policy":{"mode":"trust"}}),
        )
        .await;
    assert_eq!(later.status(), 200);

    // History is scoped to the authenticated user, even for the same app.
    let other = json_of(
        t.c.post(format!("{}/v1/auth/dev", t.base))
            .json(&json!({"email":"other@b.co"}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let other_session = other["session_token"].as_str().unwrap();
    let key =
        t.c.put(format!("{}/v1/me/keys/openai", t.base))
            .bearer_auth(other_session)
            .json(&json!({"api_key":"sk-other-SECRET1234"}))
            .send()
            .await
            .unwrap();
    assert_eq!(key.status(), 200);
    let other_code = new_code(&t, secret, "openai").await;
    let preview =
        t.c.get(format!("{}/v1/me/connect/{other_code}", t.base))
            .bearer_auth(other_session)
            .send()
            .await
            .unwrap();
    let preview = json_of(preview).await;
    assert_eq!(preview["requires_ask"], true);
    let rejected =
        t.c.post(format!("{}/v1/me/connect/{other_code}/approve", t.base))
            .bearer_auth(other_session)
            .json(&json!({"policy":{"mode":"count","remaining_count":3}}))
            .send()
            .await
            .unwrap();
    assert_eq!(rejected.status(), 400);
    assert_eq!(
        json_of(rejected).await["error"]["type"],
        "policy_not_allowed_yet"
    );
}

#[tokio::test]
async fn full_connect_flow_and_proxy_call() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    let r = t.chat(&tok, 0, msg()).await;
    assert_eq!(r.status(), 200);
    let rid = r.headers()["x-keyala-request-id"]
        .to_str()
        .unwrap()
        .to_string();
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
    let p = json_of(
        t.c.get(format!("{}/v1/requests/{rid}", t.base))
            .bearer_auth(&tok)
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(p["status"], "completed");
    assert_eq!(p["response"]["object"], "chat.completion");
}

#[tokio::test]
async fn ask_held_then_approved() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let (t2, tok2) = (t.base.clone(), tok.clone());
    let call = tokio::spawn(async move {
        Client::new()
            .post(format!("{t2}/v1/openai/chat/completions"))
            .bearer_auth(tok2)
            .header("X-Keyala-Wait", "10")
            .json(&msg())
            .send()
            .await
            .unwrap()
    });
    let id = t.pending().await;
    let detail = t.user_get(&format!("/v1/me/requests/{id}")).await;
    assert_eq!(detail["payload"]["messages"][0]["content"], "hi");
    assert!(detail["estimated_usd"].as_f64().unwrap() > 0.0);
    assert_eq!(
        t.user_post(&format!("/v1/me/requests/{id}/approve"), json!({}))
            .await
            .status(),
        200
    );
    let r = call.await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["x-keyala-request-id"], id.as_str());
    assert_eq!(json_of(r).await["object"], "chat.completion");
}

#[tokio::test]
async fn deny_gives_403() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let (t2, tok2) = (t.base.clone(), tok.clone());
    let call = tokio::spawn(async move {
        Client::new()
            .post(format!("{t2}/v1/openai/chat/completions"))
            .bearer_auth(tok2)
            .header("X-Keyala-Wait", "10")
            .json(&msg())
            .send()
            .await
            .unwrap()
    });
    let id = t.pending().await;
    assert_eq!(
        t.user_post(&format!("/v1/me/requests/{id}/deny"), json!({}))
            .await
            .status(),
        200
    );
    let r = call.await.unwrap();
    assert_eq!(r.status(), 403);
    assert_eq!(json_of(r).await["error"]["type"], "keyala_denied");
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
    t.user_post(&format!("/v1/me/requests/{id}/approve"), json!({}))
        .await;
    let d = t.wait_status(&id, "completed").await;
    assert_eq!(d["response"]["object"], "chat.completion");
    let p = json_of(
        t.c.get(format!("{}/v1/requests/{id}", t.base))
            .bearer_auth(&tok)
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(p["status"], "completed");
    assert!(p["response"].is_object());
}

#[tokio::test]
async fn count_policy_auto_approves_n_then_asks() {
    let t = T::new().await;
    let tok = t
        .grant("openai", json!({"mode": "count", "remaining_count": 2}))
        .await;
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
    let id = json_of(t.chat(&tok, 0, msg()).await).await["request_id"]
        .as_str()
        .unwrap()
        .to_string();
    t.user_post(
        &format!("/v1/me/requests/{id}/approve"),
        json!({"policy": {"mode": "count", "remaining_count": 3}}),
    )
    .await;
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
    let tok = t
        .grant("openai", json!({"mode": "budget", "remaining_usd": 0.025}))
        .await;
    assert_eq!(t.chat(&tok, 0, big.clone()).await.status(), 200);
    // reservation (~$0.010) was settled to the tiny actual cost: nearly all budget is back
    let left = t.user_get("/v1/me/grants").await[0]["policy"]["remaining_usd"]
        .as_f64()
        .unwrap();
    assert!(
        left > 0.0245 && left < 0.025,
        "settled to actual cost: {left}"
    );
    // budget below the next reservation -> needs approval
    let r = t.user_get("/v1/me/grants").await;
    let gid = r[0]["id"].as_str().unwrap();
    let patch =
        t.c.patch(format!("{}/v1/me/grants/{gid}", t.base))
            .bearer_auth(&t.sess)
            .json(&json!({"policy": {"mode": "budget", "remaining_usd": 0.005}}))
            .send()
            .await
            .unwrap();
    assert_eq!(patch.status(), 200);
    assert_eq!(t.chat(&tok, 0, big.clone()).await.status(), 202);
    // upstream failure refunds the reservation
    let t2 = T::new().await;
    let tok2 = t2
        .grant("openai", json!({"mode": "budget", "remaining_usd": 1.0}))
        .await;
    let mut fail = big.clone();
    fail["messages"][0]["content"] = json!("KEYALA_MOCK_FAIL");
    let r = t2.chat(&tok2, 0, fail).await;
    assert_eq!(r.status(), 500);
    let left = t2.user_get("/v1/me/grants").await[0]["policy"]["remaining_usd"]
        .as_f64()
        .unwrap();
    assert!((left - 1.0).abs() < 1e-9, "reservation refunded: {left}");
}

#[tokio::test]
async fn validation_rejections() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    let cases = [
        (
            json!({"model": "gpt-4o", "messages": [{"role":"user","content":"x"}], "stream": true}),
            "streaming_unsupported",
        ),
        (
            json!({"model": "gpt-99", "messages": [{"role":"user","content":"x"}]}),
            "model_not_allowed",
        ),
        (
            json!({"model": "gpt-4o", "messages": [{"role":"user","content":"x"}], "n": 2}),
            "unsupported_option",
        ),
        (
            json!({"model": "gpt-4o", "messages": [{"role":"user","content":"x"}], "tools": []}),
            "unsupported_option",
        ),
        (
            json!({"model": "gpt-4o", "messages": [{"role":"user","content":"x"}], "logit_bias": {}}),
            "unsupported_option",
        ),
        (
            json!({"model": "gpt-4o", "messages": [{"role":"user","content":[{"type":"text","text":"x","cache_control":{}}]}]}),
            "unsupported_option",
        ),
        (
            json!({"model": "gpt-4o", "messages": [{"role":"user","content":"x","tool_calls":[]}]}),
            "unsupported_option",
        ),
        (
            json!({"model": "gpt-4o", "messages": [{"role":"user","content":"x"}], "response_format": {"type":"json_schema"}}),
            "unsupported_option",
        ),
        (
            json!({"model": "gpt-4o", "messages": [{"role":"tool","content":"x"}]}),
            "invalid_request",
        ),
        (
            json!({"model": "gpt-4o", "messages": [{"role":"user","content":[{"type":"image_url","image_url":{"url":"http://x/y.png"}}]}]}),
            "unsupported_content",
        ),
        (
            json!({"model": "gpt-4o", "messages": [{"role":"user","content":"x"}], "max_tokens": 999999}),
            "max_tokens_too_large",
        ),
    ];
    for (body, ty) in cases {
        let r = t.chat(&tok, 0, body).await;
        assert_eq!(r.status(), 400, "{ty}");
        assert_eq!(json_of(r).await["error"]["type"], ty);
    }
    // missing max_tokens gets the default injected and the call works
    let r = t
        .chat(
            &tok,
            0,
            json!({"model": "gpt-4o", "messages": [{"role":"user","content":"x"}]}),
        )
        .await;
    assert_eq!(r.status(), 200);
    let id = r.headers()["x-keyala-request-id"]
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(
        t.user_get(&format!("/v1/me/requests/{id}")).await["payload"]["max_tokens"],
        1024
    );
}

#[tokio::test]
async fn keys_never_returned() {
    let t = T::new().await;
    let list =
        t.c.get(format!("{}/v1/me/keys", t.base))
            .bearer_auth(&t.sess)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
    assert!(list.contains("\"last4\":\"1234\""));
    assert!(!list.contains("SECRET") && !list.contains("sk-test"));
    let put = t
        .user_put(
            "/v1/me/keys/openai",
            json!({"api_key": "sk-test-OTHER5678"}),
        )
        .await
        .text()
        .await
        .unwrap();
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
    t.user_put(
        "/v1/me/keys/anthropic",
        json!({"api_key": "sk-ant-test-0000"}),
    )
    .await;
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
    let id = json_of(t.chat(&tok, 0, msg()).await).await["request_id"]
        .as_str()
        .unwrap()
        .to_string();
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
    assert_eq!(
        t.c.get(format!("{}/healthz", t.base))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "ok"
    );
    let r =
        t.c.get(format!("{}/v1/me/keys", t.base))
            .send()
            .await
            .unwrap();
    assert_eq!(r.status(), 401);
    assert_eq!(json_of(r).await["error"]["type"], "unauthorized");
    let page =
        t.c.get(format!("{}/connect/KY-ABCD", t.base))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
    assert!(page.contains("keyala://connect/KY-ABCD") && page.contains("Open in the Keyala app"));
    assert_eq!(
        t.c.get(format!("{}/connect/<script>", t.base))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        t.user_post("/v1/me/devices", json!({"apns_token": "abc"}))
            .await
            .status(),
        204
    );
    // app registration is rate limited
    let mut last = 0;
    for _ in 0..11 {
        last =
            t.c.post(format!("{}/v1/apps", t.base))
                .json(&json!({"name": "x"}))
                .send()
                .await
                .unwrap()
                .status()
                .as_u16();
    }
    assert_eq!(last, 429);
}

#[tokio::test]
async fn sse_events_filtered_by_user() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let mut ev =
        t.c.get(format!("{}/v1/me/events", t.base))
            .bearer_auth(&t.sess)
            .send()
            .await
            .unwrap();
    assert_eq!(ev.status(), 200);
    t.chat(&tok, 0, msg()).await;
    let chunk = tokio::time::timeout(Duration::from_secs(5), ev.chunk())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
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
        .bind(keyala_server::hash_token(&t.sess))
        .fetch_one(&t.state.db)
        .await
        .unwrap();
    sqlx::query("UPDATE requests SET created_at='2026-11-14T00:00:00.000Z' WHERE user_id=?")
        .bind(&uid)
        .execute(&t.state.db)
        .await
        .unwrap();

    let all = t
        .user_get("/v1/me/requests?status=completed&limit=200")
        .await;
    let expected: Vec<String> = all
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
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
        if rows.len() < 2 {
            break;
        }
        before = Some(rows.last().unwrap()["id"].as_str().unwrap().to_string());
    }
    assert_eq!(
        found, expected,
        "pages must cover the ordered history without gaps or duplicates"
    );

    // A cursor is valid only for the authenticated owner; missing and foreign IDs are both 404.
    let second = json_of(
        t.c.post(format!("{}/v1/auth/dev", t.base))
            .json(&json!({"email":"other@b.co"}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let other_sess = second["session_token"].as_str().unwrap();
    let foreign =
        t.c.get(format!(
            "{}/v1/me/requests?before_id={}",
            t.base, expected[0]
        ))
        .bearer_auth(other_sess)
        .send()
        .await
        .unwrap();
    assert_eq!(foreign.status(), 404);
    let missing =
        t.c.get(format!(
            "{}/v1/me/requests?before_id=missing-request",
            t.base
        ))
        .bearer_auth(&t.sess)
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
}

async fn user_b(t: &T) -> String {
    let r = json_of(
        t.c.post(format!("{}/v1/auth/dev", t.base))
            .json(&json!({"email": "b@b.co"}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    r["session_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn auth_isolation() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let tok2 = t.grant("openai", json!({"mode": "ask"})).await;
    let id = json_of(t.chat(&tok, 0, msg()).await).await["request_id"]
        .as_str()
        .unwrap()
        .to_string();
    let gid = t.user_get("/v1/me/grants").await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let b = user_b(&t).await;
    let bget = |p: String| t.c.get(format!("{}{p}", t.base)).bearer_auth(&b).send();
    assert_eq!(
        bget("/v1/me/requests".into())
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap(),
        json!([])
    );
    assert_eq!(
        bget("/v1/me/grants".into())
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap(),
        json!([])
    );
    assert_eq!(
        bget(format!("/v1/me/requests/{id}"))
            .await
            .unwrap()
            .status(),
        404
    );
    for act in ["approve", "deny"] {
        let r =
            t.c.post(format!("{}/v1/me/requests/{id}/{act}", t.base))
                .bearer_auth(&b)
                .json(&json!({}))
                .send()
                .await
                .unwrap();
        assert_eq!(r.status(), 404);
    }
    assert_eq!(
        t.c.patch(format!("{}/v1/me/grants/{gid}", t.base))
            .bearer_auth(&b)
            .json(&json!({"policy":{"mode":"trust"}}))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        t.c.delete(format!("{}/v1/me/grants/{gid}", t.base))
            .bearer_auth(&b)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    // still pending for A
    assert_eq!(
        t.user_get(&format!("/v1/me/requests/{id}")).await["status"],
        "pending"
    );
    // another grant cannot poll this request
    assert_eq!(
        t.c.get(format!("{}/v1/requests/{id}", t.base))
            .bearer_auth(&tok2)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    // token kinds are not interchangeable
    assert_eq!(t.chat(&t.sess, 0, msg()).await.status(), 401);
    assert_eq!(
        t.c.get(format!("{}/v1/me/keys", t.base))
            .bearer_auth(&tok)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    // x-api-key is only for anthropic proxy routes
    let r =
        t.c.post(format!("{}/v1/openai/chat/completions", t.base))
            .header("x-api-key", &tok)
            .json(&msg())
            .send()
            .await
            .unwrap();
    assert_eq!(r.status(), 401);
}

#[tokio::test]
async fn revoke_denies_pending() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let id = json_of(t.chat(&tok, 0, msg()).await).await["request_id"]
        .as_str()
        .unwrap()
        .to_string();
    let gid = t.user_get("/v1/me/grants").await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        t.c.delete(format!("{}/v1/me/grants/{gid}", t.base))
            .bearer_auth(&t.sess)
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
    assert_eq!(
        t.user_get(&format!("/v1/me/requests/{id}")).await["status"],
        "denied"
    );
    assert_eq!(t.chat(&tok, 0, msg()).await.status(), 401);
}

#[tokio::test]
async fn deleted_key_fails_request_and_refunds() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let id = json_of(t.chat(&tok, 0, msg()).await).await["request_id"]
        .as_str()
        .unwrap()
        .to_string();
    t.c.delete(format!("{}/v1/me/keys/openai", t.base))
        .bearer_auth(&t.sess)
        .send()
        .await
        .unwrap();
    t.user_post(
        &format!("/v1/me/requests/{id}/approve"),
        json!({"policy": {"mode": "budget", "remaining_usd": 1.0}}),
    )
    .await;
    let d = t.wait_status(&id, "failed").await;
    assert_eq!(d["response"]["error"]["type"], "no_key");
    let left = t.user_get("/v1/me/grants").await[0]["policy"]["remaining_usd"]
        .as_f64()
        .unwrap();
    assert!((left - 1.0).abs() < 1e-9, "{left}");
}

#[tokio::test]
async fn replaced_key_is_used_at_send_time() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let id = json_of(t.chat(&tok, 0, msg()).await).await["request_id"]
        .as_str()
        .unwrap()
        .to_string();
    t.user_put(
        "/v1/me/keys/openai",
        json!({"api_key": "sk-test-NEWKEY9999"}),
    )
    .await;
    t.user_post(&format!("/v1/me/requests/{id}/approve"), json!({}))
        .await;
    assert_eq!(
        t.wait_status(&id, "completed").await["response"]["mock_key_last4"],
        "9999"
    );
}

#[tokio::test]
async fn approve_needs_sufficient_allowance() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let big = json!({"model": "gpt-4o", "messages": [{"role": "user", "content": "hi"}], "max_tokens": 1000}); // est ~ $0.010
    let id = json_of(t.chat(&tok, 0, big).await).await["request_id"]
        .as_str()
        .unwrap()
        .to_string();
    let path = format!("/v1/me/requests/{id}/approve");
    for pol in [
        json!({"mode": "budget", "remaining_usd": 0.001}),
        json!({"mode": "count", "remaining_count": 0}),
    ] {
        let r = t.user_post(&path, json!({"policy": pol})).await;
        assert_eq!(r.status(), 400);
        assert_eq!(json_of(r).await["error"]["type"], "insufficient_allowance");
        assert_eq!(
            t.user_get(&format!("/v1/me/requests/{id}")).await["status"],
            "pending"
        );
    }
    assert_eq!(
        t.user_get("/v1/me/grants").await[0]["policy"]["mode"],
        "ask"
    );
    assert_eq!(
        t.user_post(
            &path,
            json!({"policy": {"mode": "budget", "remaining_usd": 0.05}})
        )
        .await
        .status(),
        200
    );
}

#[tokio::test]
async fn policy_validation() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let _ = tok;
    let gid = t.user_get("/v1/me/grants").await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
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
        let r =
            t.c.patch(format!("{}/v1/me/grants/{gid}", t.base))
                .bearer_auth(&t.sess)
                .json(&json!({"policy": pol}))
                .send()
                .await
                .unwrap();
        assert_eq!(r.status(), 400, "{pol}");
    }
}

#[tokio::test]
async fn verified_prices_work_outside_mock_and_haiku_long_prompt_tier() {
    use keyala_server::pricing::{actual_cost, lookup, validate};
    let mut body = json!({"model": "gpt-4o", "messages": [{"role": "user", "content": "x"}]});
    assert!(
        validate("openai", &mut body, false).is_ok(),
        "verified models must not need KEYALA_PRICES_VERIFIED"
    );
    let h = lookup("anthropic", "claude-haiku-5-5").unwrap();
    // 100K input tokens: short tier $0.10 -> 10_000 micro; 100_001: long tier $0.50 -> 50_001 micro (ceil).
    assert_eq!(
        actual_cost(
            "anthropic",
            h,
            &json!({"usage": {"input_tokens": 100_000, "output_tokens": 0}})
        ),
        Some(10_000)
    );
    assert_eq!(
        actual_cost(
            "anthropic",
            h,
            &json!({"usage": {"input_tokens": 100_001, "output_tokens": 0}})
        ),
        Some(50_001)
    );
    assert_eq!(
        actual_cost(
            "anthropic",
            h,
            &json!({"usage": {"input_tokens": 200_000, "output_tokens": 1_000_000}})
        ),
        Some(100_000 + 2_500_000)
    );
}

/// helper: make a pending request on an ask grant and force it to `approved` in the DB (simulates a
/// queued send), returning (request id, grant id).
async fn queued(t: &T, tok: &str) -> (String, String) {
    let id = json_of(t.chat(tok, 0, msg()).await).await["request_id"]
        .as_str()
        .unwrap()
        .to_string();
    let gid = t.user_get("/v1/me/grants").await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    sqlx_exec(
        t,
        &format!("UPDATE requests SET status='approved', reserved_micro=5000 WHERE id='{id}'"),
    )
    .await;
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
    t.c.patch(format!("{}/v1/me/grants/{gid}", t.base))
        .bearer_auth(&t.sess)
        .json(&json!({"policy": {"mode": "budget", "remaining_usd": 0.5}}))
        .send()
        .await
        .unwrap();
    sqlx_exec(&t, &format!("UPDATE requests SET policy_gen=(SELECT policy_gen FROM grants WHERE id='{gid}') WHERE id='{id}'")).await;
    assert_eq!(
        t.c.delete(format!("{}/v1/me/grants/{gid}", t.base))
            .bearer_auth(&t.sess)
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
    send_request(t.state.clone(), id.clone()).await;
    assert_eq!(
        t.user_get(&format!("/v1/me/requests/{id}")).await["status"],
        "denied"
    );
    let rem: i64 = sqlx::query_scalar("SELECT remaining_micro FROM grants WHERE id=?")
        .bind(&gid)
        .fetch_one(&t.state.db)
        .await
        .unwrap();
    assert_eq!(rem, 505_000);
}

#[tokio::test]
async fn settlement_ignores_replaced_policy() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    let (id, gid) = queued(&t, &tok).await; // request reserved under the old policy generation
    t.c.patch(format!("{}/v1/me/grants/{gid}", t.base))
        .bearer_auth(&t.sess)
        .json(&json!({"policy": {"mode": "budget", "remaining_usd": 0.5}}))
        .send()
        .await
        .unwrap();
    send_request(t.state.clone(), id.clone()).await;
    assert_eq!(
        t.user_get(&format!("/v1/me/requests/{id}")).await["status"],
        "completed"
    );
    let rem: i64 = sqlx::query_scalar("SELECT remaining_micro FROM grants WHERE id=?")
        .bind(&gid)
        .fetch_one(&t.state.db)
        .await
        .unwrap();
    assert_eq!(
        rem, 500_000,
        "new allowance untouched by the old reservation's settlement"
    );
}

#[tokio::test]
async fn startup_sends_approved_and_marks_sending_uncertain() {
    let db = std::env::temp_dir().join(format!("mk-test-{}.db", keyala_server::rand_b64(8)));
    let cfg = || Config {
        database_url: format!("sqlite://{}?mode=rwc", db.display()),
        master_key: [7u8; 32],
        dev_login: true,
        vendor_mock: true,
        public_url: "http://x".into(),
        prices_verified: false,
        ..extra()
    };
    let s1 = build_state(cfg()).await.unwrap();
    let r =
        |email: &str| format!("INSERT INTO users (id,email,created_at) VALUES ('u','{email}','t')");
    s1.db.execute(r("a@b.co").as_str()).await.unwrap();
    for q in [
        "INSERT INTO apps (id,name,secret_hash,created_at) VALUES ('a','A','h','t')",
        "INSERT INTO grants (id,user_id,app_id,vendor,mode,created_at) VALUES ('g','u','a','openai','trust','t')",
    ] { s1.db.execute(q).await.unwrap(); }
    let (ct, nonce) = keyala_server::encrypt(
        &[7u8; 32],
        &keyala_server::key_aad("u", "openai"),
        "sk-test-ABCD1234",
    );
    sqlx::query("INSERT INTO api_keys (user_id,vendor,ciphertext,nonce,last4,created_at) VALUES ('u','openai',?,?,'1234','t')").bind(ct).bind(nonce).execute(&s1.db).await.unwrap();
    for (id, st) in [("r1", "approved"), ("r2", "sending")] {
        sqlx::query("INSERT INTO requests (id,grant_id,user_id,app_id,vendor,model,endpoint,status,estimated_micro,payload,created_at,expires_at) VALUES (?, 'g','u','a','openai','gpt-4o-mini','/x',?,10,?,'t','9999')")
            .bind(id).bind(st).bind(msg().to_string()).execute(&s1.db).await.unwrap();
    }
    let s2 = build_state(cfg()).await.unwrap();
    for _ in 0..100 {
        let st: String = sqlx::query_scalar("SELECT status FROM requests WHERE id='r1'")
            .fetch_one(&s2.db)
            .await
            .unwrap();
        if st == "completed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let st = |id: &'static str| {
        sqlx::query_scalar::<_, String>("SELECT status FROM requests WHERE id=?")
            .bind(id)
            .fetch_one(&s2.db)
    };
    assert_eq!(st("r1").await.unwrap(), "completed");
    assert_eq!(st("r2").await.unwrap(), "uncertain");
}

#[tokio::test]
async fn anthropic_x_api_key_and_sdk_path() {
    let t = T::new().await;
    t.user_put(
        "/v1/me/keys/anthropic",
        json!({"api_key": "sk-ant-test-0000"}),
    )
    .await;
    let tok = t.grant("anthropic", json!({"mode": "trust"})).await;
    let body = json!({"model": "claude-haiku-5-5", "max_tokens": 10, "messages": [{"role":"user","content":"x"}]});
    for path in ["/v1/anthropic/messages", "/v1/anthropic/v1/messages"] {
        let r =
            t.c.post(format!("{}{path}", t.base))
                .header("x-api-key", &tok)
                .json(&body)
                .send()
                .await
                .unwrap();
        assert_eq!(r.status(), 200, "{path}");
    }
}

#[tokio::test]
async fn missing_usage_keeps_actual_null_and_charges_estimate() {
    let t = T::new().await;
    let tok = t
        .grant("openai", json!({"mode": "budget", "remaining_usd": 1.0}))
        .await;
    let body = json!({"model": "gpt-4o", "messages": [{"role": "user", "content": "KEYALA_MOCK_NOUSAGE"}], "max_tokens": 1000});
    let r = t.chat(&tok, 0, body).await;
    assert_eq!(r.status(), 200);
    let id = r.headers()["x-keyala-request-id"]
        .to_str()
        .unwrap()
        .to_string();
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
async fn apple_server(
    kid_served: &'static str,
) -> (
    String,
    Client,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
) {
    let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let h2 = hits.clone();
    let jwks = json!({"keys": [{"kty": "RSA", "kid": kid_served, "use": "sig", "alg": "RS256", "n": RSA_N, "e": "AQAB"}]});
    let app = axum::Router::new().route(
        "/keys",
        axum::routing::get(move || {
            h2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let j = jwks.clone();
            async move { axum::Json(j) }
        }),
    );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let jurl = format!("http://{}/keys", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    let db = std::env::temp_dir().join(format!("mk-test-{}.db", keyala_server::rand_b64(8)));
    let cfg = Config {
        database_url: format!("sqlite://{}?mode=rwc", db.display()),
        master_key: [7u8; 32],
        vendor_mock: true,
        google_jwks_url: jurl.clone(),
        google_client_ids: vec!["gid.apps.googleusercontent.com".into()],
        apple_jwks_url: jurl,
        ..extra()
    };
    let state = build_state(cfg).await.unwrap();
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    let app = router(state);
    tokio::spawn(async move {
        axum::serve(l, app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .unwrap()
    });
    (base, Client::new(), hits)
}

fn apple_token(
    kid: &str,
    sub: &str,
    aud: &str,
    iss: &str,
    exp_delta: i64,
    email: Option<&str>,
) -> String {
    let mut h = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    h.kid = Some(kid.into());
    let exp = chrono_now() + exp_delta;
    let mut c = json!({"iss": iss, "aud": aud, "sub": sub, "exp": exp, "iat": exp - 100});
    if let Some(e) = email {
        c["email"] = e.into();
    }
    jsonwebtoken::encode(
        &h,
        &c,
        &jsonwebtoken::EncodingKey::from_rsa_pem(RSA_PEM.as_bytes()).unwrap(),
    )
    .unwrap()
}
fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[tokio::test]
async fn apple_login_verifies_token() {
    let (base, c, hits) = apple_server("k1").await;
    let iss = "https://appleid.apple.com";
    let login = |tok: String| {
        let (c, base) = (c.clone(), base.clone());
        async move {
            c.post(format!("{base}/v1/auth/apple"))
                .json(&json!({"id_token": tok}))
                .send()
                .await
                .unwrap()
        }
    };
    let ok = login(apple_token(
        "k1",
        "001.abc",
        "app.keyala.ios",
        iss,
        600,
        Some("me@icloud.com"),
    ))
    .await;
    assert_eq!(ok.status(), 200);
    let j = json_of(ok).await;
    assert_eq!(j["user"]["email"], "me@icloud.com");
    let sess = j["session_token"].as_str().unwrap();
    assert_eq!(
        c.get(format!("{base}/v1/me/keys"))
            .bearer_auth(sess)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    // same sub => same user, even if the email claim changes
    let j2 = json_of(
        login(apple_token(
            "k1",
            "001.abc",
            "app.keyala.ios",
            iss,
            600,
            Some("other@icloud.com"),
        ))
        .await,
    )
    .await;
    assert_eq!(j2["user"]["id"], j["user"]["id"]);
    // JWKS cached across logins
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
    // a different sub with a taken email must not attach to the existing account
    let j3 = json_of(
        login(apple_token(
            "k1",
            "001.zzz",
            "app.keyala.ios",
            iss,
            600,
            Some("me@icloud.com"),
        ))
        .await,
    )
    .await;
    assert_ne!(j3["user"]["id"], j["user"]["id"]);
    // rejections
    for (name, tok) in [
        (
            "wrong aud",
            apple_token("k1", "s", "com.evil.app", iss, 600, None),
        ),
        (
            "wrong iss",
            apple_token(
                "k1",
                "s",
                "app.keyala.ios",
                "https://evil.example",
                600,
                None,
            ),
        ),
        (
            "expired",
            apple_token("k1", "s", "app.keyala.ios", iss, -3600, None),
        ),
        (
            "unknown kid",
            apple_token("nope", "s", "app.keyala.ios", iss, 600, None),
        ),
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
        let chk = keyala_server::pricing::validate("openai", &mut norm, true).unwrap();
        let sent_bytes = norm.to_string().len() as i64;
        let (ip, op) = chk.model.rates(sent_bytes);
        let ceiling =
            keyala_server::pricing::cost(sent_bytes, ip) + keyala_server::pricing::cost(50, op);
        assert!(
            chk.estimate_micro >= ceiling,
            "case {i}: {} < {ceiling}",
            chk.estimate_micro
        );
        // mock reports prompt_tokens = payload bytes (worst case): actual must never exceed the reservation
        let r = t.chat(&tok, 0, body).await;
        assert_eq!(r.status(), 200);
        let rid = r.headers()["x-keyala-request-id"]
            .to_str()
            .unwrap()
            .to_string();
        let b = json_of(r).await;
        assert_eq!(b["usage"]["prompt_tokens"], sent_bytes);
        let d = t.user_get(&format!("/v1/me/requests/{rid}")).await;
        assert!(
            d["actual_usd"].as_f64().unwrap() <= d["estimated_usd"].as_f64().unwrap(),
            "case {i}"
        );
    }
}

#[tokio::test]
async fn idempotency_key_dedupes_and_conflicts() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    let post = |key: &'static str, body: Value| {
        t.c.post(format!("{}/v1/openai/chat/completions", t.base))
            .bearer_auth(&tok)
            .header("Idempotency-Key", key)
            .header("X-Keyala-Wait", "0")
            .json(&body)
            .send()
    };
    let r1 = post("k1", msg()).await.unwrap();
    assert_eq!(r1.status(), 200);
    let id1 = r1.headers()["x-keyala-request-id"]
        .to_str()
        .unwrap()
        .to_string();
    let r2 = post("k1", msg()).await.unwrap();
    assert_eq!(r2.status(), 200);
    assert_eq!(r2.headers()["x-keyala-request-id"].to_str().unwrap(), id1);
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
    assert_eq!(
        t.user_get("/v1/me/requests")
            .await
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn concurrent_duplicates_send_once() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    let calls = (0..5).map(|_| {
        t.c.post(format!("{}/v1/openai/chat/completions", t.base))
            .bearer_auth(&tok)
            .header("Idempotency-Key", "same")
            .json(&msg())
            .send()
    });
    let rs = futures_join(calls).await;
    let mut ids = std::collections::HashSet::new();
    for r in rs {
        let r = r.unwrap();
        assert_eq!(r.status(), 200);
        ids.insert(
            r.headers()["x-keyala-request-id"]
                .to_str()
                .unwrap()
                .to_string(),
        );
    }
    assert_eq!(ids.len(), 1);
    assert_eq!(t.user_get("/v1/me/grants").await[0]["request_count"], 1);
    assert_eq!(
        t.user_get("/v1/me/requests")
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

async fn futures_join<F: std::future::Future + Send + 'static>(
    fs: impl Iterator<Item = F>,
) -> Vec<F::Output>
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
    let post = || {
        t.c.post(format!("{}/v1/openai/chat/completions", t.base))
            .bearer_auth(&tok)
            .header("Idempotency-Key", "p1")
            .header("X-Keyala-Wait", "0")
            .json(&msg())
            .send()
    };
    let a = json_of(post().await.unwrap()).await;
    let b = json_of(post().await.unwrap()).await;
    assert_eq!(a["status"], "pending");
    assert_eq!(a["request_id"], b["request_id"]);
    assert_eq!(
        t.user_get("/v1/me/requests?status=pending")
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

// ---------- public (browser) clients ----------

const ORIGIN: &str = "https://demo.example.com";

#[test]
fn origin_validation() {
    for ok in [
        "https://a.github.io",
        "https://x.com:8443",
        "http://localhost:3001",
        "http://127.0.0.1:3001",
        "http://[::1]:5000",
    ] {
        assert!(origin_ok(ok), "{ok}");
    }
    for bad in [
        "http://example.com",
        "https://a.io/",
        "https://a.io/path",
        "https://A.io",
        "ftp://a.io",
        "https://",
        "http://localhost.evil.com",
        "null",
        "",
    ] {
        assert!(!origin_ok(bad), "{bad}");
    }
}

#[tokio::test]
async fn public_client_connect_and_cors() {
    let t = T::new().await;
    let b = &t.base;
    // invalid origins rejected at registration
    for o in [
        json!([]),
        json!(["http://example.com"]),
        json!(["https://a.io/x"]),
    ] {
        let r =
            t.c.post(format!("{b}/v1/apps"))
                .json(&json!({"name": "X", "allowed_origins": o}))
                .send()
                .await
                .unwrap();
        assert_eq!(r.status(), 400, "{o}");
    }
    let app = json_of(
        t.c.post(format!("{b}/v1/apps"))
            .json(&json!({"name": "Static", "allowed_origins": [ORIGIN]}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let app_id = app["app_id"].as_str().unwrap();
    let connect = |origin: Option<&str>, id: &str| {
        let mut r =
            t.c.post(format!("{b}/v1/connect"))
                .json(&json!({"vendor": "openai", "app_id": id}));
        if let Some(o) = origin {
            r = r.header("origin", o);
        }
        r.send()
    };
    // wrong or missing origin, unknown app
    for (o, id) in [
        (Some("https://evil.example"), app_id),
        (None, app_id),
        (Some(ORIGIN), "app_nope"),
    ] {
        let r = connect(o, id).await.unwrap();
        assert_eq!(r.status(), 403);
        assert_eq!(json_of(r).await["error"]["type"], "origin_not_allowed");
    }
    let r = connect(Some(ORIGIN), app_id).await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["access-control-allow-origin"], "*");
    let c = json_of(r).await;
    let (code, cs) = (
        c["connect_code"].as_str().unwrap(),
        c["connect_secret"].as_str().unwrap(),
    );
    assert!(cs.starts_with("keyala_cs_"));
    let poll = |tok: &str| {
        t.c.get(format!("{b}/v1/connect/{code}"))
            .bearer_auth(tok)
            .send()
    };
    // the code alone (or another code's secret) never claims the grant
    assert_eq!(
        t.c.get(format!("{b}/v1/connect/{code}"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(poll("keyala_cs_wrong").await.unwrap().status(), 404);
    assert_eq!(json_of(poll(cs).await.unwrap()).await["status"], "pending");
    // the phone sees where the connect came from
    let prev = t.user_get(&format!("/v1/me/connect/{code}")).await;
    assert_eq!(prev["origin"], ORIGIN);
    assert_eq!(prev["app"]["name"], "Static");
    assert_eq!(prev["requires_ask"], true);
    let denied_wide = t
        .user_post(
            &format!("/v1/me/connect/{code}/approve"),
            json!({"policy": {"mode": "trust"}}),
        )
        .await;
    assert_eq!(denied_wide.status(), 400);
    assert_eq!(
        json_of(denied_wide).await["error"]["type"],
        "policy_not_allowed_yet"
    );
    let first_grant = json_of(
        t.user_post(
            &format!("/v1/me/connect/{code}/approve"),
            json!({"policy": {"mode": "ask"}}),
        )
        .await,
    )
    .await;
    assert_eq!(first_grant["policy"]["mode"], "ask");
    let r =
        t.c.patch(format!(
            "{b}/v1/me/grants/{}",
            first_grant["id"].as_str().unwrap()
        ))
        .bearer_auth(&t.sess)
        .json(&json!({"policy": {"mode": "trust"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let p = json_of(poll(cs).await.unwrap()).await;
    let grant = p["grant_token"].as_str().unwrap();
    assert!(
        json_of(poll(cs).await.unwrap())
            .await
            .get("grant_token")
            .is_none(),
        "token is shown once"
    );
    // the browser can call the proxy and read the request id
    let r =
        t.c.post(format!("{b}/v1/openai/chat/completions"))
            .header("origin", ORIGIN)
            .bearer_auth(grant)
            .json(&json!({"model": "gpt-4o-mini", "messages": [{"role": "user", "content": "hi"}]}))
            .send()
            .await
            .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["access-control-allow-origin"], "*");
    assert_eq!(
        r.headers()["access-control-expose-headers"],
        "x-keyala-request-id"
    );
    // preflight on developer routes; none on user routes
    let pre = |p: &str| {
        t.c.request(reqwest::Method::OPTIONS, format!("{b}{p}"))
            .header("origin", ORIGIN)
            .header("access-control-request-method", "POST")
            .header(
                "access-control-request-headers",
                "authorization,content-type,idempotency-key,x-keyala-wait",
            )
            .send()
    };
    let r = pre("/v1/openai/chat/completions").await.unwrap();
    assert_eq!(r.status(), 204);
    assert_eq!(
        r.headers()["access-control-allow-headers"],
        "authorization,content-type,idempotency-key,x-keyala-wait"
    );
    assert_eq!(pre("/v1/connect").await.unwrap().status(), 204);
    let r = pre("/v1/me/keys").await.unwrap();
    assert!(r.headers().get("access-control-allow-origin").is_none());
    // server apps are unchanged: no connect_secret, and no Origin needed
    let srv = json_of(
        t.c.post(format!("{b}/v1/apps"))
            .json(&json!({"name": "Server"}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let c = json_of(
        t.c.post(format!("{b}/v1/connect"))
            .bearer_auth(srv["app_secret"].as_str().unwrap())
            .json(&json!({"vendor": "openai"}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert!(c.get("connect_secret").is_none() && c["connect_code"].is_string());
    // a server app cannot be used from a browser by id
    let r = connect(Some(ORIGIN), srv["app_id"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(r.status(), 403);
}

#[tokio::test]
async fn declared_public_apps_are_seeded_and_updated() {
    let app = |name: &str| PublicApp {
        id: "app_resume_tailor".into(),
        name: name.into(),
        homepage: None,
        allowed_origins: vec![ORIGIN.into()],
    };
    let (b, c, state) = server_with(vec![app("Resume Tailor")]).await;
    let r = c
        .post(format!("{b}/v1/connect"))
        .header("origin", ORIGIN)
        .json(&json!({"vendor": "anthropic", "app_id": "app_resume_tailor"}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    // a restart with a changed declaration updates the row instead of failing
    let mut cfg = Config {
        database_url: state.cfg.database_url.clone(),
        master_key: [7u8; 32],
        public_apps: vec![app("Renamed")],
        ..extra()
    };
    cfg.vendor_mock = true;
    let s2 = build_state(cfg).await.unwrap();
    let name: String = sqlx::query_scalar("SELECT name FROM apps WHERE id='app_resume_tailor'")
        .fetch_one(&s2.db)
        .await
        .unwrap();
    assert_eq!(name, "Renamed");
}

#[tokio::test]
async fn public_connect_is_rate_limited() {
    let app = PublicApp {
        id: "app_rl".into(),
        name: "RL".into(),
        homepage: None,
        allowed_origins: vec![ORIGIN.into()],
    };
    let (b, c, _) = server_with(vec![app]).await;
    let mut last = 0;
    for _ in 0..21 {
        last = c
            .post(format!("{b}/v1/connect"))
            .header("origin", ORIGIN)
            .json(&json!({"vendor": "openai", "app_id": "app_rl"}))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16();
    }
    assert_eq!(last, 429);
}

#[tokio::test]
async fn developer_app_info_rotation_and_report() {
    let t = T::new().await;
    let r =
        t.c.request(reqwest::Method::OPTIONS, format!("{}/v1/apps", t.base))
            .header("origin", "https://x.dev")
            .header("access-control-request-method", "POST")
            .send()
            .await
            .unwrap();
    assert_eq!(r.headers()["access-control-allow-origin"], "*");
    let app = json_of(
        t.c.post(format!("{}/v1/apps", t.base))
            .json(&json!({"name": "Reports", "allowed_origins": ["https://x.dev"]}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let secret = app["app_secret"].as_str().unwrap().to_string();
    let info = json_of(
        t.c.get(format!("{}/v1/app", t.base))
            .bearer_auth(&secret)
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        (info["name"].as_str(), info["allowed_origins"][0].as_str()),
        (Some("Reports"), Some("https://x.dev"))
    );
    // empty report: every day present, zeros
    let rep = json_of(
        t.c.get(format!("{}/v1/app/report?days=7", t.base))
            .bearer_auth(&secret)
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(rep["daily"].as_array().unwrap().len(), 7);
    assert_eq!(rep["totals"]["requests"], 0);
    assert_eq!(
        t.c.get(format!("{}/v1/app/report?days=0", t.base))
            .bearer_auth(&secret)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );

    // connect approved from a phone in Japan, then 2 completed calls and 1 denied
    let conn = json_of(
        t.c.post(format!("{}/v1/connect", t.base))
            .bearer_auth(&secret)
            .json(&json!({"vendor": "openai"}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let code = conn["connect_code"].as_str().unwrap();
    let r =
        t.c.post(format!("{}/v1/me/connect/{code}/approve", t.base))
            .bearer_auth(&t.sess)
            .header("cf-ipcountry", "jp")
            .json(&json!({"policy": {"mode": "ask"}}))
            .send()
            .await
            .unwrap();
    assert_eq!(r.status(), 200);
    let tok = json_of(
        t.c.get(format!("{}/v1/connect/{code}", t.base))
            .bearer_auth(&secret)
            .send()
            .await
            .unwrap(),
    )
    .await["grant_token"]
        .as_str()
        .unwrap()
        .to_string();
    for decision in ["approve", "approve", "deny"] {
        assert_eq!(t.chat(&tok, 0, msg()).await.status(), 202);
        let id = t.pending().await;
        assert_eq!(
            t.user_post(&format!("/v1/me/requests/{id}/{decision}"), json!({}))
                .await
                .status(),
            200
        );
        t.wait_status(
            &id,
            if decision == "approve" {
                "completed"
            } else {
                "denied"
            },
        )
        .await;
    }

    // rotation: old secret stops working
    let new = json_of(
        t.c.post(format!("{}/v1/app/secret", t.base))
            .bearer_auth(&secret)
            .send()
            .await
            .unwrap(),
    )
    .await["app_secret"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        t.c.get(format!("{}/v1/app", t.base))
            .bearer_auth(&secret)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );

    let rep = json_of(
        t.c.get(format!("{}/v1/app/report", t.base))
            .bearer_auth(&new)
            .send()
            .await
            .unwrap(),
    )
    .await;
    let tot = &rep["totals"];
    assert_eq!(
        (
            tot["requests"].as_i64(),
            tot["completed"].as_i64(),
            tot["denied"].as_i64(),
            tot["users"].as_i64(),
            tot["new_users"].as_i64()
        ),
        (Some(3), Some(2), Some(1), Some(1), Some(1))
    );
    assert!(
        tot["cost_usd"].as_f64().unwrap() > 0.0
            && tot["input_tokens"].as_i64().unwrap() > 0
            && tot["output_tokens"].as_i64().unwrap() > 0
    );
    assert_eq!(tot["unmetered"], 0);
    assert_eq!(rep["daily"].as_array().unwrap().len(), 30);
    assert_eq!(rep["daily"][29]["requests"], 3);
    let hours = rep["hours_utc"].as_array().unwrap();
    assert_eq!((hours.len(), hours[0].as_array().unwrap().len()), (7, 24));
    let now = chrono::Utc::now();
    use chrono::{Datelike, Timelike};
    let cell = |t: chrono::DateTime<chrono::Utc>| {
        hours[t.weekday().num_days_from_sunday() as usize][t.hour() as usize]
            .as_i64()
            .unwrap()
    };
    assert_eq!(
        hours
            .iter()
            .flat_map(|r| r.as_array().unwrap())
            .map(|v| v.as_i64().unwrap())
            .sum::<i64>(),
        3
    );
    let earlier = now - chrono::Duration::minutes(5); // the test may straddle an hour boundary
    assert_eq!(
        cell(now)
            + if earlier.hour() != now.hour() {
                cell(earlier)
            } else {
                0
            },
        3,
        "calls land in the current UTC hour"
    );
    assert_eq!(rep["models"][0]["model"], "gpt-4o-mini");
    assert_eq!(rep["models"][0]["completed"], 2);
    assert_eq!(
        rep["countries"],
        json!([{"country": "JP", "users": 1, "requests": 3, "cost_usd": tot["cost_usd"]}])
    );
    let s = rep.to_string();
    assert!(
        !s.contains(&t.sess) && !s.contains("a@b.co") && !s.contains("\"hi\""),
        "report leaks user data"
    );

    // another app sees none of it
    let other = json_of(
        t.c.post(format!("{}/v1/apps", t.base))
            .json(&json!({"name": "Other"}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let rep = json_of(
        t.c.get(format!("{}/v1/app/report", t.base))
            .bearer_auth(other["app_secret"].as_str().unwrap())
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(rep["totals"]["requests"], 0);
    assert_eq!(rep["countries"], json!([]));
}

#[tokio::test]
async fn app_secret_survives_database_reset_and_rotation_wins() {
    let t = T::new().await;
    let app = json_of(
        t.c.post(format!("{}/v1/apps", t.base))
            .json(&json!({"name": "Durable", "allowed_origins": ["https://d.dev"]}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let old = app["app_secret"].as_str().unwrap().to_string();
    let get = |s: &str| t.c.get(format!("{}/v1/app", t.base)).bearer_auth(s).send();
    let wipe = || async {
        sqlx_query(&t.state, "DELETE FROM connect_codes WHERE app_id IN (SELECT id FROM apps WHERE name='Durable'); DELETE FROM apps WHERE name='Durable'").await
    };

    // free-plan reset: the app row is gone, the token brings it back with its settings
    wipe().await;
    let info = json_of(get(&old).await.unwrap()).await;
    assert_eq!(
        (
            info["app_id"].as_str(),
            info["name"].as_str(),
            info["allowed_origins"][0].as_str()
        ),
        (
            app["app_id"].as_str(),
            Some("Durable"),
            Some("https://d.dev")
        )
    );
    // and it can connect again
    assert_eq!(
        t.c.post(format!("{}/v1/connect", t.base))
            .bearer_auth(&old)
            .json(&json!({"vendor": "openai"}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    // rotate, reset, then the leaked old token arrives first: the newer token still takes over
    let new = json_of(
        t.c.post(format!("{}/v1/app/secret", t.base))
            .bearer_auth(&old)
            .send()
            .await
            .unwrap(),
    )
    .await["app_secret"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(get(&old).await.unwrap().status(), 401);
    wipe().await;
    assert_eq!(get(&old).await.unwrap().status(), 200);
    assert_eq!(get(&new).await.unwrap().status(), 200);
    assert_eq!(get(&old).await.unwrap().status(), 401);

    // tampered or foreign tokens are refused
    let mut bad = new.clone().into_bytes();
    let i = bad.len() - 5;
    bad[i] = if bad[i] == b'A' { b'B' } else { b'A' };
    assert_eq!(
        get(&String::from_utf8(bad).unwrap())
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        get("keyala_app_not-a-real-token").await.unwrap().status(),
        401
    );
}

#[tokio::test]
async fn password_login_single_operator_account() {
    use sha2::{Digest, Sha256};
    // no operator account configured: user web passwords still work, so it answers 401 rather than 404
    let (base, c, _) = server().await;
    let r = c
        .post(format!("{base}/v1/auth/password"))
        .json(&json!({"username": "qa", "password": "x"}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);

    let pw = "correct-horse-battery-staple-9f3a";
    let (base, c, _) = serve(Config {
        password_login: Some(("staging-qa".into(), Sha256::digest(pw).into())),
        dev_login: false,
        ..test_cfg()
    })
    .await;
    let login = |u: &'static str, p: &'static str| {
        c.post(format!("{base}/v1/auth/password"))
            .json(&json!({"username": u, "password": p}))
            .send()
    };
    for (u, p) in [("staging-qa", "wrong"), ("other", pw), ("staging-qa", "")] {
        let r = login(u, p).await.unwrap();
        assert_eq!(r.status(), 401);
        assert_eq!(json_of(r).await["error"]["type"], "invalid_credentials");
    }
    let a = json_of(login("Staging-QA", pw).await.unwrap()).await;
    let b = json_of(login("staging-qa", pw).await.unwrap()).await;
    assert_eq!(a["user"]["email"], "staging-qa@password.invalid");
    assert_eq!(a["user"]["id"], b["user"]["id"]);
    let sess = a["session_token"].as_str().unwrap();
    assert_eq!(
        c.get(format!("{base}/v1/me/keys"))
            .bearer_auth(sess)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    // dev login stays off; 10 attempts per 10 min per address (5 used above)
    assert_eq!(
        c.post(format!("{base}/v1/auth/dev"))
            .json(&json!({"email": "staging-qa@password.invalid"}))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    for _ in 0..5 {
        login("staging-qa", "wrong").await.unwrap();
    }
    assert_eq!(login("staging-qa", pw).await.unwrap().status(), 429);
}

#[tokio::test]
async fn key_shape_and_vendor_check() {
    let t = T::new().await;
    for (v, k) in [
        ("openai", "sk-ant-api03-wrongvendor"),
        ("openai", "sk-admin-orgwide1234"),
        ("anthropic", "sk-proj-wrongvendor1"),
        ("anthropic", "sk-ant-admin01-orgwide"),
        ("openai", "sk-proj-KEYALA_MOCK_INVALID"),
    ] {
        let r = t
            .user_put(&format!("/v1/me/keys/{v}"), json!({"api_key": k}))
            .await;
        assert_eq!(r.status(), 400, "{v} {k}");
        assert_eq!(json_of(r).await["error"]["type"], "invalid_key");
    }
    let keys = t.user_get("/v1/me/keys").await;
    assert_eq!(
        keys[0]["last4"], "1234",
        "rejected keys never replace the stored one"
    );
    // 403 may be a restricted key without the models-list permission: saved, but not claimed as checked
    let k = json_of(
        t.user_put(
            "/v1/me/keys/openai",
            json!({"api_key": "sk-proj-KEYALA_MOCK_RESTRICTED9"}),
        )
        .await,
    )
    .await;
    assert_eq!(
        (k["last4"].as_str(), k["checked"].as_bool()),
        (Some("TED9"), Some(false))
    );
    let k = json_of(
        t.user_put(
            "/v1/me/keys/openai",
            json!({"api_key": "sk-proj-fine00001234"}),
        )
        .await,
    )
    .await;
    assert_eq!(k["checked"], true);
}

#[tokio::test]
async fn key_enrollment_from_a_computer() {
    let t = T::new().await;
    let other = json_of(
        t.c.post(format!("{}/v1/auth/dev", t.base))
            .json(&json!({"email": "x@y.co"}))
            .send()
            .await
            .unwrap(),
    )
    .await["session_token"]
        .as_str()
        .unwrap()
        .to_string();
    let start = || async {
        json_of(
            t.c.post(format!("{}/v1/enroll", t.base))
                .json(&json!({"vendor": "anthropic"}))
                .send()
                .await
                .unwrap(),
        )
        .await
    };
    let e = start().await;
    let (id, sec) = (
        e["enroll_id"].as_str().unwrap().to_string(),
        e["enroll_secret"].as_str().unwrap().to_string(),
    );
    assert!(
        sec.starts_with("keyala_es_")
            && e["enroll_url"]
                .as_str()
                .unwrap()
                .ends_with(&format!("/enroll/{id}"))
    );
    let b = |p: &str| format!("{}{p}", t.base);
    let poll = || async {
        json_of(
            t.c.get(b(&format!("/v1/enroll/{id}")))
                .bearer_auth(&sec)
                .send()
                .await
                .unwrap(),
        )
        .await
    };
    let upload = |k: &'static str| {
        t.c.post(b(&format!("/v1/enroll/{id}/key")))
            .bearer_auth(&sec)
            .json(&json!({"api_key": k}))
            .send()
    };
    assert_eq!(poll().await["status"], "pending");
    // the browser cannot upload before a phone claims it, and needs the secret
    assert_eq!(
        upload("sk-ant-api03-early0000").await.unwrap().status(),
        409
    );
    assert_eq!(
        t.c.get(b(&format!("/v1/enroll/{id}")))
            .bearer_auth("keyala_es_wrong")
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    // phone: preview + claim; a second account can no longer see or claim it
    assert_eq!(
        t.user_get(&format!("/v1/me/enroll/{id}")).await["vendor"],
        "anthropic"
    );
    assert_eq!(
        t.user_post(&format!("/v1/me/enroll/{id}/claim"), json!({}))
            .await
            .status(),
        200
    );
    assert_eq!(
        t.c.post(b(&format!("/v1/me/enroll/{id}/claim")))
            .bearer_auth(&other)
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    assert_eq!(
        t.c.get(b(&format!("/v1/me/enroll/{id}")))
            .bearer_auth(&other)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    // confirm before upload is refused; a wrong-vendor key is refused at upload
    assert_eq!(
        t.user_post(&format!("/v1/me/enroll/{id}/confirm"), json!({}))
            .await
            .status(),
        409
    );
    assert_eq!(upload("sk-proj-openai000000").await.unwrap().status(), 400);
    let u = json_of(upload("sk-ant-api03-firstAAAA").await.unwrap()).await;
    assert_eq!(
        (u["status"].as_str(), u["last4"].as_str()),
        (Some("submitted"), Some("AAAA"))
    );
    // the candidate is immutable once submitted: what the phone reviews is what it saves
    assert_eq!(
        upload("sk-ant-api03-secondBBBB").await.unwrap().status(),
        409
    );
    assert_eq!(
        t.user_get(&format!("/v1/me/enroll/{id}")).await["last4"],
        "AAAA"
    );
    assert_eq!(
        t.c.post(b(&format!("/v1/me/enroll/{id}/confirm")))
            .bearer_auth(&other)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let k = json_of(
        t.user_post(&format!("/v1/me/enroll/{id}/confirm"), json!({}))
            .await,
    )
    .await;
    assert_eq!(
        (k["vendor"].as_str(), k["last4"].as_str()),
        (Some("anthropic"), Some("AAAA"))
    );
    assert_eq!(poll().await["status"], "saved");
    // single use: no second confirm, no further upload, ciphertext wiped
    assert_eq!(
        t.user_post(&format!("/v1/me/enroll/{id}/confirm"), json!({}))
            .await
            .status(),
        409
    );
    assert_eq!(
        upload("sk-ant-api03-thirdCCCC").await.unwrap().status(),
        409
    );
    let ct: Option<Vec<u8>> = sqlx::query_scalar("SELECT ciphertext FROM enrollments WHERE id=?")
        .bind(&id)
        .fetch_one(&t.state.db)
        .await
        .unwrap();
    assert!(ct.is_none());
    // the saved key is the one the proxy uses
    let keys = t.user_get("/v1/me/keys").await;
    assert!(keys
        .as_array()
        .unwrap()
        .iter()
        .any(|k| k["vendor"] == "anthropic" && k["last4"] == "AAAA"));

    // cancel on the phone wipes an uploaded key
    let e = start().await;
    let (id2, sec2) = (
        e["enroll_id"].as_str().unwrap().to_string(),
        e["enroll_secret"].as_str().unwrap().to_string(),
    );
    t.user_post(&format!("/v1/me/enroll/{id2}/claim"), json!({}))
        .await;
    t.c.post(b(&format!("/v1/enroll/{id2}/key")))
        .bearer_auth(&sec2)
        .json(&json!({"api_key": "sk-ant-api03-cancelDDDD"}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        t.user_post(&format!("/v1/me/enroll/{id2}/cancel"), json!({}))
            .await
            .status(),
        204
    );
    assert_eq!(
        t.user_post(&format!("/v1/me/enroll/{id2}/confirm"), json!({}))
            .await
            .status(),
        409
    );
    assert_eq!(
        t.user_get("/v1/me/keys")
            .await
            .as_array()
            .unwrap()
            .iter()
            .find(|k| k["vendor"] == "anthropic")
            .unwrap()["last4"],
        "AAAA"
    );

    // pages, QR, universal links; no CORS on enrollment
    let page = t.c.get(b("/add-key")).send().await.unwrap();
    assert!(page.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .contains("frame-ancestors 'none'"));
    assert!(page.headers().get("access-control-allow-origin").is_none());
    let qr =
        t.c.get(b(&format!("/enroll/{id}/qr.svg")))
            .send()
            .await
            .unwrap();
    assert_eq!(qr.headers()["content-type"], "image/svg+xml");
    assert_eq!(
        t.c.get(b("/connect/KY-0000/qr.svg"))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let aasa = json_of(
        t.c.get(b("/.well-known/apple-app-site-association"))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        aasa["applinks"]["details"][0]["appIDs"][0],
        "TEAMID.app.keyala.ios"
    );
    let pre =
        t.c.request(reqwest::Method::OPTIONS, b("/v1/enroll"))
            .header("origin", "https://evil.example")
            .send()
            .await
            .unwrap();
    assert!(pre.headers().get("access-control-allow-origin").is_none());
}

#[tokio::test]
async fn openrouter_one_tap_and_proxy() {
    let t = T::new().await;
    // OAuth start needs a session; in mock mode the consent page is the server's own stand-in
    assert_eq!(
        t.c.post(format!("{}/v1/me/oauth/openrouter", t.base))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        t.user_post("/v1/me/oauth/openrouter", json!({"limit_usd": -1}))
            .await
            .status(),
        400
    );
    let s = json_of(
        t.user_post("/v1/me/oauth/openrouter", json!({"limit_usd": 5}))
            .await,
    )
    .await;
    let flow = s["flow_id"].as_str().unwrap().to_string();
    assert!(flow.starts_with("of_"));
    assert_eq!(
        t.user_get(&format!("/v1/me/oauth/openrouter/flows/{flow}"))
            .await["status"],
        "pending"
    );
    // another account cannot see or cancel the flow
    let other = json_of(
        t.c.post(format!("{}/v1/auth/dev", t.base))
            .json(&json!({"email": "z@z.co"}))
            .send()
            .await
            .unwrap(),
    )
    .await["session_token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        t.c.get(format!("{}/v1/me/oauth/openrouter/flows/{flow}", t.base))
            .bearer_auth(&other)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        t.c.post(format!(
            "{}/v1/me/oauth/openrouter/flows/{flow}/cancel",
            t.base
        ))
        .bearer_auth(&other)
        .send()
        .await
        .unwrap()
        .status(),
        404
    );
    let auth = s["auth_url"].as_str().unwrap().to_string();
    // test public_url is http://mk.test: rebase onto the real listener and follow by hand
    let local = auth.replace("http://mk.test", &t.base);
    let nr = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let r = nr.get(&local).send().await.unwrap();
    assert_eq!(r.status(), 303);
    let cb = r.headers()["location"]
        .to_str()
        .unwrap()
        .replace("http://mk.test", &t.base);
    let page = nr.get(&cb).send().await.unwrap().text().await.unwrap();
    assert!(
        page.contains(&format!("keyala://oauth/openrouter?status=ok&flow={flow}")),
        "{page}"
    );
    assert!(
        !page.contains("sk-or-"),
        "the key never reaches the browser"
    );
    let f = t
        .user_get(&format!("/v1/me/oauth/openrouter/flows/{flow}"))
        .await;
    assert_eq!(f["status"], "saved");
    let keys = t.user_get("/v1/me/keys").await;
    assert!(keys
        .as_array()
        .unwrap()
        .iter()
        .any(|k| k["vendor"] == "openrouter" && k["last4"] == f["last4"]));
    // the state is single use (a replay neither exchanges again nor changes the flow); no state = declined
    assert!(nr
        .get(&cb)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap()
        .contains("status=error&reason=expired"));
    assert_eq!(
        t.user_get(&format!("/v1/me/oauth/openrouter/flows/{flow}"))
            .await["status"],
        "saved"
    );
    assert!(nr
        .get(format!("{}/oauth/openrouter/callback?code=x", t.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap()
        .contains("reason=cancelled"));
    // cancel works only while pending, and a cancelled flow's callback does nothing
    let s2 = json_of(t.user_post("/v1/me/oauth/openrouter", json!({})).await).await;
    let flow2 = s2["flow_id"].as_str().unwrap().to_string();
    assert_eq!(
        json_of(
            t.user_post(
                &format!("/v1/me/oauth/openrouter/flows/{flow2}/cancel"),
                json!({})
            )
            .await
        )
        .await["status"],
        "cancelled"
    );
    let r = nr
        .get(
            s2["auth_url"]
                .as_str()
                .unwrap()
                .replace("http://mk.test", &t.base),
        )
        .send()
        .await
        .unwrap();
    let cb2 = r.headers()["location"]
        .to_str()
        .unwrap()
        .replace("http://mk.test", &t.base);
    assert!(nr
        .get(&cb2)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap()
        .contains("reason=expired"));
    assert_eq!(
        json_of(
            t.user_post(
                &format!("/v1/me/oauth/openrouter/flows/{flow}/cancel"),
                json!({})
            )
            .await
        )
        .await["status"],
        "saved"
    );

    // an OpenRouter grant calls the OpenAI-shaped endpoint with exact slugs only
    let tok = t.grant("openrouter", json!({"mode": "trust"})).await;
    let call = |body: Value| {
        t.c.post(format!("{}/v1/openrouter/chat/completions", t.base))
            .bearer_auth(&tok)
            .json(&body)
            .send()
    };
    let ok = json_of(call(json!({"model": "anthropic/claude-haiku-5.5", "messages": [{"role": "user", "content": "hi"}]})).await.unwrap()).await;
    assert_eq!(ok["object"], "chat.completion");
    for (k, v) in [
        ("provider", json!({"order": ["x"]})),
        ("models", json!(["openai/gpt-4o"])),
        ("plugins", json!([{"id": "web"}])),
    ] {
        let mut b =
            json!({"model": "openai/gpt-4o-mini", "messages": [{"role": "user", "content": "hi"}]});
        b[k] = v;
        let r = call(b).await.unwrap();
        assert_eq!(r.status(), 400, "{k}");
        assert_eq!(json_of(r).await["error"]["type"], "unsupported_option");
    }
    let r = call(
        json!({"model": "meta-llama/llama-3-70b", "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await
    .unwrap();
    assert_eq!(json_of(r).await["error"]["type"], "model_not_allowed");
    // OpenRouter keys are checked for their own shape
    let r = t
        .user_put(
            "/v1/me/keys/openrouter",
            json!({"api_key": "sk-proj-notopenrouter"}),
        )
        .await;
    assert_eq!(json_of(r).await["error"]["type"], "invalid_key");
    let r = t
        .user_put(
            "/v1/me/keys/openai",
            json!({"api_key": "sk-or-v1-wrongvendor0"}),
        )
        .await;
    assert_eq!(json_of(r).await["error"]["type"], "invalid_key");
}

#[tokio::test]
async fn openrouter_live_auth_url_shape() {
    let (base, c, _) = serve(Config {
        vendor_mock: false,
        ..test_cfg()
    })
    .await;
    let sess = json_of(
        c.post(format!("{base}/v1/auth/dev"))
            .json(&json!({"email": "o@r.co"}))
            .send()
            .await
            .unwrap(),
    )
    .await["session_token"]
        .as_str()
        .unwrap()
        .to_string();
    let s = json_of(
        c.post(format!("{base}/v1/me/oauth/openrouter"))
            .bearer_auth(&sess)
            .json(&json!({"limit_usd": 12.5}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let u = reqwest::Url::parse(s["auth_url"].as_str().unwrap()).unwrap();
    assert_eq!(
        (u.scheme(), u.host_str(), u.path()),
        ("https", Some("openrouter.ai"), "/auth")
    );
    let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
    assert_eq!(
        q["callback_url"],
        "http://mk.test/oauth/openrouter/callback"
    );
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["code_challenge"].len(), 43); // base64url(sha256)
    assert_eq!(q["limit"], "12.50");
    assert!(q["state"].len() >= 20);
    // the mock consent page does not exist outside mock mode
    assert_eq!(
        c.get(format!("{base}/oauth/openrouter/mock?state=x"))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
}

// ---------- launch readiness: account controls & operations ----------

fn db_dir(t: &T) -> std::path::PathBuf {
    let u = t
        .state
        .cfg
        .database_url
        .strip_prefix("sqlite://")
        .unwrap()
        .split('?')
        .next()
        .unwrap();
    std::path::Path::new(u).parent().unwrap().to_path_buf()
}
async fn dev_session(c: &Client, base: &str, email: &str) -> String {
    json_of(
        c.post(format!("{base}/v1/auth/dev"))
            .json(&json!({"email": email}))
            .send()
            .await
            .unwrap(),
    )
    .await["session_token"]
        .as_str()
        .unwrap()
        .to_string()
}
async fn user_id(t: &T, email: &str) -> String {
    sqlx::query_scalar("SELECT id FROM users WHERE email=?")
        .bind(email)
        .fetch_one(&t.state.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn revoke_all_sessions_keeps_current_by_default() {
    let t = T::new().await;
    let other = dev_session(&t.c, &t.base, "a@b.co").await;
    let keys = |tok: String| {
        let (c, u) = (t.c.clone(), format!("{}/v1/me/keys", t.base));
        async move {
            c.get(u)
                .bearer_auth(tok)
                .send()
                .await
                .unwrap()
                .status()
                .as_u16()
        }
    };
    let r = json_of(t.user_post("/v1/me/sessions/revoke-all", json!({})).await).await;
    assert_eq!(r["revoked"], 1);
    assert_eq!(
        (keys(t.sess.clone()).await, keys(other.clone()).await),
        (200, 401)
    );
    let r = json_of(
        t.user_post("/v1/me/sessions/revoke-all", json!({"keep_current": false}))
            .await,
    )
    .await;
    assert_eq!(r["revoked"], 1);
    assert_eq!(keys(t.sess.clone()).await, 401);
}

#[tokio::test]
async fn stop_all_kills_grants_and_denies_pending_but_keeps_keys() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    assert_eq!(t.chat(&tok, 0, msg()).await.status(), 202);
    let id = t.pending().await;
    // an open key enrollment and OpenRouter attempt are cancelled too
    let en = json_of(
        t.c.post(format!("{}/v1/enroll", t.base))
            .json(&json!({"vendor": "anthropic"}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let eid = en["enroll_id"].as_str().unwrap();
    assert_eq!(
        t.user_post(&format!("/v1/me/enroll/{eid}/claim"), json!({}))
            .await
            .status(),
        200
    );
    let fl = json_of(t.user_post("/v1/me/oauth/openrouter", json!({})).await).await;
    let r = json_of(t.user_post("/v1/me/stop-all", json!({})).await).await;
    assert_eq!(
        r,
        json!({"grants_revoked": 1, "requests_denied": 1, "requests_in_flight": 0, "enrollments_cancelled": 1, "flows_cancelled": 1})
    );
    assert_eq!(
        t.user_get(&format!("/v1/me/requests/{id}")).await["status"],
        "denied"
    );
    assert_eq!(
        t.chat(&tok, 0, msg()).await.status(),
        401,
        "grant token must die"
    );
    assert_eq!(
        t.user_get("/v1/me/keys").await.as_array().unwrap().len(),
        1,
        "keys stay"
    );
    assert_eq!(
        t.user_get(&format!("/v1/me/enroll/{eid}")).await["status"],
        "cancelled"
    );
    let flow = t
        .user_get(&format!(
            "/v1/me/oauth/openrouter/flows/{}",
            fl["flow_id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(flow["status"], "cancelled");
    // idempotent, and a fresh connect works afterwards
    let r = json_of(t.user_post("/v1/me/stop-all", json!({})).await).await;
    assert_eq!(r["grants_revoked"], 0);
    let tok2 = t.grant("openai", json!({"mode": "ask"})).await;
    assert_eq!(t.chat(&tok2, 0, msg()).await.status(), 202);
}

#[tokio::test]
async fn pause_blocks_proxy_and_holds_approved_requests_and_stop_all_refunds_them() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "ask"})).await;
    assert_eq!(t.chat(&tok, 0, msg()).await.status(), 202);
    let id = t.pending().await;
    let st = json_of(
        t.c.get(format!("{}/v1/status", t.base))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        (st["ok"].clone(), st["paused"].clone(), st["db"].clone()),
        (json!(true), json!(false), json!(true))
    );
    let flag = db_dir(&t).join("PAUSE_UPSTREAM");
    std::fs::write(&flag, "").unwrap();
    assert_eq!(
        json_of(
            t.c.get(format!("{}/v1/status", t.base))
                .send()
                .await
                .unwrap()
        )
        .await["paused"],
        true
    );
    // new calls are refused before anything is stored
    let r = t.chat(&tok, 0, msg()).await;
    assert_eq!(r.status(), 503);
    assert_eq!(json_of(r).await["error"]["type"], "upstream_paused");
    assert_eq!(
        t.user_get("/v1/me/requests")
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // approving with a budget reserves money, but nothing is sent while paused
    let a = t
        .user_post(
            &format!("/v1/me/requests/{id}/approve"),
            json!({"policy": {"mode": "budget", "remaining_usd": 1.0}}),
        )
        .await;
    assert_eq!(a.status(), 200);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        t.user_get(&format!("/v1/me/requests/{id}")).await["status"],
        "approved"
    );
    let remaining = || async {
        sqlx::query_scalar::<_, i64>("SELECT remaining_micro FROM grants")
            .fetch_one(&t.state.db)
            .await
            .unwrap()
    };
    assert!(remaining().await < 1_000_000);
    // unpaused: the held request is sent (the sweeper does this every 30 s; call it directly here)
    std::fs::remove_file(&flag).unwrap();
    send_request(t.state.clone(), id.clone()).await;
    assert_eq!(t.wait_status(&id, "completed").await["status"], "completed");
    // a second request approved while paused, then stop-all: denied, reservation back
    let gid = t.user_get("/v1/me/grants").await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r =
        t.c.patch(format!("{}/v1/me/grants/{gid}", t.base))
            .bearer_auth(&t.sess)
            .json(&json!({"policy": {"mode": "ask"}}))
            .send()
            .await
            .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(t.chat(&tok, 0, msg()).await.status(), 202);
    let id2 = t.pending().await;
    std::fs::write(&flag, "").unwrap();
    t.user_post(&format!("/v1/me/requests/{id2}/approve"), json!({}))
        .await;
    let before = remaining().await;
    let r = json_of(t.user_post("/v1/me/stop-all", json!({})).await).await;
    assert_eq!(r["requests_denied"], 1);
    assert_eq!(
        t.user_get(&format!("/v1/me/requests/{id2}")).await["status"],
        "denied"
    );
    assert!(remaining().await >= before);
    std::fs::remove_file(&flag).unwrap();
    send_request(t.state.clone(), id2.clone()).await;
    assert_eq!(
        t.user_get(&format!("/v1/me/requests/{id2}")).await["status"],
        "denied",
        "never sent afterwards"
    );
}

#[tokio::test]
async fn pause_env_flag_blocks_proxy() {
    let (base, c, _) = serve(Config {
        pause_upstream: true,
        ..test_cfg()
    })
    .await;
    let r = c
        .post(format!("{base}/v1/openai/chat/completions"))
        .bearer_auth("keyala_grant_x")
        .json(&msg())
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 503);
    assert_eq!(
        json_of(c.get(format!("{base}/v1/status")).send().await.unwrap()).await["paused"],
        true
    );
}

/// every table that has a user_id column, found from the schema so a future table is caught
async fn user_tables(t: &T) -> Vec<String> {
    sqlx::query_scalar("SELECT m.name FROM sqlite_master m, pragma_table_info(m.name) p WHERE m.type='table' AND p.name='user_id'").fetch_all(&t.state.db).await.unwrap()
}
async fn rows_for(t: &T, uid: &str) -> i64 {
    let mut n = 0;
    for tab in user_tables(t).await {
        n += sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {tab} WHERE user_id=?"))
            .bind(uid)
            .fetch_one(&t.state.db)
            .await
            .unwrap();
    }
    n + sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE id=?")
        .bind(uid)
        .fetch_one(&t.state.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn delete_account_needs_fresh_identity_and_removes_everything() {
    let t = T::new().await;
    // another user whose data must survive
    let o = dev_session(&t.c, &t.base, "other@b.co").await;
    let r =
        t.c.put(format!("{}/v1/me/keys/openai", t.base))
            .bearer_auth(&o)
            .json(&json!({"api_key": "sk-test-OTHER0000"}))
            .send()
            .await
            .unwrap();
    assert_eq!(r.status(), 200);
    // data of every kind for the user
    let trust = t.grant("openai", json!({"mode": "trust"})).await;
    assert_eq!(t.chat(&trust, 5, msg()).await.status(), 200);
    let ask = t.grant("openai", json!({"mode": "ask"})).await;
    assert_eq!(t.chat(&ask, 0, msg()).await.status(), 202);
    assert_eq!(
        t.user_post("/v1/me/devices", json!({"apns_token": "aabbcc"}))
            .await
            .status(),
        204
    );
    let en = json_of(
        t.c.post(format!("{}/v1/enroll", t.base))
            .json(&json!({"vendor": "anthropic"}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    t.user_post(
        &format!("/v1/me/enroll/{}/claim", en["enroll_id"].as_str().unwrap()),
        json!({}),
    )
    .await;
    t.user_post("/v1/me/oauth/openrouter", json!({})).await;
    let uid = user_id(&t, "a@b.co").await;
    let oid = user_id(&t, "other@b.co").await;
    assert!(rows_for(&t, &uid).await >= 8);
    assert!(
        user_tables(&t).await.len() >= 6,
        "{:?}",
        user_tables(&t).await
    );
    let del = |body: Value, tok: String| {
        let (c, u) = (t.c.clone(), format!("{}/v1/me/delete", t.base));
        async move { c.post(u).bearer_auth(tok).json(&body).send().await.unwrap() }
    };
    // refusals: no confirmation, bare session, wrong email
    let r = del(
        json!({"confirm": "delete", "email": "a@b.co"}),
        t.sess.clone(),
    )
    .await;
    assert_eq!(
        (
            r.status().as_u16(),
            json_of(r).await["error"]["type"].clone()
        ),
        (400, json!("confirmation_required"))
    );
    for body in [
        json!({"confirm": "DELETE"}),
        json!({"confirm": "DELETE", "email": "other@b.co"}),
        json!({"confirm": "DELETE", "password": "x"}),
    ] {
        let r = del(body, t.sess.clone()).await;
        assert_eq!(
            (
                r.status().as_u16(),
                json_of(r).await["error"]["type"].clone()
            ),
            (401, json!("reauth_failed"))
        );
    }
    assert!(rows_for(&t, &uid).await >= 8);
    // a request being sent blocks deletion; an uncertain one does not
    sqlx::query("UPDATE requests SET status='sending' WHERE user_id=? AND status='completed'")
        .bind(&uid)
        .execute(&t.state.db)
        .await
        .unwrap();
    let r = del(
        json!({"confirm": "DELETE", "email": "A@b.co"}),
        t.sess.clone(),
    )
    .await;
    assert_eq!(
        (
            r.status().as_u16(),
            json_of(r).await["error"]["type"].clone()
        ),
        (409, json!("request_in_flight"))
    );
    assert_eq!(
        t.c.get(format!("{}/v1/me/keys", t.base))
            .bearer_auth(&t.sess)
            .send()
            .await
            .unwrap()
            .status(),
        200,
        "session survives a 409"
    );
    sqlx::query("UPDATE requests SET status='uncertain' WHERE status='sending'")
        .execute(&t.state.db)
        .await
        .unwrap();
    let r = del(
        json!({"confirm": "DELETE", "email": "A@b.co"}),
        t.sess.clone(),
    )
    .await;
    assert_eq!(r.status(), 200);
    assert_eq!(
        json_of(r).await,
        json!({"deleted": true, "apple_revoked": null})
    );
    assert_eq!(rows_for(&t, &uid).await, 0);
    assert!(rows_for(&t, &oid).await >= 2, "other user untouched");
    assert_eq!(
        t.c.get(format!("{}/v1/me/keys", t.base))
            .bearer_auth(&t.sess)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        t.chat(&trust, 0, msg()).await.status(),
        401,
        "grant tokens died"
    );
    assert_eq!(
        t.c.get(format!("{}/v1/me/keys", t.base))
            .bearer_auth(&o)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    // opaque id (no email) in the deletion log
    let log = std::fs::read_to_string(db_dir(&t).join("deleted-accounts.log")).unwrap();
    assert!(
        log.starts_with(&format!("deleted {uid} ")) && !log.contains('@'),
        "{log}"
    );
}

#[tokio::test]
async fn delete_account_rate_limited_and_dev_email_needs_dev_login() {
    let (base, c, _) = serve(Config {
        dev_login: true,
        ..test_cfg()
    })
    .await;
    let sess = dev_session(&c, &base, "rl@b.co").await;
    let mut last = 0;
    for _ in 0..6 {
        last = c
            .post(format!("{base}/v1/me/delete"))
            .bearer_auth(&sess)
            .json(&json!({"confirm": "DELETE", "email": "nope@b.co"}))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16();
    }
    assert_eq!(last, 429);
}

#[tokio::test]
async fn delete_password_account_needs_the_password() {
    use sha2::{Digest, Sha256};
    let pw = "a-random-staging-password";
    let (base, c, _) = serve(Config {
        password_login: Some(("staging-qa".into(), Sha256::digest(pw).into())),
        dev_login: false,
        ..test_cfg()
    })
    .await;
    let sess = json_of(
        c.post(format!("{base}/v1/auth/password"))
            .json(&json!({"username": "staging-qa", "password": pw}))
            .send()
            .await
            .unwrap(),
    )
    .await["session_token"]
        .as_str()
        .unwrap()
        .to_string();
    let del = |body: Value| {
        let (c, u, s) = (c.clone(), format!("{base}/v1/me/delete"), sess.clone());
        async move { c.post(u).bearer_auth(s).json(&body).send().await.unwrap() }
    };
    assert_eq!(
        del(json!({"confirm": "DELETE", "password": "wrong"}))
            .await
            .status(),
        401
    );
    assert_eq!(
        del(json!({"confirm": "DELETE", "email": "staging-qa@password.invalid"}))
            .await
            .status(),
        401,
        "dev email path is off"
    );
    assert_eq!(
        del(json!({"confirm": "DELETE", "password": pw}))
            .await
            .status(),
        200
    );
    assert_eq!(
        c.get(format!("{base}/v1/me/keys"))
            .bearer_auth(&sess)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
}

/// Apple stand-in: JWKS plus token/revoke endpoints; records each call's path and form.
type AppleLog =
    std::sync::Arc<std::sync::Mutex<Vec<(String, std::collections::HashMap<String, String>)>>>;
async fn apple_ops_server(token_ok: bool, with_key: bool) -> (String, Client, AppleLog) {
    use axum::{
        extract::Form,
        routing::{get, post},
    };
    type F = Form<std::collections::HashMap<String, String>>;
    let log: AppleLog = Default::default();
    let (l1, l2) = (log.clone(), log.clone());
    let jwks = json!({"keys": [{"kty": "RSA", "kid": "k1", "use": "sig", "alg": "RS256", "n": RSA_N, "e": "AQAB"}]});
    let app = axum::Router::new()
        .route(
            "/auth/keys",
            get(move || {
                let j = jwks.clone();
                async move { axum::Json(j) }
            }),
        )
        .route(
            "/auth/token",
            post(move |Form(f): F| {
                l1.lock().unwrap().push(("token".into(), f));
                async move {
                    if token_ok {
                        (
                            axum::http::StatusCode::OK,
                            axum::Json(json!({"refresh_token": "r-123", "access_token": "a"})),
                        )
                    } else {
                        (
                            axum::http::StatusCode::BAD_REQUEST,
                            axum::Json(json!({"error": "invalid_grant"})),
                        )
                    }
                }
            }),
        )
        .route(
            "/auth/revoke",
            post(move |Form(f): F| {
                l2.lock().unwrap().push(("revoke".into(), f));
                async move { axum::Json(json!({})) }
            }),
        );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    let cfg = Config {
        apple_jwks_url: format!("{host}/auth/keys"),
        apple_auth_url: host,
        apple_bundle_id: "app.keyala.ios".into(),
        apple_signin_key: with_key.then(|| {
            (
                "KEYID123".to_string(),
                include_str!("fixtures/test_ec_private.pem").to_string(),
            )
        }),
        ..test_cfg()
    };
    let (base, c, _) = serve(cfg).await;
    (base, c, log)
}
fn apple_token_at(sub: &str, iat_age: Option<i64>) -> String {
    let mut h = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    h.kid = Some("k1".into());
    let now = chrono_now();
    let mut c = json!({"iss": "https://appleid.apple.com", "aud": "app.keyala.ios", "sub": sub, "exp": now + 600});
    if let Some(a) = iat_age {
        c["iat"] = (now - a).into();
    }
    jsonwebtoken::encode(
        &h,
        &c,
        &jsonwebtoken::EncodingKey::from_rsa_pem(RSA_PEM.as_bytes()).unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn delete_apple_account_reauth_and_revocation() {
    for (token_ok, with_key, want) in [
        (true, true, json!(true)),
        (false, true, json!(false)),
        (true, false, json!(false)),
    ] {
        let (base, c, log) = apple_ops_server(token_ok, with_key).await;
        let sess = json_of(
            c.post(format!("{base}/v1/auth/apple"))
                .json(&json!({"id_token": apple_token_at("001.del", Some(10))}))
                .send()
                .await
                .unwrap(),
        )
        .await["session_token"]
            .as_str()
            .unwrap()
            .to_string();
        let del = |body: Value| {
            let (c, u, s) = (c.clone(), format!("{base}/v1/me/delete"), sess.clone());
            async move { c.post(u).bearer_auth(s).json(&body).send().await.unwrap() }
        };
        // not fresh, no iat, wrong subject, missing token: all the same 401, and nothing is sent to Apple
        for body in [
            json!({"confirm": "DELETE", "id_token": apple_token_at("001.del", Some(900)), "authorization_code": "CODE"}),
            json!({"confirm": "DELETE", "id_token": apple_token_at("001.del", None), "authorization_code": "CODE"}),
            json!({"confirm": "DELETE", "id_token": apple_token_at("001.other", Some(5)), "authorization_code": "CODE"}),
            json!({"confirm": "DELETE", "authorization_code": "CODE"}),
        ] {
            let r = del(body).await;
            assert_eq!(
                (
                    r.status().as_u16(),
                    json_of(r).await["error"]["type"].clone()
                ),
                (401, json!("reauth_failed"))
            );
        }
        assert!(log.lock().unwrap().is_empty());
        let r = del(json!({"confirm": "DELETE", "id_token": apple_token_at("001.del", Some(5)), "authorization_code": "CODE"})).await;
        assert_eq!(r.status(), 200);
        assert_eq!(
            json_of(r).await,
            json!({"deleted": true, "apple_revoked": want}),
            "token_ok={token_ok} key={with_key}"
        );
        assert_eq!(
            c.get(format!("{base}/v1/me/keys"))
                .bearer_auth(&sess)
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
        let calls = log.lock().unwrap().clone();
        if with_key {
            assert_eq!(calls[0].0, "token");
            assert_eq!(
                (
                    calls[0].1["code"].as_str(),
                    calls[0].1["grant_type"].as_str(),
                    calls[0].1["client_id"].as_str()
                ),
                ("CODE", "authorization_code", "app.keyala.ios")
            );
            let hdr = jsonwebtoken::decode_header(&calls[0].1["client_secret"]).unwrap();
            assert_eq!(
                (hdr.alg, hdr.kid.as_deref()),
                (jsonwebtoken::Algorithm::ES256, Some("KEYID123"))
            );
            assert_eq!(calls.len(), if token_ok { 2 } else { 1 });
            if token_ok {
                assert_eq!(
                    (calls[1].0.as_str(), calls[1].1["token"].as_str()),
                    ("revoke", "r-123")
                );
            }
        } else {
            assert!(calls.is_empty());
        }
    }
}

#[tokio::test]
async fn in_process_backup_runs_prunes_and_is_a_database() {
    let dir = std::env::temp_dir().join(format!("mk-bk-{}", keyala_server::rand_b64(6)));
    let (_, _, _) = serve(Config {
        backup_dir: Some(dir.clone()),
        backup_keep: 2,
        backup_interval: Duration::from_secs(1),
        ..test_cfg()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(4500)).await;
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    assert_eq!(files.len(), 2, "{files:?}");
    assert!(std::fs::read(&files[1])
        .unwrap()
        .starts_with(b"SQLite format 3"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&files[1]).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[tokio::test]
async fn logout_ends_only_the_current_session() {
    let t = T::new().await;
    let second = dev_session(&t.c, &t.base, "a@b.co").await; // same user, another device
    let other = dev_session(&t.c, &t.base, "other@b.co").await;
    let keys = |tok: String| {
        let (c, u) = (t.c.clone(), format!("{}/v1/me/keys", t.base));
        async move {
            c.get(u)
                .bearer_auth(tok)
                .send()
                .await
                .unwrap()
                .status()
                .as_u16()
        }
    };
    let out = |tok: String| {
        let (c, u) = (t.c.clone(), format!("{}/v1/me/logout", t.base));
        async move { c.post(u).bearer_auth(tok).send().await.unwrap() }
    };
    assert_eq!(
        out("keyala_sess_bogus".into()).await.status(),
        401,
        "unknown token"
    );
    assert_eq!(
        t.c.post(format!("{}/v1/me/logout", t.base))
            .send()
            .await
            .unwrap()
            .status(),
        401,
        "no token"
    );
    let r = out(t.sess.clone()).await;
    assert_eq!(
        (r.status().as_u16(), json_of(r).await),
        (200, json!({"signed_out": true}))
    );
    assert_eq!(keys(t.sess.clone()).await, 401, "current token is dead");
    assert_eq!(
        keys(second.clone()).await,
        200,
        "same user's other session unaffected"
    );
    assert_eq!(keys(other.clone()).await, 200, "other user unaffected");
    assert_eq!(
        out(t.sess.clone()).await.status(),
        401,
        "repeat = 401, clients treat it as already signed out"
    );
    assert_eq!(out(other.clone()).await.status(), 200);
    assert_eq!(keys(second).await, 200);
}

#[tokio::test]
async fn login_reports_auth_provider() {
    use sha2::{Digest, Sha256};
    let (base, c, _) = serve(Config {
        password_login: Some(("qa-user".into(), Sha256::digest("pw-long-random").into())),
        ..test_cfg()
    })
    .await;
    let d = json_of(
        c.post(format!("{base}/v1/auth/dev"))
            .json(&json!({"email": "x@b.co"}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(d["user"]["auth_provider"], "dev");
    let p = json_of(
        c.post(format!("{base}/v1/auth/password"))
            .json(&json!({"username": "qa-user", "password": "pw-long-random"}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        (
            p["user"]["auth_provider"].clone(),
            p["user"]["email"].clone()
        ),
        (json!("password"), json!("qa-user@password.invalid"))
    );
    // a typed dev email can never reach the password or Apple placeholder identities
    let r = c
        .post(format!("{base}/v1/auth/dev"))
        .json(&json!({"email": "qa-user@password.invalid"}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);
}

#[tokio::test]
async fn deletion_without_a_tombstone_deletes_nothing() {
    let t = T::new().await;
    let uid = user_id(&t, "a@b.co").await;
    // the tombstone file cannot be opened (a directory is in its place): deletion must refuse, not proceed
    let log = db_dir(&t).join("deleted-accounts.log");
    std::fs::create_dir(&log).unwrap();
    let r = t
        .user_post(
            "/v1/me/delete",
            json!({"confirm": "DELETE", "email": "a@b.co"}),
        )
        .await;
    assert_eq!(
        (
            r.status().as_u16(),
            json_of(r).await["error"]["type"].clone()
        ),
        (503, json!("deletion_unavailable"))
    );
    assert!(
        rows_for(&t, &uid).await >= 2,
        "user, session and key still there"
    );
    assert_eq!(
        t.c.get(format!("{}/v1/me/keys", t.base))
            .bearer_auth(&t.sess)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    // once the log is writable again the same request succeeds
    std::fs::remove_dir(&log).unwrap();
    assert_eq!(
        t.user_post(
            "/v1/me/delete",
            json!({"confirm": "DELETE", "email": "a@b.co"})
        )
        .await
        .status(),
        200
    );
    assert!(std::fs::read_to_string(&log)
        .unwrap()
        .starts_with(&format!("deleted {uid} ")));
}

#[tokio::test]
async fn trust_pages_are_served_and_linked() {
    let t = T::new().await;
    for p in ["/about", "/privacy"] {
        let r = t.c.get(format!("{}{p}", t.base)).send().await.unwrap();
        assert_eq!(r.status(), 200);
        let body = r.text().await.unwrap();
        assert!(
            body.contains("personal experimental project")
                && body.contains("support@keyala.io")
                && body.contains("decrypts"),
            "{p}"
        );
        assert!(!body.contains('\u{b7}'));
    }
    for p in [
        "/add-key",
        "/enroll/en_abcdefghijklmnopqrstuv",
        "/connect/KY-ABCD",
    ] {
        let r = t.c.get(format!("{}{p}", t.base)).send().await.unwrap();
        assert_eq!(r.status(), 200, "{p}");
        assert!(
            r.text().await.unwrap().contains("/privacy"),
            "{p} links to the disclosure"
        );
    }
}

#[tokio::test]
async fn connect_reports_where_the_link_was_seen_and_serves_the_sdk() {
    let t = T::new().await;
    let b = &t.base;
    let app = json_of(
        t.c.post(format!("{b}/v1/apps"))
            .json(&json!({"name": "Seen", "allowed_origins": [ORIGIN]}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let c = json_of(
        t.c.post(format!("{b}/v1/connect"))
            .header("origin", ORIGIN)
            .json(&json!({"vendor": "openai", "app_id": app["app_id"]}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let (code, cs) = (
        c["connect_code"].as_str().unwrap(),
        c["connect_secret"].as_str().unwrap(),
    );
    let seen = || async {
        json_of(
            t.c.get(format!("{b}/v1/connect/{code}"))
                .bearer_auth(cs)
                .send()
                .await
                .unwrap(),
        )
        .await["seen"]
            .clone()
    };
    assert_eq!(seen().await, Value::Null);
    // phone without the app: the browser lands on the page, which offers the install path and the app scheme
    let page =
        t.c.get(format!("{b}/connect/{code}"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
    assert!(page.contains(&format!("keyala://connect/{code}")) && page.contains("Get Keyala"));
    assert_eq!(seen().await, "web");
    // after installing, the app previews it
    assert_eq!(
        t.user_get(&format!("/v1/me/connect/{code}")).await["vendor"],
        "openai"
    );
    assert_eq!(seen().await, "app");
    let r = t.c.get(format!("{b}/sdk/keyala.js")).send().await.unwrap();
    assert!(r.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/javascript"));
    assert!(r.text().await.unwrap().contains("Keyala.connect"));
}

// ---------- OpenRouter routing for OpenAI/Anthropic-protocol apps ----------

const OR_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const OR_MSG_URL: &str = "https://openrouter.ai/api/v1/messages";
const OPENAI_URL: &str = "https://api.openai.com/v1/chat/completions";

async fn set_pref(t: &T, on: bool) -> Value {
    let r =
        t.c.patch(format!("{}/v1/me/preferences", t.base))
            .bearer_auth(&t.sess)
            .json(&json!({"openrouter_for_direct_apis": on}))
            .send()
            .await
            .unwrap();
    assert_eq!(r.status(), 200);
    json_of(r).await
}
async fn add_or_key(t: &T) {
    assert_eq!(
        t.user_put(
            "/v1/me/keys/openrouter",
            json!({"api_key": "sk-or-v1-testORKEY9999"})
        )
        .await
        .status(),
        200
    );
}
async fn anthropic_call(t: &T, tok: &str, body: Value) -> Response {
    t.c.post(format!("{}/v1/anthropic/messages", t.base))
        .bearer_auth(tok)
        .header("X-Keyala-Wait", "5")
        .json(&body)
        .send()
        .await
        .unwrap()
}
fn haiku() -> Value {
    json!({"model": "claude-haiku-5-5", "messages": [{"role": "user", "content": "hi"}], "max_tokens": 16})
}

#[tokio::test]
async fn routing_preference_is_off_by_default_and_owner_scoped() {
    let t = T::new().await;
    let other = dev_session(&t.c, &t.base, "other@b.co").await;
    let get = |tok: String| {
        let (c, u) = (t.c.clone(), format!("{}/v1/me/preferences", t.base));
        async move { c.get(u).bearer_auth(tok).send().await.unwrap() }
    };
    assert_eq!(
        json_of(get(t.sess.clone()).await).await,
        json!({"openrouter_for_direct_apis": false})
    );
    assert_eq!(
        t.c.get(format!("{}/v1/me/preferences", t.base))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        set_pref(&t, true).await,
        json!({"openrouter_for_direct_apis": true})
    );
    assert_eq!(
        json_of(get(t.sess.clone()).await).await["openrouter_for_direct_apis"],
        true
    );
    assert_eq!(
        json_of(get(other).await).await["openrouter_for_direct_apis"],
        false,
        "other user unaffected"
    );
    let bad =
        t.c.patch(format!("{}/v1/me/preferences", t.base))
            .bearer_auth(&t.sess)
            .json(&json!({"openrouter_for_direct_apis": "yes"}))
            .send()
            .await
            .unwrap();
    assert!(bad.status().is_client_error());
    assert_eq!(
        json_of(get(t.sess.clone()).await).await["openrouter_for_direct_apis"],
        true,
        "bad body changes nothing"
    );
}

#[tokio::test]
async fn openai_and_anthropic_apps_work_with_only_an_openrouter_key() {
    let (base, c, state) = server().await;
    let sess = dev_session(&c, &base, "only-or@b.co").await;
    let t = T {
        base,
        c,
        sess,
        state,
    };
    add_or_key(&t).await; // no OpenAI or Anthropic key at all
    set_pref(&t, true).await;
    // OpenAI protocol
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    let grants = t.user_get("/v1/me/grants").await;
    assert_eq!(
        (
            grants[0]["vendor"].clone(),
            grants[0]["billing_vendor"].clone()
        ),
        (json!("openai"), json!("openrouter"))
    );
    let r = t.chat(&tok, 5, msg()).await;
    assert_eq!(r.status(), 200);
    let body = json_of(r).await;
    assert_eq!(body["mock_url"], OR_URL);
    assert_eq!(
        body["mock_key_last4"], "9999",
        "the OpenRouter key, not a direct key"
    );
    assert_eq!(
        body["model"], "openai/gpt-4o-mini",
        "exact mapped slug is what is sent"
    );
    assert!(
        body["choices"].is_array(),
        "response stays in the OpenAI shape"
    );
    // Anthropic protocol: native Messages endpoint, Anthropic response shape
    let atok = t.grant("anthropic", json!({"mode": "trust"})).await;
    let r = anthropic_call(&t, &atok, haiku()).await;
    assert_eq!(r.status(), 200);
    let body = json_of(r).await;
    assert_eq!(body["mock_url"], OR_MSG_URL);
    assert_eq!(body["model"], "anthropic/claude-haiku-5.5");
    assert!(body["content"].is_array() && body["usage"]["input_tokens"].is_number());
    // the review record keeps the original model and discloses the route; actual cost comes from usage at OpenRouter prices
    let reqs = t.user_get("/v1/me/requests").await;
    let a = &reqs[0];
    assert_eq!(
        (
            a["vendor"].clone(),
            a["billing_vendor"].clone(),
            a["model"].clone(),
            a["upstream_model"].clone()
        ),
        (
            json!("anthropic"),
            json!("openrouter"),
            json!("claude-haiku-5-5"),
            json!("anthropic/claude-haiku-5.5")
        )
    );
    assert!(
        a["actual_usd"].is_number()
            && a["estimated_usd"].as_f64().unwrap() >= a["actual_usd"].as_f64().unwrap()
    );
    let detail = t
        .user_get(&format!("/v1/me/requests/{}", a["id"].as_str().unwrap()))
        .await;
    assert_eq!(
        detail["payload"]["model"], "claude-haiku-5-5",
        "stored payload is the original, immutable one"
    );
}

#[tokio::test]
async fn route_is_frozen_per_grant_and_visible_before_approval() {
    let t = T::new().await; // has a direct OpenAI key (last4 1234)
    add_or_key(&t).await;
    let direct = t.grant("openai", json!({"mode": "trust"})).await; // setting off
    set_pref(&t, true).await;
    // preview shows the route; approving with a stale route is refused
    let app = json_of(
        t.c.post(format!("{}/v1/apps", t.base))
            .json(&json!({"name": "Late"}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let secret = app["app_secret"].as_str().unwrap();
    let code = |vendor: &str| {
        let (c, u, s) = (
            t.c.clone(),
            format!("{}/v1/connect", t.base),
            secret.to_string(),
        );
        let v = vendor.to_string();
        async move {
            json_of(
                c.post(u)
                    .bearer_auth(s)
                    .json(&json!({"vendor": v}))
                    .send()
                    .await
                    .unwrap(),
            )
            .await["connect_code"]
                .as_str()
                .unwrap()
                .to_string()
        }
    };
    let c1 = code("openai").await;
    assert_eq!(
        t.user_get(&format!("/v1/me/connect/{c1}")).await["billing_vendor"],
        "openrouter"
    );
    set_pref(&t, false).await;
    let r = t
        .user_post(
            &format!("/v1/me/connect/{c1}/approve"),
            json!({"policy": {"mode": "ask"}, "billing_vendor": "openrouter"}),
        )
        .await;
    assert_eq!(
        (
            r.status().as_u16(),
            json_of(r).await["error"]["type"].clone()
        ),
        (409, json!("route_changed"))
    );
    set_pref(&t, true).await;
    let routed = t
        .user_post(
            &format!("/v1/me/connect/{c1}/approve"),
            json!({"policy": {"mode": "ask"}, "billing_vendor": "openrouter"}),
        )
        .await;
    assert_eq!(routed.status(), 200);
    // toggling again never switches an existing grant; the old direct grant stays direct
    set_pref(&t, false).await;
    let r = json_of(t.chat(&direct, 5, msg()).await).await;
    assert_eq!(
        (r["mock_url"].clone(), r["mock_key_last4"].clone()),
        (json!(OPENAI_URL), json!("1234"))
    );
    let grants = t.user_get("/v1/me/grants").await;
    let billing: Vec<&str> = grants
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["billing_vendor"].as_str().unwrap())
        .collect();
    assert!(
        billing.contains(&"openai") && billing.contains(&"openrouter"),
        "{billing:?}"
    );
}

#[tokio::test]
async fn routed_connect_needs_an_openrouter_key_and_unsupported_options_are_refused_unsent() {
    let t = T::new().await; // direct OpenAI key only
    set_pref(&t, true).await;
    let app = json_of(
        t.c.post(format!("{}/v1/apps", t.base))
            .json(&json!({"name": "NoOr"}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let secret = app["app_secret"].as_str().unwrap();
    let conn = json_of(
        t.c.post(format!("{}/v1/connect", t.base))
            .bearer_auth(secret)
            .json(&json!({"vendor": "openai"}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let code = conn["connect_code"].as_str().unwrap();
    let r = t
        .user_post(
            &format!("/v1/me/connect/{code}/approve"),
            json!({"policy": {"mode": "ask"}}),
        )
        .await;
    assert_eq!(
        (
            r.status().as_u16(),
            json_of(r).await["error"]["type"].clone()
        ),
        (409, json!("no_key")),
        "the direct key is not a fallback"
    );
    // with a key, options we have not verified on OpenRouter are rejected before anything is stored or sent
    add_or_key(&t).await;
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    for extra in [json!({"max_completion_tokens": 10}), json!({"n": 1})] {
        let mut b = msg();
        b.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let r = t.chat(&tok, 0, b).await;
        assert_eq!(
            (
                r.status().as_u16(),
                json_of(r).await["error"]["type"].clone()
            ),
            (400, json!("unsupported_option"))
        );
    }
    let dev = json!({"model": "gpt-4o-mini", "messages": [{"role": "developer", "content": "x"}], "max_tokens": 10});
    assert_eq!(t.chat(&tok, 0, dev).await.status(), 400);
    let atok = t.grant("anthropic", json!({"mode": "trust"})).await;
    let mut b = haiku();
    b["top_k"] = json!(5);
    assert_eq!(anthropic_call(&t, &atok, b).await.status(), 400);
    assert_eq!(
        t.user_get("/v1/me/requests")
            .await
            .as_array()
            .unwrap()
            .len(),
        0,
        "nothing was stored or sent"
    );
}

#[tokio::test]
async fn routed_request_is_at_most_once_never_falls_back_to_a_direct_key_and_missing_usage_stays_null(
) {
    let t = T::new().await;
    add_or_key(&t).await;
    set_pref(&t, true).await;
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    // a repeated POST with the same Idempotency-Key is one request
    let send = || {
        t.c.post(format!("{}/v1/openai/chat/completions", t.base))
            .bearer_auth(&tok)
            .header("X-Keyala-Wait", "5")
            .header("Idempotency-Key", "route-1")
            .json(&msg())
            .send()
    };
    let (a, b) = (send().await.unwrap(), send().await.unwrap());
    assert_eq!((a.status().as_u16(), b.status().as_u16()), (200, 200));
    assert_eq!(
        t.user_get("/v1/me/requests")
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // missing usage: actual stays null (not zero, not the estimate)
    let mut nu = msg();
    nu["messages"][0]["content"] = json!("KEYALA_MOCK_NOUSAGE");
    assert_eq!(t.chat(&tok, 5, nu).await.status(), 200);
    let reqs = t.user_get("/v1/me/requests").await;
    assert!(reqs[0]["actual_usd"].is_null() && reqs[0]["estimated_usd"].as_f64().unwrap() > 0.0);
    // removing the OpenRouter key: the routed call fails with no_key; the direct OpenAI key is never used
    assert_eq!(
        t.c.delete(format!("{}/v1/me/keys/openrouter", t.base))
            .bearer_auth(&t.sess)
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
    let r = t.chat(&tok, 5, msg()).await;
    assert_eq!(r.status(), 400);
    assert_eq!(json_of(r).await["error"]["type"], "no_key");
}

// ---------- content at rest: per-user key, retention, no residue ----------

async fn raw_content(s: &AppState, id: &str) -> (String, Option<String>) {
    sqlx::query_as("SELECT payload, response FROM requests WHERE id=?")
        .bind(id)
        .fetch_one(&s.db)
        .await
        .unwrap()
}

fn secret_msg(word: &str) -> Value {
    json!({"model": "gpt-4o-mini", "messages": [{"role": "user", "content": word}], "max_tokens": 50})
}

#[tokio::test]
async fn request_content_is_encrypted_at_rest_and_reads_back() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    let r = t.chat(&tok, 0, secret_msg("PROMPT-SECRET-7731")).await;
    assert_eq!(r.status(), 200);
    let rid = r.headers()["x-keyala-request-id"]
        .to_str()
        .unwrap()
        .to_string();
    let (p, resp) = raw_content(&t.state, &rid).await;
    let resp = resp.unwrap();
    assert!(p.starts_with("enc1:") && resp.starts_with("enc1:"));
    assert!(!p.contains("PROMPT-SECRET") && !resp.contains("mock vendor"));
    // the user, the developer poll and the report still see what they did before
    let d = t.user_get(&format!("/v1/me/requests/{rid}")).await;
    assert_eq!(d["payload"]["messages"][0]["content"], "PROMPT-SECRET-7731");
    assert_eq!(d["response"]["object"], "chat.completion");
    assert_eq!(d["purged"], false);
    let p = json_of(
        t.c.get(format!("{}/v1/requests/{rid}", t.base))
            .bearer_auth(&tok)
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(p["response"]["object"], "chat.completion");
    let (i, o): (Option<i64>, Option<i64>) =
        sqlx::query_as("SELECT input_tokens, output_tokens FROM requests WHERE id=?")
            .bind(&rid)
            .fetch_one(&t.state.db)
            .await
            .unwrap();
    assert!(i.unwrap() > 0 && o.unwrap() > 0);
    // the data key itself is stored wrapped, never as raw bytes
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE dek_ct IS NOT NULL AND length(dek_ct) > 32",
    )
    .fetch_one(&t.state.db)
    .await
    .unwrap();
    assert_eq!(n, 1, "one wrapped key (ciphertext + GCM tag)");
}

#[tokio::test]
async fn plaintext_rows_from_before_encryption_are_sealed_at_startup() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    let r = t.chat(&tok, 0, msg()).await;
    let rid = r.headers()["x-keyala-request-id"]
        .to_str()
        .unwrap()
        .to_string();
    // simulate a row written by a build without encryption
    sqlx::query("UPDATE requests SET payload=?, response=? WHERE id=?")
        .bind(r#"{"model":"gpt-4o-mini","note":"LEGACY-PLAIN"}"#)
        .bind(r#"{"object":"chat.completion","note":"LEGACY-RESP"}"#)
        .bind(&rid)
        .execute(&t.state.db)
        .await
        .unwrap();
    // legacy plaintext stays readable before the pass
    let d = t.user_get(&format!("/v1/me/requests/{rid}")).await;
    assert_eq!(d["payload"]["note"], "LEGACY-PLAIN");
    // restart on the same file and master key
    let s2 = build_state(Config {
        database_url: t.state.cfg.database_url.clone(),
        ..test_cfg()
    })
    .await
    .unwrap();
    let (p, resp) = raw_content(&s2, &rid).await;
    assert!(p.starts_with("enc1:") && resp.unwrap().starts_with("enc1:"));
    let row = keyala_server::request_row(&s2, &rid)
        .await
        .unwrap()
        .unwrap();
    assert!(row.payload.contains("LEGACY-PLAIN"));
    assert!(row.response.unwrap().contains("LEGACY-RESP"));
}

#[tokio::test]
async fn retention_removes_content_and_keeps_metadata() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    let r = t.chat(&tok, 0, msg()).await;
    let rid = r.headers()["x-keyala-request-id"]
        .to_str()
        .unwrap()
        .to_string();
    // a pending one, just as old, is kept: nobody has decided it yet
    let pending = json_of(
        t.chat(&t.grant("openai", json!({"mode": "ask"})).await, 0, msg())
            .await,
    )
    .await["request_id"]
        .as_str()
        .unwrap()
        .to_string();
    let old = chrono_days_ago(keyala_server::RETENTION_DAYS + 1);
    sqlx::query("UPDATE requests SET completed_at=?, decided_at=?, created_at=?")
        .bind(&old)
        .bind(&old)
        .bind(&old)
        .execute(&t.state.db)
        .await
        .unwrap();
    assert_eq!(
        keyala_server::purge_expired_content(&t.state)
            .await
            .unwrap(),
        1
    );
    let d = t.user_get(&format!("/v1/me/requests/{rid}")).await;
    assert_eq!(d["purged"], true);
    assert!(d["payload"].is_null() && d["response"].is_null());
    assert_eq!(d["status"], "completed");
    assert!(d["actual_usd"].as_f64().unwrap() > 0.0);
    let (p, resp) = raw_content(&t.state, &rid).await;
    assert!(p.is_empty() && resp.is_none());
    let i: Option<i64> = sqlx::query_scalar("SELECT input_tokens FROM requests WHERE id=?")
        .bind(&rid)
        .fetch_one(&t.state.db)
        .await
        .unwrap();
    assert!(i.unwrap() > 0, "token counts survive the purge");
    let pd = t.user_get(&format!("/v1/me/requests/{pending}")).await;
    assert_eq!(pd["purged"], false);
    assert!(pd["payload"].is_object());
}

fn chrono_days_ago(days: i64) -> String {
    let t = std::time::SystemTime::now() - Duration::from_secs((days * 86_400) as u64);
    let secs = t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    // RFC3339 UTC with milliseconds, like the server writes
    let (d, s) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, dd) = civil(d);
    format!(
        "{y:04}-{m:02}-{dd:02}T{:02}:{:02}:{:02}.000Z",
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}
/// days since 1970-01-01 -> (year, month, day) (Howard Hinnant's algorithm)
fn civil(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[tokio::test]
async fn deleted_account_leaves_no_content_in_the_database_file() {
    let t = T::new().await;
    let tok = t.grant("openai", json!({"mode": "trust"})).await;
    let r = t.chat(&tok, 0, msg()).await;
    let rid = r.headers()["x-keyala-request-id"]
        .to_str()
        .unwrap()
        .to_string();
    let (sealed_payload, _) = raw_content(&t.state, &rid).await;
    let key_ct: Vec<u8> = sqlx::query_scalar("SELECT ciphertext FROM api_keys")
        .fetch_one(&t.state.db)
        .await
        .unwrap();
    let r = t
        .user_post(
            "/v1/me/delete",
            json!({"confirm": "DELETE", "email": "a@b.co"}),
        )
        .await;
    assert_eq!(r.status(), 200);
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&t.state.db)
        .await
        .unwrap();
    let path = t.state.cfg.database_url.trim_start_matches("sqlite://");
    let path = path.split('?').next().unwrap();
    let mut file = std::fs::read(path).unwrap();
    if let Ok(w) = std::fs::read(format!("{path}-wal")) {
        file.extend(w);
    }
    let has = |needle: &[u8]| file.windows(needle.len()).any(|w| w == needle);
    assert!(
        !has(&sealed_payload.as_bytes()[..40]),
        "request content left in free pages"
    );
    assert!(!has(&key_ct[..16]), "API key ciphertext left in free pages");
}

// ---------- web console ----------

async fn call(
    t: &T,
    m: reqwest::Method,
    path: &str,
    tok: Option<&str>,
    body: Option<Value>,
) -> Response {
    let mut r = t.c.request(m, format!("{}{path}", t.base));
    if let Some(k) = tok {
        r = r.bearer_auth(k);
    }
    if let Some(b) = body {
        r = r.json(&b);
    }
    r.send().await.unwrap()
}
use reqwest::Method as M;

/// phone sign-in for the web: returns the web session token
async fn web_via_phone(t: &T) -> String {
    let s = json_of(call(t, M::POST, "/v1/web/login", None, None).await).await;
    let (id, sec) = (
        s["login_id"].as_str().unwrap(),
        s["login_secret"].as_str().unwrap(),
    );
    let r = call(
        t,
        M::POST,
        &format!("/v1/me/web-login/{id}/approve"),
        Some(&t.sess),
        None,
    )
    .await;
    assert_eq!(r.status(), 200);
    let p = json_of(call(t, M::GET, &format!("/v1/web/login/{id}"), Some(sec), None).await).await;
    p["session_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn web_login_phone_flow_deny_expiry_and_secret() {
    let t = T::new().await;
    let s = json_of(
        t.c.post(format!("{}/v1/web/login", t.base))
            .header("user-agent", "Browser/1 ".repeat(40))
            .header("cf-ipcountry", "jp")
            .send()
            .await
            .unwrap(),
    )
    .await;
    let (id, sec) = (
        s["login_id"].as_str().unwrap().to_string(),
        s["login_secret"].as_str().unwrap().to_string(),
    );
    assert!(id.starts_with("wl_") && id.len() == 25);
    assert_eq!(s["url"], format!("http://mk.test/login/{id}"));
    assert_eq!(s["qr_url"], format!("http://mk.test/login/{id}/qr.svg"));
    let poll = |tok: String, id: String| {
        let (c, b) = (t.c.clone(), t.base.clone());
        async move {
            c.get(format!("{b}/v1/web/login/{id}"))
                .bearer_auth(tok)
                .send()
                .await
                .unwrap()
        }
    };
    assert_eq!(
        json_of(poll(sec.clone(), id.clone()).await).await["status"],
        "pending"
    );
    // wrong or missing secret, and an unknown id, are all 401
    assert_eq!(
        poll("keyala_wl_nope".into(), id.clone()).await.status(),
        401
    );
    assert_eq!(
        poll(sec.clone(), "wl_AAAAAAAAAAAAAAAAAAAAAA".into())
            .await
            .status(),
        401
    );
    assert_eq!(
        call(&t, M::GET, &format!("/v1/web/login/{id}"), None, None)
            .await
            .status(),
        401
    );
    // the phone sees what it is approving; a web login cannot be previewed without an app session
    let v = json_of(
        call(
            &t,
            M::GET,
            &format!("/v1/me/web-login/{id}"),
            Some(&t.sess),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(
        (v["id"].as_str(), v["status"].as_str()),
        (Some(id.as_str()), Some("pending"))
    );
    assert_eq!(v["user_agent"].as_str().unwrap().chars().count(), 200);
    assert_eq!(v["country"], "JP");
    assert!(v["expires_at"].is_string() && v["created_at"].is_string());
    assert_eq!(
        call(&t, M::GET, &format!("/v1/me/web-login/{id}"), None, None)
            .await
            .status(),
        401
    );
    let r = call(
        &t,
        M::POST,
        &format!("/v1/me/web-login/{id}/approve"),
        Some(&t.sess),
        None,
    )
    .await;
    assert_eq!(json_of(r).await["status"], "approved");
    // second decision loses
    let r = call(
        &t,
        M::POST,
        &format!("/v1/me/web-login/{id}/deny"),
        Some(&t.sess),
        None,
    )
    .await;
    assert_eq!(r.status(), 409);
    let a = json_of(poll(sec.clone(), id.clone()).await).await;
    assert_eq!(a["status"], "approved");
    let web = a["session_token"].as_str().unwrap().to_string();
    assert!(a["expires_at"].is_string());
    // the token is handed out once
    let b = json_of(poll(sec.clone(), id.clone()).await).await;
    assert_eq!(b["status"], "used");
    assert!(b.get("session_token").is_none());
    let me = json_of(call(&t, M::GET, "/v1/me", Some(&web), None).await).await;
    assert_eq!(
        (me["email"].as_str(), me["has_password"].as_bool()),
        (Some("a@b.co"), Some(false))
    );

    // deny
    let s = json_of(call(&t, M::POST, "/v1/web/login", None, None).await).await;
    let (id, sec) = (
        s["login_id"].as_str().unwrap().to_string(),
        s["login_secret"].as_str().unwrap().to_string(),
    );
    call(
        &t,
        M::POST,
        &format!("/v1/me/web-login/{id}/deny"),
        Some(&t.sess),
        None,
    )
    .await;
    let d = json_of(poll(sec.clone(), id.clone()).await).await;
    assert_eq!(d["status"], "denied");
    assert!(d.get("session_token").is_none());

    // expiry: neither approve nor an already approved row can be used past 5 minutes
    let s = json_of(call(&t, M::POST, "/v1/web/login", None, None).await).await;
    let (id, sec) = (
        s["login_id"].as_str().unwrap().to_string(),
        s["login_secret"].as_str().unwrap().to_string(),
    );
    sqlx_exec(
        &t,
        &format!("UPDATE web_logins SET expires_at='2000-01-01T00:00:00.000Z' WHERE id='{id}'"),
    )
    .await;
    let r = call(
        &t,
        M::POST,
        &format!("/v1/me/web-login/{id}/approve"),
        Some(&t.sess),
        None,
    )
    .await;
    assert_eq!(r.status(), 409);
    assert_eq!(
        json_of(poll(sec, id.clone()).await).await["status"],
        "expired"
    );
    // pages
    let r = call(&t, M::GET, &format!("/login/{id}"), None, None).await;
    assert!(r
        .text()
        .await
        .unwrap()
        .contains(&format!("keyala://login/{id}")));
    let r = call(&t, M::GET, &format!("/login/{id}/qr.svg"), None, None).await;
    assert_eq!(r.headers()["content-type"], "image/svg+xml");
    assert_eq!(
        call(&t, M::GET, "/login/bad", None, None).await.status(),
        404
    );
    // universal links
    let aasa = call(
        &t,
        M::GET,
        "/.well-known/apple-app-site-association",
        None,
        None,
    )
    .await
    .text()
    .await
    .unwrap();
    assert!(aasa.contains("/login/*"));
}

#[tokio::test]
async fn web_session_is_limited_and_expires() {
    let t = T::new().await;
    let gtok = t.grant("openai", json!({"mode": "ask"})).await;
    let rid = json_of(t.chat(&gtok, 0, msg()).await).await["request_id"]
        .as_str()
        .unwrap()
        .to_string();
    let gid = t.user_get("/v1/me/grants").await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let web = web_via_phone(&t).await;
    // allowed: reads
    for p in [
        "/v1/me",
        "/v1/me/keys",
        "/v1/me/grants",
        "/v1/me/requests",
        "/v1/me/preferences",
        "/v1/me/apps",
    ] {
        assert_eq!(
            call(&t, M::GET, p, Some(&web), None).await.status(),
            200,
            "{p}"
        );
    }
    let d = call(
        &t,
        M::GET,
        &format!("/v1/me/requests/{rid}"),
        Some(&web),
        None,
    )
    .await;
    assert_eq!(d.status(), 200);
    // blocked: everything that spends, changes keys or policy, or touches the account
    let blocked: Vec<(M, String, Option<Value>)> = vec![
        (
            M::POST,
            format!("/v1/me/requests/{rid}/approve"),
            Some(json!({})),
        ),
        (M::POST, format!("/v1/me/requests/{rid}/deny"), None),
        (
            M::PUT,
            "/v1/me/keys/openai".into(),
            Some(json!({"api_key": "sk-test-XXXXXXXX1234"})),
        ),
        (M::DELETE, "/v1/me/keys/openai".into(), None),
        (
            M::PATCH,
            format!("/v1/me/grants/{gid}"),
            Some(json!({"policy": {"mode": "ask"}})),
        ),
        (
            M::PATCH,
            "/v1/me/preferences".into(),
            Some(json!({"openrouter_for_direct_apis": true})),
        ),
        (
            M::POST,
            "/v1/me/devices".into(),
            Some(json!({"apns_token": "ab"})),
        ),
        (M::POST, "/v1/me/oauth/openrouter".into(), Some(json!({}))),
        (
            M::PUT,
            "/v1/me/password".into(),
            Some(json!({"password": "a-long-enough-password"})),
        ),
        (M::DELETE, "/v1/me/password".into(), None),
        (
            M::POST,
            "/v1/me/web-login/wl_AAAAAAAAAAAAAAAAAAAAAA/approve".into(),
            None,
        ),
        (
            M::GET,
            "/v1/me/web-login/wl_AAAAAAAAAAAAAAAAAAAAAA".into(),
            None,
        ),
        (M::POST, "/v1/me/stop-all".into(), None),
        (
            M::POST,
            "/v1/me/sessions/revoke-all".into(),
            Some(json!({})),
        ),
        (
            M::POST,
            "/v1/me/delete".into(),
            Some(json!({"confirm": "DELETE", "email": "a@b.co"})),
        ),
    ];
    for (m, p, b) in blocked {
        let r = call(&t, m.clone(), &p, Some(&web), b).await;
        let st = r.status();
        let j = json_of(r).await;
        assert_eq!(
            (st.as_u16(), j["error"]["type"].as_str()),
            (403, Some("web_session_not_allowed")),
            "{m} {p}"
        );
    }
    // nothing above changed anything
    assert_eq!(
        t.user_get(&format!("/v1/me/requests/{rid}")).await["status"],
        "pending"
    );
    assert_eq!(t.user_get("/v1/me/keys").await.as_array().unwrap().len(), 1);
    // revoking a grant is allowed, and denies its pending request
    assert_eq!(
        call(
            &t,
            M::DELETE,
            &format!("/v1/me/grants/{gid}"),
            Some(&web),
            None
        )
        .await
        .status(),
        204
    );
    assert_eq!(
        t.user_get(&format!("/v1/me/requests/{rid}")).await["status"],
        "denied"
    );
    // logout works and ends only this session
    assert_eq!(
        call(&t, M::POST, "/v1/me/logout", Some(&web), None)
            .await
            .status(),
        200
    );
    assert_eq!(
        call(&t, M::GET, "/v1/me", Some(&web), None).await.status(),
        401
    );
    assert_eq!(
        call(&t, M::GET, "/v1/me", Some(&t.sess), None)
            .await
            .status(),
        200
    );
    // expiry
    let web2 = web_via_phone(&t).await;
    assert_eq!(
        call(&t, M::GET, "/v1/me", Some(&web2), None).await.status(),
        200
    );
    sqlx_exec(
        &t,
        "UPDATE sessions SET expires_at='2000-01-01T00:00:00.000Z' WHERE scope='web'",
    )
    .await;
    assert_eq!(
        call(&t, M::GET, "/v1/me", Some(&web2), None).await.status(),
        401
    );
    // the app session is untouched by any of this and never expires
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sessions WHERE scope='app' AND expires_at IS NULL",
    )
    .fetch_one(&t.state.db)
    .await
    .unwrap();
    assert!(n >= 1);
}

#[tokio::test]
async fn web_password_set_login_and_operator_account_unchanged() {
    use sha2::{Digest, Sha256};
    let (base, c, state) = serve(Config {
        password_login: Some(("qa-user".into(), Sha256::digest("pw-long-random").into())),
        ..test_cfg()
    })
    .await;
    let t = T {
        sess: dev_session(&c, &base, "Pw@b.co").await,
        base,
        c,
        state,
    };
    let login = |u: &str, p: &str| {
        call(
            &t,
            M::POST,
            "/v1/auth/password",
            None,
            Some(json!({"username": u, "password": p})),
        )
    };
    // not set yet
    assert_eq!(login("pw@b.co", "a-very-long-password").await.status(), 401);
    // validation, then set (app session only)
    for bad in ["short", &"x".repeat(201)] {
        assert_eq!(
            call(
                &t,
                M::PUT,
                "/v1/me/password",
                Some(&t.sess),
                Some(json!({"password": bad}))
            )
            .await
            .status(),
            400
        );
    }
    assert_eq!(
        call(
            &t,
            M::PUT,
            "/v1/me/password",
            None,
            Some(json!({"password": "a-very-long-password"}))
        )
        .await
        .status(),
        401
    );
    let r = call(
        &t,
        M::PUT,
        "/v1/me/password",
        Some(&t.sess),
        Some(json!({"password": "a-very-long-password"})),
    )
    .await;
    assert_eq!(r.status(), 200);
    assert_eq!(t.user_get("/v1/me").await["has_password"], true);
    let stored: String =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE email='pw@b.co'")
            .fetch_one(&t.state.db)
            .await
            .unwrap();
    assert!(stored.starts_with("$argon2id$") && !stored.contains("a-very-long"));
    // same 401 for a wrong password and an unknown user
    for (u, p) in [
        ("pw@b.co", "wrong-password-here"),
        ("nobody@b.co", "a-very-long-password"),
    ] {
        let r = login(u, p).await;
        assert_eq!(r.status(), 401);
        assert_eq!(json_of(r).await["error"]["type"], "invalid_credentials");
    }
    let r = json_of(login("PW@b.co", "a-very-long-password").await).await;
    assert_eq!(
        (r["scope"].as_str(), r["user"]["email"].as_str()),
        (Some("web"), Some("pw@b.co"))
    );
    let web = r["session_token"].as_str().unwrap();
    assert_eq!(
        call(&t, M::GET, "/v1/me/grants", Some(web), None)
            .await
            .status(),
        200
    );
    assert_eq!(
        call(&t, M::POST, "/v1/me/stop-all", Some(web), None)
            .await
            .status(),
        403
    );
    // operator account: app-scoped session exactly as before
    let op = json_of(login("qa-user", "pw-long-random").await).await;
    assert_eq!(op["user"]["email"], "qa-user@password.invalid");
    assert!(op.get("scope").is_none());
    let op_tok = op["session_token"].as_str().unwrap();
    assert_eq!(
        call(
            &t,
            M::POST,
            "/v1/me/sessions/revoke-all",
            Some(op_tok),
            Some(json!({}))
        )
        .await
        .status(),
        200
    );
    // removing the password closes the door again
    assert_eq!(
        call(&t, M::DELETE, "/v1/me/password", Some(&t.sess), None)
            .await
            .status(),
        200
    );
    assert_eq!(login("pw@b.co", "a-very-long-password").await.status(), 401);
}

#[tokio::test]
async fn console_apps_create_list_claim_report_rotate_and_isolation() {
    let t = T::new().await;
    let web = web_via_phone(&t).await;
    let other = dev_session(&t.c, &t.base, "other@b.co").await;
    // create (web session allowed): the secret is shown once and works as a normal app secret
    let r = call(
        &t,
        M::POST,
        "/v1/me/apps",
        Some(&web),
        Some(json!({"name": "Mine", "allowed_origins": ["https://x.dev"]})),
    )
    .await;
    assert_eq!(r.status(), 200);
    let a = json_of(r).await;
    let (id, secret) = (
        a["app_id"].as_str().unwrap().to_string(),
        a["app_secret"].as_str().unwrap().to_string(),
    );
    assert_eq!(
        call(
            &t,
            M::POST,
            "/v1/me/apps",
            Some(&web),
            Some(json!({"name": ""}))
        )
        .await
        .status(),
        400
    );
    assert_eq!(
        call(&t, M::GET, "/v1/app", Some(&secret), None)
            .await
            .status(),
        200
    );
    let l = json_of(call(&t, M::GET, "/v1/me/apps", Some(&web), None).await).await;
    assert_eq!(l.as_array().unwrap().len(), 1);
    assert_eq!(
        (
            l[0]["id"].as_str(),
            l[0]["name"].as_str(),
            l[0]["requests_7d"].as_i64()
        ),
        (Some(id.as_str()), Some("Mine"), Some(0))
    );
    assert_eq!(l[0]["allowed_origins"][0], "https://x.dev");
    // some traffic: a grant for this app, one call
    let conn = json_of(
        call(
            &t,
            M::POST,
            "/v1/connect",
            Some(&secret),
            Some(json!({"vendor": "openai"})),
        )
        .await,
    )
    .await;
    let code = conn["connect_code"].as_str().unwrap().to_string();
    call(
        &t,
        M::POST,
        &format!("/v1/me/connect/{code}/approve"),
        Some(&t.sess),
        Some(json!({"policy": {"mode": "ask"}})),
    )
    .await;
    let g = json_of(
        call(
            &t,
            M::GET,
            &format!("/v1/connect/{code}"),
            Some(&secret),
            None,
        )
        .await,
    )
    .await;
    let gt = g["grant_token"].as_str().unwrap().to_string();
    t.chat(&gt, 0, msg()).await;
    let rid = t.pending().await;
    call(
        &t,
        M::POST,
        &format!("/v1/me/requests/{rid}/approve"),
        Some(&t.sess),
        Some(json!({})),
    )
    .await;
    t.wait_status(&rid, "completed").await;
    // report = exactly the /v1/app/report shape
    let mine = json_of(
        call(
            &t,
            M::GET,
            &format!("/v1/me/apps/{id}/report?days=7"),
            Some(&web),
            None,
        )
        .await,
    )
    .await;
    let direct =
        json_of(call(&t, M::GET, "/v1/app/report?days=7", Some(&secret), None).await).await;
    assert_eq!(mine, direct);
    assert_eq!(mine["totals"]["completed"], 1);
    assert_eq!(
        call(
            &t,
            M::GET,
            &format!("/v1/me/apps/{id}/report?days=0"),
            Some(&web),
            None
        )
        .await
        .status(),
        400
    );
    let l = json_of(call(&t, M::GET, "/v1/me/apps", Some(&web), None).await).await;
    assert_eq!(l[0]["requests_7d"], 1);
    assert!(l[0]["cost_usd_7d"].as_f64().unwrap() > 0.0);
    // another account sees nothing and cannot touch it
    assert_eq!(
        json_of(call(&t, M::GET, "/v1/me/apps", Some(&other), None).await)
            .await
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        call(
            &t,
            M::GET,
            &format!("/v1/me/apps/{id}/report"),
            Some(&other),
            None
        )
        .await
        .status(),
        404
    );
    assert_eq!(
        call(
            &t,
            M::POST,
            &format!("/v1/me/apps/{id}/secret"),
            Some(&other),
            None
        )
        .await
        .status(),
        404
    );
    let r = call(
        &t,
        M::POST,
        "/v1/me/apps/claim",
        Some(&other),
        Some(json!({"app_secret": secret})),
    )
    .await;
    assert_eq!(r.status(), 409);
    assert_eq!(json_of(r).await["error"]["type"], "already_owned");
    // rotate: the old secret dies at once
    let n = json_of(
        call(
            &t,
            M::POST,
            &format!("/v1/me/apps/{id}/secret"),
            Some(&web),
            None,
        )
        .await,
    )
    .await;
    let secret2 = n["app_secret"].as_str().unwrap().to_string();
    assert_ne!(secret, secret2);
    assert_eq!(
        call(&t, M::GET, "/v1/app", Some(&secret), None)
            .await
            .status(),
        401
    );
    assert_eq!(
        call(&t, M::GET, "/v1/app", Some(&secret2), None)
            .await
            .status(),
        200
    );
    // claim an unowned app by its secret; a bad secret is 401
    let free = json_of(
        call(
            &t,
            M::POST,
            "/v1/apps",
            None,
            Some(json!({"name": "Old one"})),
        )
        .await,
    )
    .await;
    let fsec = free["app_secret"].as_str().unwrap();
    let r = call(
        &t,
        M::POST,
        "/v1/me/apps/claim",
        Some(&web),
        Some(json!({"app_secret": fsec})),
    )
    .await;
    assert_eq!(r.status(), 200);
    assert_eq!(json_of(r).await["name"], "Old one");
    assert_eq!(
        call(
            &t,
            M::POST,
            "/v1/me/apps/claim",
            Some(&web),
            Some(json!({"app_secret": fsec}))
        )
        .await
        .status(),
        200
    ); // idempotent for the owner
    assert_eq!(
        call(
            &t,
            M::POST,
            "/v1/me/apps/claim",
            Some(&web),
            Some(json!({"app_secret": "keyala_app_nope"}))
        )
        .await
        .status(),
        401
    );
    assert_eq!(
        json_of(call(&t, M::GET, "/v1/me/apps", Some(&web), None).await)
            .await
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // deleting the account detaches the apps; they keep working with their secret
    let r = call(
        &t,
        M::POST,
        "/v1/me/delete",
        Some(&t.sess),
        Some(json!({"confirm": "DELETE", "email": "a@b.co"})),
    )
    .await;
    assert_eq!(r.status(), 200, "{}", r.text().await.unwrap());
    let owner: Option<String> = sqlx::query_scalar("SELECT owner_user_id FROM apps WHERE id=?")
        .bind(&id)
        .fetch_one(&t.state.db)
        .await
        .unwrap();
    assert!(owner.is_none());
    assert_eq!(
        call(&t, M::GET, "/v1/app", Some(&secret2), None)
            .await
            .status(),
        200
    );
}

#[tokio::test]
async fn console_page_is_served_with_strict_csp() {
    let t = T::new().await;
    let r = call(&t, M::GET, "/console", None, None).await;
    assert_eq!(r.status(), 200);
    let csp = r.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        csp.contains("default-src 'self'")
            && csp.contains("frame-ancestors 'none'")
            && csp.contains("connect-src 'self'")
    );
    assert!(!csp.contains("script-src 'unsafe-inline'"));
    assert!(r.text().await.unwrap().contains("/console.js"));
    let r = call(&t, M::GET, "/console.js", None, None).await;
    assert_eq!(r.status(), 200);
    let js = r.text().await.unwrap();
    assert!(js.contains("textContent"));
    // the sign-in QR page shows the QR and polls, but never prints the raw login URL
    assert!(js.contains("s.qr_url") && js.contains("s.login_secret"));
    assert!(!js.contains("s.url"));
}

#[tokio::test]
async fn console_apps_edit_and_delete() {
    let t = T::new().await;
    let web = web_via_phone(&t).await;
    let other = dev_session(&t.c, &t.base, "other@b.co").await;
    let a = json_of(
        call(
            &t,
            M::POST,
            "/v1/me/apps",
            Some(&web),
            Some(json!({"name": "Old", "allowed_origins": ["https://old.dev"]})),
        )
        .await,
    )
    .await;
    let (id, secret) = (
        a["app_id"].as_str().unwrap().to_string(),
        a["app_secret"].as_str().unwrap().to_string(),
    );
    let public = |o: &'static str| {
        t.c.post(format!("{}/v1/connect", t.base))
            .header("origin", o)
            .json(&json!({"vendor": "openai", "app_id": id}))
            .send()
    };
    assert_eq!(public("https://old.dev").await.unwrap().status(), 200);

    // edit: new name + origin; old origin refused, new accepted, secret unchanged
    let path = format!("/v1/me/apps/{id}");
    let e = call(&t, M::PUT, &path, Some(&web), Some(json!({"name": "New", "homepage": "https://new.dev/", "allowed_origins": ["https://new.dev"]}))).await;
    assert_eq!(e.status(), 200);
    let e = json_of(e).await;
    assert_eq!(
        (e["name"].as_str(), e["allowed_origins"][0].as_str()),
        (Some("New"), Some("https://new.dev"))
    );
    assert_eq!(public("https://old.dev").await.unwrap().status(), 403);
    assert_eq!(public("https://new.dev").await.unwrap().status(), 200);
    assert_eq!(
        json_of(call(&t, M::GET, "/v1/app", Some(&secret), None).await).await["name"],
        "New"
    );
    for bad in [
        json!({"name": ""}),
        json!({"name": "X", "allowed_origins": ["https://new.dev/path"]}),
    ] {
        assert_eq!(
            call(&t, M::PUT, &path, Some(&web), Some(bad))
                .await
                .status(),
            400
        );
    }
    assert_eq!(
        call(
            &t,
            M::PUT,
            &path,
            Some(&other),
            Some(json!({"name": "Mine now"}))
        )
        .await
        .status(),
        404
    );
    assert_eq!(
        call(&t, M::DELETE, &path, Some(&other), None)
            .await
            .status(),
        404
    );

    // traffic: an approved grant with one waiting call, plus a second pending connect code
    let code = json_of(
        call(
            &t,
            M::POST,
            "/v1/connect",
            Some(&secret),
            Some(json!({"vendor": "openai"})),
        )
        .await,
    )
    .await["connect_code"]
        .as_str()
        .unwrap()
        .to_string();
    call(
        &t,
        M::POST,
        &format!("/v1/me/connect/{code}/approve"),
        Some(&t.sess),
        Some(json!({"policy": {"mode": "ask"}})),
    )
    .await;
    let gt = json_of(
        call(
            &t,
            M::GET,
            &format!("/v1/connect/{code}"),
            Some(&secret),
            None,
        )
        .await,
    )
    .await["grant_token"]
        .as_str()
        .unwrap()
        .to_string();
    t.chat(&gt, 0, msg()).await;
    let rid = t.pending().await;
    let code2 = json_of(
        call(
            &t,
            M::POST,
            "/v1/connect",
            Some(&secret),
            Some(json!({"vendor": "openai"})),
        )
        .await,
    )
    .await["connect_code"]
        .as_str()
        .unwrap()
        .to_string();

    // delete
    let d = call(&t, M::DELETE, &path, Some(&web), None).await;
    assert_eq!(d.status(), 200);
    let d = json_of(d).await;
    assert_eq!(
        (d["grants_revoked"].as_i64(), d["requests_denied"].as_i64()),
        (Some(1), Some(1))
    );
    assert_eq!(
        t.user_get(&format!("/v1/me/requests/{rid}")).await["status"],
        "denied"
    );
    assert_eq!(t.chat(&gt, 0, msg()).await.status(), 401);
    assert_eq!(
        call(&t, M::GET, "/v1/app", Some(&secret), None)
            .await
            .status(),
        401
    );
    assert_eq!(
        call(
            &t,
            M::POST,
            "/v1/connect",
            Some(&secret),
            Some(json!({"vendor": "openai"}))
        )
        .await
        .status(),
        401
    );
    assert_eq!(public("https://new.dev").await.unwrap().status(), 403);
    assert_eq!(
        call(
            &t,
            M::POST,
            &format!("/v1/me/connect/{code2}/approve"),
            Some(&t.sess),
            Some(json!({"policy": {"mode": "ask"}}))
        )
        .await
        .status(),
        404
    );
    assert_eq!(
        json_of(call(&t, M::GET, "/v1/me/apps", Some(&web), None).await)
            .await
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        call(
            &t,
            M::GET,
            &format!("/v1/me/apps/{id}/report"),
            Some(&web),
            None
        )
        .await
        .status(),
        404
    );
    assert_eq!(
        call(&t, M::PUT, &path, Some(&web), Some(json!({"name": "Back"})))
            .await
            .status(),
        404
    );
    assert_eq!(
        call(&t, M::DELETE, &path, Some(&web), None).await.status(),
        404
    );
    let c = call(
        &t,
        M::POST,
        "/v1/me/apps/claim",
        Some(&web),
        Some(json!({"app_secret": secret})),
    )
    .await;
    assert_eq!(c.status(), 401);
    // the user's history keeps the call and the app name
    let h = t.user_get(&format!("/v1/me/requests/{rid}")).await;
    assert_eq!(h["status"], "denied");
}

fn idp_token(kid: &str, claims: Value) -> String {
    let mut h = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    h.kid = Some(kid.into());
    let mut c = claims;
    c["exp"] = (chrono_now() + 600).into();
    c["iat"] = chrono_now().into();
    jsonwebtoken::encode(
        &h,
        &c,
        &jsonwebtoken::EncodingKey::from_rsa_pem(RSA_PEM.as_bytes()).unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn google_login_is_its_own_account() {
    let (base, c, _) = apple_server("k1").await;
    let post = |path: &'static str, tok: String| {
        let (c, base) = (c.clone(), base.clone());
        async move {
            c.post(format!("{base}{path}"))
                .json(&json!({"id_token": tok}))
                .send()
                .await
                .unwrap()
        }
    };
    let g = |sub: &str, aud: &str, iss: &str, verified: bool| {
        idp_token(
            "k1",
            json!({"iss": iss, "aud": aud, "sub": sub, "email": "same@b.co", "email_verified": verified}),
        )
    };
    let gi = "https://accounts.google.com";
    let aud = "gid.apps.googleusercontent.com";
    let r = json_of(post("/v1/auth/google", g("s1", aud, gi, true)).await).await;
    assert_eq!(r["user"]["auth_provider"], "google");
    assert_eq!(r["user"]["email"], "same@b.co");
    let gid = r["user"]["id"].as_str().unwrap().to_string();
    // same Google sub -> same account; the bare issuer form is accepted too
    let r = json_of(post("/v1/auth/google", g("s1", aud, "accounts.google.com", true)).await).await;
    assert_eq!(r["user"]["id"], gid.as_str());
    // Apple with the same sub string and email: a different account, never linked by email
    let a = idp_token(
        "k1",
        json!({"iss": "https://appleid.apple.com", "aud": "app.keyala.ios", "sub": "s1", "email": "same@b.co"}),
    );
    let r = json_of(post("/v1/auth/apple", a).await).await;
    assert_ne!(r["user"]["id"], gid.as_str());
    assert_eq!(r["user"]["email"], "s1@apple.invalid");
    // unverified Google email is not used
    let r = json_of(post("/v1/auth/google", g("s2", aud, gi, false)).await).await;
    assert_eq!(r["user"]["email"], "s2@google.invalid");
    let sess = r["session_token"].as_str().unwrap().to_string();
    let me = json_of(
        c.get(format!("{base}/v1/me"))
            .bearer_auth(&sess)
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(me["auth_provider"], "google");
    assert_eq!(me["has_passkey"], false);
    for bad in [
        g("s1", "app.keyala.ios", gi, true),
        g("s1", aud, "https://appleid.apple.com", true),
    ] {
        let r = post("/v1/auth/google", bad).await;
        assert_eq!(r.status(), 401);
        assert_eq!(json_of(r).await["error"]["type"], "invalid_token");
    }
    // a Google token is not an Apple token
    assert_eq!(
        post("/v1/auth/apple", g("s1", aud, gi, true))
            .await
            .status(),
        401
    );
    // deletion needs a fresh Google token for the same sub
    let del = |body: Value| {
        let (c, base, sess) = (c.clone(), base.clone(), sess.clone());
        async move {
            c.post(format!("{base}/v1/me/delete"))
                .bearer_auth(&sess)
                .json(&body)
                .send()
                .await
                .unwrap()
        }
    };
    assert_eq!(
        del(json!({"confirm": "DELETE", "id_token": g("s1", aud, gi, true)}))
            .await
            .status(),
        401
    );
    let r = del(json!({"confirm": "DELETE", "id_token": g("s2", aud, gi, true)})).await;
    assert_eq!(r.status(), 200);
}

#[tokio::test]
async fn google_login_off_without_client_ids() {
    let t = T::new().await;
    let r =
        t.c.post(format!("{}/v1/auth/google", t.base))
            .json(&json!({"id_token": "x"}))
            .send()
            .await
            .unwrap();
    assert_eq!(r.status(), 404);
    assert_eq!(
        t.c.get(format!("{}/.well-known/assetlinks.json", t.base))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
}

/// a software authenticator: one P-256 passkey for rp "localhost"
struct Authn {
    kp: ring::signature::EcdsaKeyPair,
    id: Vec<u8>,
}
fn b64u(b: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, b)
}
impl Authn {
    fn new() -> Authn {
        use ring::signature::{EcdsaKeyPair, ECDSA_P256_SHA256_ASN1_SIGNING};
        let rng = ring::rand::SystemRandom::new();
        let p8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
        let kp =
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, p8.as_ref(), &rng).unwrap();
        Authn {
            kp,
            id: keyala_server::rand_b64(16).into_bytes(),
        }
    }
    fn auth_data(flags: u8, count: u32) -> Vec<u8> {
        use sha2::Digest;
        [
            &sha2::Sha256::digest(b"localhost")[..],
            &[flags],
            &count.to_be_bytes(),
        ]
        .concat()
    }
    fn client_data(ty: &str, ch: &Value, origin: &str) -> Vec<u8> {
        json!({"type": ty, "challenge": ch, "origin": origin})
            .to_string()
            .into_bytes()
    }
    fn registration(&self, opts: &Value) -> Value {
        self.registration_with(opts, 0x45, None)
    }
    fn registration_with(&self, opts: &Value, flags: u8, cross: Option<Value>) -> Value {
        use ciborium::Value as C;
        use ring::signature::KeyPair;
        let pk = self.kp.public_key().as_ref();
        let i = |n: i64| C::Integer(n.into());
        let cose = C::Map(vec![
            (i(1), i(2)),
            (i(3), i(-7)),
            (i(-1), i(1)),
            (i(-2), C::Bytes(pk[1..33].to_vec())),
            (i(-3), C::Bytes(pk[33..].to_vec())),
        ]);
        let mut key = vec![];
        ciborium::into_writer(&cose, &mut key).unwrap();
        let ad = [
            Self::auth_data(flags, 0),
            vec![0; 16],
            (self.id.len() as u16).to_be_bytes().to_vec(),
            self.id.clone(),
            key,
        ]
        .concat();
        let att = C::Map(vec![
            (C::Text("fmt".into()), C::Text("none".into())),
            (C::Text("attStmt".into()), C::Map(vec![])),
            (C::Text("authData".into()), C::Bytes(ad)),
        ]);
        let mut ao = vec![];
        ciborium::into_writer(&att, &mut ao).unwrap();
        let mut cd: Value = serde_json::from_slice(&Self::client_data(
            "webauthn.create",
            &opts["challenge"],
            "https://localhost",
        ))
        .unwrap();
        if let Some(c) = cross {
            cd["crossOrigin"] = c;
        }
        json!({"challenge_id": opts["challenge_id"], "credential_id": b64u(&self.id), "attestation_object": b64u(&ao),
            "client_data_json": b64u(cd.to_string().as_bytes()), "name": "iPhone"})
    }
    fn assertion(&self, opts: &Value, origin: &str, count: u32) -> Value {
        use sha2::Digest;
        let ad = Self::auth_data(0x05, count);
        let cd = Self::client_data("webauthn.get", &opts["challenge"], origin);
        let msg = [&ad[..], &sha2::Sha256::digest(&cd)[..]].concat();
        let sig = self
            .kp
            .sign(&ring::rand::SystemRandom::new(), &msg)
            .unwrap();
        json!({"challenge_id": opts["challenge_id"], "credential_id": b64u(&self.id), "authenticator_data": b64u(&ad),
            "client_data_json": b64u(&cd), "signature": b64u(sig.as_ref())})
    }
}

#[tokio::test]
async fn passkey_register_sign_in_and_delete() {
    let t = T::new().await;
    let post = |path: String, tok: Option<String>, body: Value| {
        let (c, base) = (t.c.clone(), t.base.clone());
        async move {
            let mut r = c.post(format!("{base}{path}")).json(&body);
            if let Some(tok) = tok {
                r = r.bearer_auth(tok);
            }
            r.send().await.unwrap()
        }
    };
    let me = Some(t.sess.clone());
    let a = Authn::new();
    let opts = json_of(
        post(
            "/v1/me/passkeys/register/options".into(),
            me.clone(),
            json!({"email": "a@b.co"}),
        )
        .await,
    )
    .await;
    assert_eq!(opts["rp_id"], "localhost");
    assert_eq!(opts["user_name"], "a@b.co");
    let reg = a.registration(&opts);
    let r = post("/v1/me/passkeys/register".into(), me.clone(), reg.clone()).await;
    assert_eq!(r.status(), 200);
    assert_eq!(json_of(r).await["name"], "iPhone");
    // challenges are single use
    let r = post("/v1/me/passkeys/register".into(), me.clone(), reg).await;
    assert_eq!(json_of(r).await["error"]["type"], "challenge_expired");
    // wrong origin at registration
    let opts = json_of(
        post(
            "/v1/me/passkeys/register/options".into(),
            me.clone(),
            json!({"email": "a@b.co"}),
        )
        .await,
    )
    .await;
    assert_eq!(opts["exclude_credentials"][0], b64u(&a.id));
    let mut bad = Authn::new().registration(&opts);
    bad["client_data_json"] = b64u(&Authn::client_data(
        "webauthn.create",
        &opts["challenge"],
        "https://evil.example",
    ))
    .into();
    assert_eq!(
        json_of(post("/v1/me/passkeys/register".into(), me.clone(), bad).await).await["error"]
            ["type"],
        "passkey_rejected"
    );
    let info = json_of(
        t.c.get(format!("{}/v1/me", t.base))
            .bearer_auth(&t.sess)
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(info["has_passkey"], true);
    let uid = info["id"].as_str().unwrap().to_string();

    // sign in with no session at all
    let lo =
        || async { json_of(post("/v1/auth/passkey/options".into(), None, json!({})).await).await };
    let o = lo().await;
    let r = json_of(
        post(
            "/v1/auth/passkey".into(),
            None,
            a.assertion(&o, "https://localhost", 0),
        )
        .await,
    )
    .await;
    assert_eq!(r["user"]["id"], uid.as_str());
    assert_eq!(r["user"]["auth_provider"], "dev");
    // bad origin, someone else's key, tampered signature: all the same 401
    let other = Authn {
        kp: Authn::new().kp,
        id: a.id.clone(),
    };
    let mut tampered = a.assertion(&lo().await, "https://localhost", 0);
    tampered["signature"] = a.assertion(&lo().await, "https://localhost", 0)["signature"].clone();
    for body in [
        a.assertion(&lo().await, "https://evil.example", 0),
        other.assertion(&lo().await, "https://localhost", 0),
        tampered,
    ] {
        let r = post("/v1/auth/passkey".into(), None, body).await;
        assert_eq!(r.status(), 401);
        assert_eq!(json_of(r).await["error"]["type"], "passkey_failed");
    }
    // a counter, once used, must go up
    assert_eq!(
        post(
            "/v1/auth/passkey".into(),
            None,
            a.assertion(&lo().await, "https://localhost", 5)
        )
        .await
        .status(),
        200
    );
    assert_eq!(
        post(
            "/v1/auth/passkey".into(),
            None,
            a.assertion(&lo().await, "https://localhost", 5)
        )
        .await
        .status(),
        401
    );
    let list = json_of(
        t.c.get(format!("{}/v1/me/passkeys", t.base))
            .bearer_auth(&t.sess)
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert!(list[0]["last_used_at"].is_string());

    // a passkey is a fresh identity for deletion
    let r = post(
        "/v1/me/delete".into(),
        me.clone(),
        json!({"confirm": "DELETE", "passkey": a.assertion(&lo().await, "https://localhost", 6)}),
    )
    .await;
    assert_eq!(r.status(), 200);
    assert_eq!(rows_for(&t, &uid).await, 0);
}

#[tokio::test]
async fn passkey_registration_needs_fresh_identity() {
    let t = T::new().await;
    let opts_req = |tok: String, body: Option<Value>| {
        let r =
            t.c.post(format!("{}/v1/me/passkeys/register/options", t.base))
                .bearer_auth(tok);
        match body {
            Some(b) => r.json(&b).send(),
            None => r.send(),
        }
    };
    let fresh = |tok: String, email: &'static str| {
        let response = opts_req(tok, Some(json!({ "email": email })));
        async move { json_of(response.await.unwrap()).await }
    };
    // a session alone, wrong email, unknown field, extra field next to a valid proof, non-object: all 401 reauth_failed
    for body in [
        None,
        Some(json!({})),
        Some(json!({"email": "someone@else.co"})),
        Some(json!({"nope": 1})),
        Some(json!({"email": "a@b.co", "confirm": "DELETE"})),
        Some(json!([])),
    ] {
        let r = opts_req(t.sess.clone(), body.clone()).await.unwrap();
        assert_eq!(r.status(), 401, "{body:?}");
        assert_eq!(json_of(r).await["error"]["type"], "reauth_failed");
    }
    // a valid proof works, and the challenge is bound to this account
    let a = Authn::new();
    let opts = fresh(t.sess.clone(), " A@B.co ").await;
    let reg = |tok: &str, body: Value| {
        t.c.post(format!("{}/v1/me/passkeys/register", t.base))
            .bearer_auth(tok.to_string())
            .json(&body)
            .send()
    };
    let o = dev_session(&t.c, &t.base, "other@b.co").await;
    assert_eq!(
        json_of(reg(&o, a.registration(&opts)).await.unwrap()).await["error"]["type"],
        "challenge_expired"
    );
    // crossOrigin true (or not a boolean) and BS without BE are rejected
    for (flags, cross) in [
        (0x45, Some(json!(true))),
        (0x45, Some(json!("true"))),
        (0x55, None),
    ] {
        let opts = fresh(t.sess.clone(), "a@b.co").await;
        let r = reg(&t.sess, a.registration_with(&opts, flags, cross))
            .await
            .unwrap();
        assert_eq!(json_of(r).await["error"]["type"], "passkey_rejected");
    }
    // crossOrigin false with BE+BS is fine
    let opts = fresh(t.sess.clone(), "a@b.co").await;
    let r = reg(
        &t.sess,
        a.registration_with(&opts, 0x5d, Some(json!(false))),
    )
    .await
    .unwrap();
    assert_eq!(r.status(), 200);
    // an existing passkey of this user is a valid proof; another user's passkey is not
    let lo = || async {
        json_of(
            t.c.post(format!("{}/v1/auth/passkey/options", t.base))
                .send()
                .await
                .unwrap(),
        )
        .await
    };
    let r = opts_req(
        t.sess.clone(),
        Some(json!({"passkey": a.assertion(&lo().await, "https://localhost", 0)})),
    )
    .await
    .unwrap();
    assert_eq!(r.status(), 200);
    let b = Authn::new();
    let ob = fresh(o.clone(), "other@b.co").await;
    assert_eq!(reg(&o, b.registration(&ob)).await.unwrap().status(), 200);
    let r = opts_req(
        t.sess.clone(),
        Some(json!({"passkey": b.assertion(&lo().await, "https://localhost", 0)})),
    )
    .await
    .unwrap();
    assert_eq!(r.status(), 401);
    assert_eq!(json_of(r).await["error"]["type"], "reauth_failed");
}

#[tokio::test]
async fn passkey_remove_is_owner_only() {
    let t = T::new().await;
    let a = Authn::new();
    let opts = json_of(
        t.c.post(format!("{}/v1/me/passkeys/register/options", t.base))
            .bearer_auth(&t.sess)
            .json(&json!({"email": "a@b.co"}))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let pk = json_of(
        t.c.post(format!("{}/v1/me/passkeys/register", t.base))
            .bearer_auth(&t.sess)
            .json(&a.registration(&opts))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let id = pk["id"].as_str().unwrap();
    let o = dev_session(&t.c, &t.base, "other@b.co").await;
    let del = |tok: String| {
        t.c.delete(format!("{}/v1/me/passkeys/{id}", t.base))
            .bearer_auth(tok)
            .send()
    };
    assert_eq!(del(o).await.unwrap().status(), 404);
    assert_eq!(del(t.sess.clone()).await.unwrap().status(), 204);
    assert_eq!(del(t.sess.clone()).await.unwrap().status(), 404);
}

#[tokio::test]
async fn auth_methods_reports_what_is_configured() {
    let methods = |cfg: Config| async move {
        let (base, c, _) = serve(cfg).await;
        let r = c
            .get(format!("{base}/v1/auth/methods"))
            .send()
            .await
            .unwrap(); // no auth header
        assert_eq!(r.status(), 200);
        r.json::<Value>().await.unwrap()
    };
    // test default: http public_url, no Google client IDs
    let v = methods(test_cfg()).await;
    assert_eq!(
        v,
        json!({"google": false, "passkeys": false, "passkey_rp_id": null})
    );
    // Google configured; an http public URL still means no passkeys
    let v = methods(Config {
        google_client_ids: vec!["g.apps.googleusercontent.com".into()],
        passkey_rp_id: "mk.test".into(),
        ..test_cfg()
    })
    .await;
    assert_eq!(
        v,
        json!({"google": true, "passkeys": false, "passkey_rp_id": null})
    );
    // https public URL whose host is the RP ID
    let v = methods(Config {
        public_url: "https://app.keyala.io".into(),
        passkey_rp_id: "app.keyala.io".into(),
        ..test_cfg()
    })
    .await;
    assert_eq!(
        v,
        json!({"google": false, "passkeys": true, "passkey_rp_id": "app.keyala.io"})
    );
    // https but the RP ID is another host (KEYALA_PASSKEY_RP_ID override): off
    let v = methods(Config {
        public_url: "https://staging.keyala.io".into(),
        passkey_rp_id: "keyala.io".into(),
        ..test_cfg()
    })
    .await;
    assert_eq!(v["passkeys"], false);
}

// ---------- Plaza ----------

/// (owner, reviewer): Plaza on; the only reviewer is the Google identity `google:rev-sub`, seeded onto
/// the `rev@b.co` account the way a verified Google sign-in would store it.
async fn plaza_server() -> (T, String) {
    let (base, c, state) = serve(Config {
        plaza_enabled: true,
        plaza_reviewers: vec!["google:rev-sub".into()],
        ..test_cfg()
    })
    .await;
    let owner = dev_session(&c, &base, "owner@b.co").await;
    let rev = dev_session(&c, &base, "rev@b.co").await;
    sqlx::query("UPDATE users SET google_sub='rev-sub' WHERE email='rev@b.co'")
        .execute(&state.db)
        .await
        .unwrap();
    (
        T {
            base,
            c,
            sess: owner,
            state,
        },
        rev,
    )
}

async fn plaza_app(t: &T, name: &str, homepage: Option<&str>) -> String {
    let r = call(
        t,
        M::POST,
        "/v1/me/apps",
        Some(&t.sess),
        Some(json!({"name": name, "homepage": homepage})),
    )
    .await;
    assert_eq!(r.status(), 200);
    let v = json_of(r).await;
    // the create response keeps its shape
    assert_eq!(v.as_object().unwrap().len(), 2);
    v["app_id"].as_str().unwrap().to_string()
}

async fn plaza_public(t: &T) -> Value {
    let r = call(t, M::GET, "/v1/plaza", None, None).await;
    assert_eq!(r.status(), 200);
    json_of(r).await
}

async fn plaza_mine(t: &T, id: &str) -> Value {
    let v = json_of(call(t, M::GET, "/v1/me/apps", Some(&t.sess), None).await).await;
    v.as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .unwrap()["plaza"]
        .clone()
}

async fn plaza_decide(t: &T, rev: &str, id: &str, d: &str, name: &str, hp: &str) -> Response {
    call(
        t,
        M::POST,
        &format!("/v1/plaza/review/{id}"),
        Some(rev),
        Some(json!({"decision": d, "name": name, "homepage": hp})),
    )
    .await
}

async fn plaza_post(t: &T, id: &str) -> u16 {
    call(
        t,
        M::POST,
        &format!("/v1/me/apps/{id}/plaza"),
        Some(&t.sess),
        None,
    )
    .await
    .status()
    .as_u16()
}

#[tokio::test]
async fn plaza_is_off_by_default_and_needs_listed_reviewers() {
    // flag off: nothing exists
    let t = T::new().await;
    let id = plaza_app(&t, "A", Some("https://a.dev/")).await;
    for (m, p) in [
        (M::GET, "/v1/plaza".to_string()),
        (M::GET, "/examples".to_string()),
        (M::GET, "/examples/".to_string()),
        (M::GET, "/examples/plaza.js".to_string()),
        (M::GET, "/v1/plaza/review".to_string()),
        (M::POST, format!("/v1/me/apps/{id}/plaza")),
        (M::DELETE, format!("/v1/me/apps/{id}/plaza")),
    ] {
        assert_eq!(
            call(&t, m, &p, Some(&t.sess), None).await.status(),
            404,
            "{p}"
        );
    }
    let me = json_of(call(&t, M::GET, "/v1/me", Some(&t.sess), None).await).await;
    assert_eq!(me["plaza_enabled"], false);
    assert_eq!(me["plaza_reviewer"], false);
    assert_eq!(me["plaza_review_id"], Value::Null);
    // flag on but reviewer list empty: nobody can review, so nothing can become public
    let (base, c, state) = serve(Config {
        plaza_enabled: true,
        ..test_cfg()
    })
    .await;
    let sess = dev_session(&c, &base, "x@b.co").await;
    let t2 = T {
        base,
        c,
        sess: sess.clone(),
        state,
    };
    assert_eq!(
        call(&t2, M::GET, "/v1/plaza/review", Some(&sess), None)
            .await
            .status(),
        404
    );
    assert_eq!(plaza_public(&t2).await, json!([]));
}

#[tokio::test]
async fn plaza_submit_review_and_public_list() {
    let (t, rev) = plaza_server().await;
    let other = dev_session(&t.c, &t.base, "other@b.co").await;
    let hp = "https://cool-app.dev/";
    let id = plaza_app(&t, "Cool", Some(hp)).await;
    let path = format!("/v1/me/apps/{id}/plaza");
    assert_eq!(plaza_mine(&t, &id).await, Value::Null);
    assert_eq!(plaza_public(&t).await, json!([]));

    // CORS: any origin may read the list; reviewer routes are not opened
    let r = call(&t, M::GET, "/v1/plaza", None, None).await;
    assert_eq!(r.headers()["access-control-allow-origin"], "*");
    let r = call(&t, M::OPTIONS, "/v1/plaza", None, None).await;
    assert_eq!(r.status(), 204);
    assert_eq!(r.headers()["access-control-allow-origin"], "*");
    let r = call(&t, M::GET, "/v1/plaza/review", None, None).await;
    assert!(r.headers().get("access-control-allow-origin").is_none());

    // only the owner submits
    assert_eq!(
        call(&t, M::POST, &path, Some(&other), None).await.status(),
        404
    );
    assert_eq!(call(&t, M::POST, &path, None, None).await.status(), 401);
    let r = call(&t, M::POST, &path, Some(&t.sess), None).await;
    assert_eq!(json_of(r).await, json!({"status": "pending"}));
    assert_eq!(plaza_mine(&t, &id).await, "pending");
    assert_eq!(plaza_post(&t, &id).await, 409);
    assert_eq!(plaza_public(&t).await, json!([]), "pending is not public");

    // the owner is not a reviewer and sees 404; the reviewer sees public fields only
    assert_eq!(
        call(&t, M::GET, "/v1/plaza/review", Some(&t.sess), None)
            .await
            .status(),
        404
    );
    assert_eq!(
        plaza_decide(&t, &t.sess, &id, "approve", "Cool", hp)
            .await
            .status(),
        404
    );
    assert_eq!(
        call(&t, M::GET, "/v1/plaza/review", None, None)
            .await
            .status(),
        401
    );
    let me = json_of(call(&t, M::GET, "/v1/me", Some(&rev), None).await).await;
    assert_eq!(me["plaza_enabled"], true);
    assert_eq!(me["plaza_reviewer"], true);
    assert_eq!(me["plaza_review_id"], "google:rev-sub");
    let q = json_of(call(&t, M::GET, "/v1/plaza/review", Some(&rev), None).await).await;
    assert_eq!(q.as_array().unwrap().len(), 1);
    let mut keys: Vec<_> = q[0].as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        ["app_id", "homepage", "name", "status", "submitted_at"]
    );
    assert_eq!(
        call(
            &t,
            M::GET,
            "/v1/plaza/review?status=bogus",
            Some(&rev),
            None
        )
        .await
        .status(),
        400
    );

    // a decision on text the reviewer did not see is refused and changes nothing
    for (n, h) in [("Other", hp), ("Cool", "https://evil.dev/")] {
        let r = plaza_decide(&t, &rev, &id, "approve", n, h).await;
        assert_eq!(r.status(), 409);
        assert_eq!(json_of(r).await["error"]["type"], "stale_submission");
    }
    assert_eq!(
        plaza_decide(&t, &rev, &id, "bogus", "Cool", hp)
            .await
            .status(),
        400
    );
    assert_eq!(plaza_mine(&t, &id).await, "pending");

    // the owner edits between the review screen and the click: approval is refused
    let put = |n: &'static str, h: &'static str| {
        let (t, id) = (&t, id.clone());
        async move {
            call(
                t,
                M::PUT,
                &format!("/v1/me/apps/{id}"),
                Some(&t.sess),
                Some(json!({"name": n, "homepage": h, "allowed_origins": null})),
            )
            .await
        }
    };
    assert_eq!(put("Cool", "https://cool-app.dev/new").await.status(), 200);
    assert_eq!(plaza_mine(&t, &id).await, Value::Null);
    assert_eq!(
        plaza_decide(&t, &rev, &id, "approve", "Cool", hp)
            .await
            .status(),
        409
    );
    assert_eq!(plaza_public(&t).await, json!([]));

    // resubmit the new text, approve it
    let hp = "https://cool-app.dev/new";
    assert_eq!(plaza_post(&t, &id).await, 200);
    let r = plaza_decide(&t, &rev, &id, "approve", "Cool", hp).await;
    assert_eq!(json_of(r).await["status"], "approved");
    assert_eq!(
        plaza_decide(&t, &rev, &id, "approve", "Cool", hp)
            .await
            .status(),
        409
    );
    assert_eq!(
        plaza_public(&t).await,
        json!([{"name": "Cool", "homepage": hp}])
    );
    assert_eq!(plaza_mine(&t, &id).await, "approved");
    assert_eq!(plaza_post(&t, &id).await, 409);

    // origins alone keep the listing; a new name or homepage drops it at once
    let r = call(
        &t,
        M::PUT,
        &format!("/v1/me/apps/{id}"),
        Some(&t.sess),
        Some(
            json!({"name": " Cool ", "homepage": hp, "allowed_origins": ["https://cool-app.dev"]}),
        ),
    )
    .await;
    assert_eq!(json_of(r).await["plaza"], "approved");
    assert_eq!(plaza_public(&t).await.as_array().unwrap().len(), 1);
    assert_eq!(put("Cooler", hp).await.status(), 200);
    assert_eq!(plaza_public(&t).await, json!([]));
    assert_eq!(plaza_mine(&t, &id).await, Value::Null);

    // reviewer reject / take-down; a rejected app may resubmit; the owner may withdraw
    assert_eq!(plaza_post(&t, &id).await, 200);
    assert_eq!(
        plaza_decide(&t, &rev, &id, "withdraw", "Cooler", hp)
            .await
            .status(),
        409,
        "withdraw needs approved"
    );
    assert_eq!(
        plaza_decide(&t, &rev, &id, "reject", "Cooler", hp)
            .await
            .status(),
        200
    );
    assert_eq!(plaza_mine(&t, &id).await, "rejected");
    let rej = json_of(
        call(
            &t,
            M::GET,
            "/v1/plaza/review?status=rejected",
            Some(&rev),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(rej.as_array().unwrap().len(), 1);
    assert_eq!(plaza_post(&t, &id).await, 200);
    assert_eq!(
        plaza_decide(&t, &rev, &id, "approve", "Cooler", hp)
            .await
            .status(),
        200
    );
    assert_eq!(
        plaza_decide(&t, &rev, &id, "withdraw", "Cooler", hp)
            .await
            .status(),
        200
    );
    assert_eq!(plaza_public(&t).await, json!([]));
    assert_eq!(plaza_post(&t, &id).await, 200);
    assert_eq!(
        call(&t, M::DELETE, &path, Some(&other), None)
            .await
            .status(),
        404
    );
    let r = call(&t, M::DELETE, &path, Some(&t.sess), None).await;
    assert_eq!(json_of(r).await, json!({"status": null}));
    assert_eq!(plaza_mine(&t, &id).await, Value::Null);

    // deleting an approved app hides it and removes it from review
    assert_eq!(plaza_post(&t, &id).await, 200);
    assert_eq!(
        plaza_decide(&t, &rev, &id, "approve", "Cooler", hp)
            .await
            .status(),
        200
    );
    assert_eq!(plaza_public(&t).await.as_array().unwrap().len(), 1);
    let r = call(
        &t,
        M::DELETE,
        &format!("/v1/me/apps/{id}"),
        Some(&t.sess),
        None,
    )
    .await;
    assert_eq!(r.status(), 200);
    assert_eq!(plaza_public(&t).await, json!([]));
    assert_eq!(
        plaza_decide(&t, &rev, &id, "withdraw", "Cooler", hp)
            .await
            .status(),
        404
    );
}

#[tokio::test]
async fn plaza_rejects_unsafe_homepages() {
    let (t, rev) = plaza_server().await;
    let none = plaza_app(&t, "NoHome", None).await;
    assert_eq!(plaza_post(&t, &none).await, 400);
    for bad in [
        "http://a.dev/",
        "https://localhost/",
        "https://127.0.0.1/",
        "https://a.dev@evil.dev/",
        "https://user:pw@a.dev/",
        "https://printer.local/",
        "https://[::1]/",
        "javascript:alert(1)",
        "https://a.dev/<script>",
        "https://0x7f.0x1/",
        "https://nodots/",
        "https://a.dev:abc/",
        "https://example.com:999999/",
        "https://a.dev:/<x",
        "https://127.1/",
        "https://a.dev/ x",
    ] {
        let id = plaza_app(&t, "Bad", Some(bad)).await;
        assert_eq!(plaza_post(&t, &id).await, 400, "{bad}");
    }
    // valid Unicode path and IDN host are accepted, and the raw string is kept as typed
    for ok in ["https://good.dev/日本語/ページ?q=é", "https://例え.jp/"] {
        let id = plaza_app(&t, "Uni", Some(ok)).await;
        assert_eq!(plaza_post(&t, &id).await, 200, "{ok}");
        let q = json_of(call(&t, M::GET, "/v1/plaza/review", Some(&rev), None).await).await;
        let row = q
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["app_id"] == id)
            .unwrap()
            .clone();
        assert_eq!(row["homepage"], ok);
        let r = plaza_decide(&t, &rev, &id, "approve", "Uni", ok).await;
        assert_eq!(r.status(), 200, "{ok}");
    }
    // even a pending row cannot be approved if its homepage is somehow not valid
    let id = plaza_app(&t, "Good", Some("https://good.dev:8443/p?q=1")).await;
    assert_eq!(plaza_post(&t, &id).await, 200);
    sqlx_query(
        &t.state,
        &format!("UPDATE apps SET homepage='http://good.dev/' WHERE id='{id}'"),
    )
    .await;
    let r = plaza_decide(&t, &rev, &id, "approve", "Good", "http://good.dev/").await;
    assert_eq!(r.status(), 400);
}

#[tokio::test]
async fn plaza_page_is_served_with_strict_csp() {
    let (t, _) = plaza_server().await;
    for p in ["/examples", "/examples/"] {
        let r = call(&t, M::GET, p, None, None).await;
        assert_eq!(r.status(), 200);
        let csp = r.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            csp.contains("script-src 'self'")
                && csp.contains("frame-ancestors 'none'")
                && !csp.contains("script-src 'unsafe")
        );
        assert!(r.text().await.unwrap().contains("community-plaza"));
    }
    let r = call(&t, M::GET, "/examples/plaza.js", None, None).await;
    assert_eq!(r.status(), 200);
    assert!(r.text().await.unwrap().contains("/v1/plaza"));
}

async fn me_of(t: &T, s: &str) -> Value {
    json_of(call(t, M::GET, "/v1/me", Some(s), None).await).await
}

#[tokio::test]
async fn plaza_reviewers_are_stable_verified_identities() {
    use sha2::{Digest, Sha256};
    let pw = "pw-long-random";
    let cfg = |url: Option<String>, list: &[&str]| Config {
        plaza_enabled: true,
        dev_login: true,
        password_login: Some(("ops".into(), Sha256::digest(pw).into())),
        plaza_reviewers: list.iter().map(|x| x.to_string()).collect(),
        database_url: url.unwrap_or_else(|| test_cfg().database_url),
        ..test_cfg()
    };
    let list = ["google:g1", "apple:a1", "password:ops", "g1", "x@y.co"];
    // the same verified subject is a reviewer on two fresh databases, under different user ids
    let mut uids = vec![];
    for _ in 0..2 {
        let (base, c, state) = serve(cfg(None, &list)).await;
        let t = T {
            base: base.clone(),
            c: c.clone(),
            sess: String::new(),
            state,
        };
        let g = dev_session(&c, &base, "g@b.co").await;
        sqlx::query("UPDATE users SET google_sub='g1' WHERE email='g@b.co'")
            .execute(&t.state.db)
            .await
            .unwrap();
        let me = me_of(&t, &g).await;
        assert_eq!(
            (me["plaza_reviewer"].clone(), me["plaza_review_id"].clone()),
            (json!(true), json!("google:g1"))
        );
        uids.push(me["id"].clone());
        // Apple, and the operator password identity
        let a = dev_session(&c, &base, "a@b.co").await;
        sqlx::query("UPDATE users SET apple_sub='a1' WHERE email='a@b.co'")
            .execute(&t.state.db)
            .await
            .unwrap();
        assert_eq!(me_of(&t, &a).await["plaza_review_id"], "apple:a1");
        let r = c
            .post(format!("{base}/v1/auth/password"))
            .json(&json!({"username": "ops", "password": pw}))
            .send()
            .await
            .unwrap();
        let ops = json_of(r).await["session_token"]
            .as_str()
            .unwrap()
            .to_string();
        let me = me_of(&t, &ops).await;
        assert_eq!(
            (me["plaza_reviewer"].clone(), me["plaza_review_id"].clone()),
            (json!(true), json!("password:ops"))
        );
        // dev login: no identity, never a reviewer, cannot reach the password identity by email
        let d = dev_session(&c, &base, "x@y.co").await;
        let me = me_of(&t, &d).await;
        assert_eq!(
            (me["plaza_reviewer"].clone(), me["plaza_review_id"].clone()),
            (json!(false), Value::Null)
        );
        let r = c
            .post(format!("{base}/v1/auth/dev"))
            .json(&json!({"email": "ops@password.invalid"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400);
        // same email, different provider subject: identity is the subject, not the email
        let e = dev_session(&c, &base, "same@b.co").await;
        sqlx::query("UPDATE users SET google_sub='g2' WHERE email='same@b.co'")
            .execute(&t.state.db)
            .await
            .unwrap();
        let me = me_of(&t, &e).await;
        assert_eq!(
            (me["plaza_reviewer"].clone(), me["plaza_review_id"].clone()),
            (json!(false), json!("google:g2"))
        );
        assert_eq!(
            call(&t, M::GET, "/v1/plaza/review", Some(&e), None)
                .await
                .status(),
            404
        );
    }
    assert_ne!(uids[0], uids[1], "user ids differ per database");
    // a former random user id in the list is no longer enough
    let (base, c, state) = serve(cfg(None, &[])).await;
    let s = dev_session(&c, &base, "u@b.co").await;
    let t = T {
        base: base.clone(),
        c: c.clone(),
        sess: s.clone(),
        state,
    };
    let uid = me_of(&t, &s).await["id"].as_str().unwrap().to_string();
    let (base, c, state) = serve(Config {
        database_url: t.state.cfg.database_url.clone(),
        ..cfg(None, &[&uid])
    })
    .await;
    let t = T {
        base,
        c,
        sess: s.clone(),
        state,
    };
    let me = me_of(&t, &s).await;
    assert_eq!(me["plaza_reviewer"], false);
    // flag off hides the id
    let (base, c, state) = serve(Config {
        plaza_enabled: false,
        ..cfg(None, &list)
    })
    .await;
    let g = dev_session(&c, &base, "g@b.co").await;
    sqlx::query("UPDATE users SET google_sub='g1' WHERE email='g@b.co'")
        .execute(&state.db)
        .await
        .unwrap();
    let t = T {
        base,
        c,
        sess: g.clone(),
        state,
    };
    let me = me_of(&t, &g).await;
    assert_eq!(
        (me["plaza_reviewer"].clone(), me["plaza_review_id"].clone()),
        (json!(false), Value::Null)
    );
}
