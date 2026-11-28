//! Keyala Coupon: claims, sponsored calls, money invariants, expiry/revoke, deletion, reports, email outbox.
//! No real provider or mail call is ever made: the vendor and the mail transport are mocks.
use keyala_server::{
    batch_key_aad, build_state, decrypt, process_outbox, reconcile_hold, router, sweep_coupons,
    AppState, Config,
};
use reqwest::{Client, Response};
use serde_json::{json, Value};
use std::{net::SocketAddr, time::Duration};

fn cfg(email_mock: bool) -> (Config, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("mk-coupon-{}", keyala_server::rand_b64(8)));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("masterkey.db");
    (
        Config {
            database_url: format!("sqlite://{}?mode=rwc", db.display()),
            master_key: [7u8; 32],
            dev_login: true,
            vendor_mock: true,
            public_url: "http://mk.test".into(),
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
            resend_api_key: None,
            email_from: None,
            email_mock,
            plaza_enabled: false,
            plaza_reviewers: vec![],
        },
        dir,
    )
}

struct E {
    base: String,
    c: Client,
    st: AppState,
    dir: std::path::PathBuf,
    owner: String,
    app_id: String,
    app_secret: String,
}

async fn env_with(cfg_pair: (Config, std::path::PathBuf)) -> E {
    let (cfg, dir) = cfg_pair;
    let st = build_state(cfg).await.unwrap();
    let app = router(st.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(l, app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .unwrap()
    });
    let c = Client::new();
    let owner = sess(&c, &base, "owner@x.co").await;
    let a: Value = c
        .post(format!("{base}/v1/me/apps"))
        .bearer_auth(&owner)
        .json(&json!({"name": "Sponsor App"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    E {
        base,
        c,
        st,
        dir,
        owner,
        app_id: a["app_id"].as_str().unwrap().into(),
        app_secret: a["app_secret"].as_str().unwrap().into(),
    }
}
async fn env() -> E {
    env_with(cfg(true)).await
}

async fn sess(c: &Client, base: &str, email: &str) -> String {
    let r: Value = c
        .post(format!("{base}/v1/auth/dev"))
        .json(&json!({"email": email}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    r["session_token"].as_str().unwrap().into()
}

async fn jv(r: Response) -> (u16, Value) {
    let s = r.status().as_u16();
    (s, r.json().await.unwrap_or(Value::Null))
}

fn deadline() -> String {
    (chrono::Utc::now() + chrono::Duration::days(3)).to_rfc3339()
}

impl E {
    async fn post(&self, tok: &str, p: &str, b: Value) -> (u16, Value) {
        jv(self
            .c
            .post(format!("{}{p}", self.base))
            .bearer_auth(tok)
            .json(&b)
            .send()
            .await
            .unwrap())
        .await
    }
    async fn get(&self, tok: &str, p: &str) -> (u16, Value) {
        jv(self
            .c
            .get(format!("{}{p}", self.base))
            .bearer_auth(tok)
            .send()
            .await
            .unwrap())
        .await
    }
    async fn patch(&self, tok: &str, p: &str, b: Value) -> (u16, Value) {
        jv(self
            .c
            .patch(format!("{}{p}", self.base))
            .bearer_auth(tok)
            .json(&b)
            .send()
            .await
            .unwrap())
        .await
    }
    async fn delete(&self, tok: &str, p: &str) -> (u16, Value) {
        jv(self
            .c
            .delete(format!("{}{p}", self.base))
            .bearer_auth(tok)
            .send()
            .await
            .unwrap())
        .await
    }
    async fn create_batch(&self, per: f64, total: f64) -> Value {
        let (s, b) = self
            .post(
                &self.owner,
                &format!("/v1/me/apps/{}/coupons", self.app_id),
                json!({"vendor": "openai", "models": ["gpt-4o-mini"], "api_key": "sk-test-BATCHKEY9999",
                       "per_coupon_usd": per, "total_usd": total, "claim_deadline": deadline()}),
            )
            .await;
        assert_eq!(s, 200, "{b}");
        b
    }
    async fn claim(&self, tok: &str, code: &str) -> (u16, Value) {
        self.post(tok, &format!("/v1/me/coupons/{code}/claim"), json!({}))
            .await
    }
    /// sponsored connect: the user has no key of their own. Returns (grant id, grant token).
    async fn connect(&self, tok: &str, code: &str) -> (String, String) {
        let (s, c) = jv(self
            .c
            .post(format!("{}/v1/connect", self.base))
            .bearer_auth(&self.app_secret)
            .json(&json!({"vendor": "openai", "coupon_code": code}))
            .send()
            .await
            .unwrap())
        .await;
        assert_eq!(s, 200, "{c}");
        let cc = c["connect_code"].as_str().unwrap();
        let (_, p) = self.get(tok, &format!("/v1/me/connect/{cc}")).await;
        assert!(p["coupon"]["batch_id"].as_str().is_some(), "{p}");
        let (s, g) = self
            .post(
                tok,
                &format!("/v1/me/connect/{cc}/approve"),
                json!({"policy": {"mode": "ask"}}),
            )
            .await;
        assert_eq!(s, 200, "{g}");
        let (_, p) = jv(self
            .c
            .get(format!("{}/v1/connect/{cc}", self.base))
            .bearer_auth(&self.app_secret)
            .send()
            .await
            .unwrap())
        .await;
        (
            g["id"].as_str().unwrap().into(),
            p["grant_token"].as_str().unwrap().into(),
        )
    }
    async fn chat(&self, grant: &str, content: &str, idem: Option<&str>) -> (u16, Value, String) {
        let mut r = self
            .c
            .post(format!("{}/v1/openai/chat/completions", self.base))
            .bearer_auth(grant)
            .header("X-Keyala-Wait", "0")
            .json(&json!({"model": "gpt-4o-mini", "messages": [{"role": "user", "content": content}], "max_tokens": 50}));
        if let Some(k) = idem {
            r = r.header("Idempotency-Key", k);
        }
        let r = r.send().await.unwrap();
        let id = r
            .headers()
            .get("x-keyala-request-id")
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        let (s, v) = jv(r).await;
        (s, v, id)
    }
    async fn wait(&self, tok: &str, rid: &str, want: &str) -> Value {
        for _ in 0..100 {
            let (_, r) = self.get(tok, &format!("/v1/me/requests/{rid}")).await;
            if r["status"] == want {
                return r;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("request never reached {want}");
    }
    fn pause(&self) {
        std::fs::write(self.dir.join("PAUSE_UPSTREAM"), "").unwrap();
    }
    async fn q<
        T: for<'r> sqlx::Decode<'r, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite> + Send + Unpin,
    >(
        &self,
        sql: &str,
        bind: &str,
    ) -> T {
        let mut q = sqlx::query_scalar(sql);
        if !bind.is_empty() {
            q = q.bind(bind);
        }
        q.fetch_one(&self.st.db).await.unwrap()
    }
    /// total = available + Σ(remaining + held) + spent, always
    async fn invariant(&self, batch: &str) {
        let (total, avail, spent): (i64, i64, i64) = sqlx::query_as(
            "SELECT total_micro, available_micro, spent_micro FROM coupon_batches WHERE id=?",
        )
        .bind(batch)
        .fetch_one(&self.st.db)
        .await
        .unwrap();
        let held: i64 = self
            .q(
                "SELECT COALESCE(SUM(remaining_micro+held_micro),0) FROM coupons WHERE batch_id=?",
                batch,
            )
            .await;
        assert_eq!(total, avail + held + spent, "money invariant broken");
    }
    async fn expire_coupon(&self, coupon: &str) {
        sqlx::query("UPDATE coupons SET expires_at='2000-01-01T00:00:00.000Z' WHERE id=?")
            .bind(coupon)
            .execute(&self.st.db)
            .await
            .unwrap();
    }
}

/// owner batch, one recipient with a coupon and a sponsored grant
async fn funded(per: f64, total: f64) -> (E, Value, String, String, String, String) {
    let e = env().await;
    let b = e.create_batch(per, total).await;
    let u = sess(&e.c, &e.base, "r@x.co").await;
    let (s, c) = e.claim(&u, b["coupon_code"].as_str().unwrap()).await;
    assert_eq!(s, 200, "{c}");
    let (_g, tok) = e.connect(&u, b["coupon_code"].as_str().unwrap()).await;
    let cid = c["id"].as_str().unwrap().to_string();
    (e, b, u, tok, cid, _g)
}

#[tokio::test]
async fn batch_key_is_encrypted_bound_and_never_listed() {
    let e = env().await;
    let b = e.create_batch(0.5, 2.0).await;
    assert_eq!(b["key_last4"], "9999");
    assert_eq!(b["email_enabled"], true);
    assert!(b["claim_url"]
        .as_str()
        .unwrap()
        .ends_with(&format!("/coupon/{}", b["coupon_code"].as_str().unwrap())));
    let code = b["coupon_code"].as_str().unwrap();
    assert!(code.starts_with("KC-") && code.len() == 25, "{code}");
    assert!(code[3..]
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'));
    let id = b["id"].as_str().unwrap();
    let (ct, n): (Vec<u8>, Vec<u8>) =
        sqlx::query_as("SELECT key_ct, key_nonce FROM coupon_batches WHERE id=?")
            .bind(id)
            .fetch_one(&e.st.db)
            .await
            .unwrap();
    assert!(!String::from_utf8_lossy(&ct).contains("BATCHKEY"));
    assert_eq!(
        decrypt(&[7u8; 32], &batch_key_aad(id, "openai"), &ct, &n).unwrap(),
        "sk-test-BATCHKEY9999"
    );
    // bound to its batch and vendor: another AAD does not open it
    assert!(decrypt(&[7u8; 32], &batch_key_aad(id, "anthropic"), &ct, &n).is_none());
    assert!(decrypt(&[7u8; 32], &batch_key_aad("cpb_other", "openai"), &ct, &n).is_none());
    // the personal key table is untouched
    let own: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_keys")
        .fetch_one(&e.st.db)
        .await
        .unwrap();
    assert_eq!(own, 0);
    let (s, list) = e
        .get(&e.owner, &format!("/v1/me/apps/{}/coupons", e.app_id))
        .await;
    assert_eq!(s, 200);
    assert!(!list.to_string().contains("BATCHKEY"));
    assert_eq!(list[0]["id"], id);
    // validation: unknown model, bad vendor key shape, total below one coupon, another user's app
    let url = format!("/v1/me/apps/{}/coupons", e.app_id);
    let ok = json!({"vendor": "openai", "models": ["gpt-4o-mini"], "api_key": "sk-test-BATCHKEY9999", "per_coupon_usd": 1.0, "total_usd": 2.0, "claim_deadline": deadline()});
    let mut bad = ok.clone();
    bad["models"] = json!(["gpt-nope"]);
    assert_eq!(e.post(&e.owner, &url, bad).await.0, 400);
    let mut bad = ok.clone();
    bad["api_key"] = json!("sk-ant-api-notopenai1");
    assert_eq!(e.post(&e.owner, &url, bad).await.0, 400);
    let mut bad = ok.clone();
    bad["total_usd"] = json!(0.5);
    assert_eq!(e.post(&e.owner, &url, bad).await.0, 400);
    let mut bad = ok.clone();
    bad["claim_deadline"] = json!("2001-01-01T00:00:00Z");
    assert_eq!(e.post(&e.owner, &url, bad).await.0, 400);
    let other = sess(&e.c, &e.base, "other@x.co").await;
    assert_eq!(e.post(&other, &url, ok.clone()).await.0, 404);
    assert_eq!(e.get(&other, &url).await.0, 404);
    assert_eq!(e.delete(&other, &format!("{url}/{id}")).await.0, 404);
    assert_eq!(e.get(&other, &format!("{url}/{id}/report")).await.0, 404);
    e.invariant(id).await;
}

#[tokio::test]
async fn claim_race_never_over_reserves_and_is_once_per_account() {
    let e = env().await;
    let b = e.create_batch(0.25, 1.0).await; // exactly four coupons
    let code = b["coupon_code"].as_str().unwrap().to_string();
    let mut toks = vec![];
    for i in 0..12 {
        toks.push(sess(&e.c, &e.base, &format!("u{i}@x.co")).await);
    }
    let mut hs = vec![];
    for t in toks.clone() {
        let (c, base, code) = (e.c.clone(), e.base.clone(), code.clone());
        hs.push(tokio::spawn(async move {
            jv(c.post(format!("{base}/v1/me/coupons/{code}/claim"))
                .bearer_auth(t)
                .json(&json!({}))
                .send()
                .await
                .unwrap())
            .await
        }));
    }
    let mut ok = 0;
    for h in hs {
        let (s, v) = h.await.unwrap();
        match s {
            200 => ok += 1,
            409 => assert_eq!(v["error"]["type"], "coupon_claim_limit"),
            x => panic!("unexpected {x} {v}"),
        }
    }
    assert_eq!(ok, 4);
    let id = b["id"].as_str().unwrap();
    assert_eq!(
        e.q::<i64>("SELECT available_micro FROM coupon_batches WHERE id=?", id)
            .await,
        0
    );
    assert_eq!(
        e.q::<i64>("SELECT COUNT(*) FROM coupons WHERE batch_id=?", id)
            .await,
        4
    );
    e.invariant(id).await;
    // the same account claiming again gets the same coupon, never a second slice
    let holder: String = e.q("SELECT u.email FROM coupons c JOIN users u ON u.id=c.user_id WHERE c.batch_id=? LIMIT 1", id).await;
    let t0 = &toks[holder
        .trim_start_matches('u')
        .split('@')
        .next()
        .unwrap()
        .parse::<usize>()
        .unwrap()];
    let (s1, first) = e.claim(t0, &code).await;
    let (s2, again) = e.claim(t0, &code).await;
    assert_eq!((s1, s2), (200, 200));
    assert_eq!(again["id"], first["id"]);
    assert_eq!(
        e.q::<i64>("SELECT COUNT(*) FROM coupons WHERE batch_id=?", id)
            .await,
        4
    );
    e.invariant(id).await;
}

#[tokio::test]
async fn claim_is_phone_only_idempotent_and_never_reissues_after_expiry() {
    let e = env().await;
    let b = e.create_batch(0.5, 1.0).await;
    let code = b["coupon_code"].as_str().unwrap();
    let u = sess(&e.c, &e.base, "r@x.co").await;
    let (s, pv) = e.get(&u, &format!("/v1/me/coupons/{code}")).await;
    assert_eq!(
        (s, pv["claimable"].clone(), pv["coupon"].clone()),
        (200, json!(true), Value::Null)
    );
    assert_eq!(pv["per_coupon_usd"], 0.5);
    // codes are case-sensitive and never normalised: another casing is another (unknown) code, everywhere
    let (s, v) = e
        .get(&u, &format!("/v1/me/coupons/{}", code.to_lowercase()))
        .await;
    assert_eq!(
        (s, v["error"]["type"].clone()),
        (404, json!("coupon_not_found"))
    );
    assert_eq!(e.claim(&u, &code.to_lowercase()).await.0, 404);
    assert_eq!(e.claim(&u, &code.to_uppercase()).await.0, 404);
    for bad in [
        "KC-short",
        "XX-AAAAAAAAAAAAAAAAAAAAAA",
        "KC-AAAAAAAAAAAAAAAAAAAAA!",
        &format!("{code}A"),
    ] {
        assert_eq!(
            e.get(&u, &format!("/v1/me/coupons/{bad}")).await.0,
            404,
            "{bad}"
        );
    }
    let r =
        e.c.get(format!("{}/coupon/{}", e.base, code.to_lowercase()))
            .send()
            .await
            .unwrap();
    assert_eq!(r.status(), 404, "landing page is case-sensitive too");
    let r =
        e.c.get(format!("{}/coupon/{code}/qr.svg", e.base))
            .send()
            .await
            .unwrap();
    assert_eq!(r.status(), 200);
    let (s, c1) = e.claim(&u, code).await;
    assert_eq!(s, 200);
    assert_eq!(c1["status"], "active");
    assert_eq!(c1["remaining_usd"], 0.5);
    assert_eq!(c1["app"]["id"], e.app_id.as_str());
    let (_, c2) = e.claim(&u, code).await;
    assert_eq!(c1["id"], c2["id"]);
    assert_eq!(
        e.q::<f64>(
            "SELECT available_micro/1000000.0 FROM coupon_batches WHERE id=?",
            b["id"].as_str().unwrap()
        )
        .await,
        0.5
    );
    let (_, mine) = e.get(&u, "/v1/me/coupons").await;
    assert_eq!(mine.as_array().unwrap().len(), 1);
    // expiry: the coupon reads expired, the unused balance returns once, and a new claim never reissues
    e.expire_coupon(c1["id"].as_str().unwrap()).await;
    sweep_coupons(&e.st).await.unwrap();
    sweep_coupons(&e.st).await.unwrap();
    assert_eq!(
        e.q::<f64>(
            "SELECT available_micro/1000000.0 FROM coupon_batches WHERE id=?",
            b["id"].as_str().unwrap()
        )
        .await,
        1.0
    );
    let (s, c3) = e.claim(&u, code).await;
    assert_eq!(
        (s, c3["id"].clone(), c3["status"].clone()),
        (200, c1["id"].clone(), json!("expired"))
    );
    assert_eq!(
        e.q::<f64>(
            "SELECT available_micro/1000000.0 FROM coupon_batches WHERE id=?",
            b["id"].as_str().unwrap()
        )
        .await,
        1.0
    );
    // a web session may look but not claim; unknown / malformed codes are 404
    sqlx::query(
        "UPDATE sessions SET scope='web' WHERE user_id=(SELECT id FROM users WHERE email='r@x.co')",
    )
    .execute(&e.st.db)
    .await
    .unwrap();
    assert_eq!(e.get(&u, &format!("/v1/me/coupons/{code}")).await.0, 200);
    let other = sess(&e.c, &e.base, "o@x.co").await;
    sqlx::query(
        "UPDATE sessions SET scope='web' WHERE user_id=(SELECT id FROM users WHERE email='o@x.co')",
    )
    .execute(&e.st.db)
    .await
    .unwrap();
    assert_eq!(e.claim(&other, code).await.0, 403);
    assert_eq!(e.get(&u, "/v1/me/coupons/KC-NOPE").await.0, 404);
    assert_eq!(
        e.get(&u, "/v1/me/coupons/KC-AAAAAAAAAAAAAAAAAAAAAA")
            .await
            .0,
        404
    );
    // no session, no access
    let r =
        e.c.get(format!("{}/v1/me/coupons/{code}", e.base))
            .send()
            .await
            .unwrap();
    assert_eq!(r.status(), 401);
}

#[tokio::test]
async fn a_deleted_app_cannot_be_claimed_connected_or_previewed() {
    let e = env().await;
    let b = e.create_batch(0.5, 2.0).await;
    let (code, bid) = (
        b["coupon_code"].as_str().unwrap(),
        b["id"].as_str().unwrap(),
    );
    let u = sess(&e.c, &e.base, "r@x.co").await;
    // the app is gone but its batch is (in this test, deliberately) not revoked: the claim itself must still refuse
    sqlx::query("UPDATE apps SET deleted_at=? WHERE id=?")
        .bind("2026-01-01T00:00:00.000Z")
        .bind(&e.app_id)
        .execute(&e.st.db)
        .await
        .unwrap();
    let (s, v) = e.claim(&u, code).await;
    assert_eq!(
        (s, v["error"]["type"].clone()),
        (404, json!("coupon_not_found")),
        "{v}"
    );
    assert_eq!(e.get(&u, &format!("/v1/me/coupons/{code}")).await.0, 404);
    assert_eq!(
        e.q::<i64>("SELECT available_micro FROM coupon_batches WHERE id=?", bid)
            .await,
        2_000_000
    );
    assert_eq!(e.q::<i64>("SELECT COUNT(*) FROM coupons", "").await, 0);
    // the issuer's own endpoints no longer see the app
    assert_eq!(
        e.get(&e.owner, &format!("/v1/me/apps/{}/coupons", e.app_id))
            .await
            .0,
        404
    );
    e.invariant(bid).await;
}

#[tokio::test]
async fn claim_deadline_and_revoke_close_claims() {
    let e = env().await;
    let b = e.create_batch(0.5, 5.0).await;
    let code = b["coupon_code"].as_str().unwrap();
    let id = b["id"].as_str().unwrap();
    sqlx::query("UPDATE coupon_batches SET claim_deadline='2000-01-01T00:00:00.000Z' WHERE id=?")
        .bind(id)
        .execute(&e.st.db)
        .await
        .unwrap();
    let u = sess(&e.c, &e.base, "r@x.co").await;
    let (s, v) = e.claim(&u, code).await;
    assert_eq!(
        (s, v["error"]["type"].clone()),
        (410, json!("coupon_expired"))
    );
    let (_, pv) = e.get(&u, &format!("/v1/me/coupons/{code}")).await;
    assert_eq!(pv["claimable"], false);
    sqlx::query("UPDATE coupon_batches SET claim_deadline='2099-01-01T00:00:00.000Z' WHERE id=?")
        .bind(id)
        .execute(&e.st.db)
        .await
        .unwrap();
    assert_eq!(e.claim(&u, code).await.0, 200);
    let (s, rb) = e
        .delete(&e.owner, &format!("/v1/me/apps/{}/coupons/{id}", e.app_id))
        .await;
    assert_eq!(s, 200);
    assert!(rb["revoked_at"].is_string());
    let u2 = sess(&e.c, &e.base, "r2@x.co").await;
    assert_eq!(
        e.claim(&u2, code).await.1["error"]["type"],
        "coupon_revoked"
    );
    // revoke returned the unused balance; a second revoke changes nothing
    assert_eq!(
        e.q::<i64>("SELECT available_micro FROM coupon_batches WHERE id=?", id)
            .await,
        5_000_000
    );
    assert_eq!(
        e.delete(&e.owner, &format!("/v1/me/apps/{}/coupons/{id}", e.app_id))
            .await
            .0,
        200
    );
    assert_eq!(
        e.q::<i64>("SELECT available_micro FROM coupon_batches WHERE id=?", id)
            .await,
        5_000_000
    );
    // the wiped key can never be used again
    assert_eq!(
        e.q::<i64>("SELECT key_ct IS NULL FROM coupon_batches WHERE id=?", id)
            .await,
        1
    );
    let (_, mine) = e.get(&u, "/v1/me/coupons").await;
    assert_eq!(mine[0]["status"], "revoked");
    e.invariant(id).await;
}

#[tokio::test]
async fn sponsored_connect_needs_no_key_and_every_call_waits_for_the_phone() {
    let e = env().await;
    let b = e.create_batch(1.0, 3.0).await;
    let code = b["coupon_code"].as_str().unwrap();
    let u = sess(&e.c, &e.base, "r@x.co").await;
    // the user also owns a personal key: it must never be used for sponsored calls
    assert_eq!(
        jv(e.c
            .put(format!("{}/v1/me/keys/openai", e.base))
            .bearer_auth(&u)
            .json(&json!({"api_key": "sk-test-OWNKEY0000"}))
            .send()
            .await
            .unwrap())
        .await
        .0,
        200
    );
    // connect with a coupon, no policy wider than ask
    let cc = jv(e
        .c
        .post(format!("{}/v1/connect", e.base))
        .bearer_auth(&e.app_secret)
        .json(&json!({"vendor": "openai", "coupon_code": code}))
        .send()
        .await
        .unwrap())
    .await
    .1;
    let cc = cc["connect_code"].as_str().unwrap();
    let (_, p) = e.get(&u, &format!("/v1/me/connect/{cc}")).await;
    assert_eq!(p["requires_ask"], true);
    assert_eq!(p["billing_vendor"], "openai");
    assert_eq!(p["coupon"]["claimable"], true);
    let (s, v) = e
        .post(
            &u,
            &format!("/v1/me/connect/{cc}/approve"),
            json!({"policy": {"mode": "budget", "remaining_usd": 5.0}}),
        )
        .await;
    assert_eq!(
        (s, v["error"]["type"].clone()),
        (403, json!("coupon_policy_required"))
    );
    // the refused approval left nothing behind: no coupon claimed yet
    assert_eq!(e.q::<i64>("SELECT COUNT(*) FROM coupons", "").await, 0);
    let (s, g) = e
        .post(
            &u,
            &format!("/v1/me/connect/{cc}/approve"),
            json!({"policy": {"mode": "ask"}}),
        )
        .await;
    assert_eq!(s, 200, "{g}");
    assert_eq!(g["coupon"]["status"], "active");
    assert_eq!(g["coupon"]["remaining_usd"], 1.0);
    assert!(g["coupon"]["models"]
        .as_array()
        .unwrap()
        .contains(&json!("gpt-4o-mini")));
    let gid = g["id"].as_str().unwrap();
    let tok = jv(e
        .c
        .get(format!("{}/v1/connect/{cc}", e.base))
        .bearer_auth(&e.app_secret)
        .send()
        .await
        .unwrap())
    .await
    .1["grant_token"]
        .as_str()
        .unwrap()
        .to_string();
    // policy can never be widened afterwards, by grant PATCH or by approve-request
    for pol in [
        json!({"mode": "trust"}),
        json!({"mode": "budget", "remaining_usd": 1.0}),
        json!({"mode": "count", "remaining_count": 5}),
    ] {
        let (s, v) = e
            .patch(&u, &format!("/v1/me/grants/{gid}"), json!({"policy": pol}))
            .await;
        assert_eq!(
            (s, v["error"]["type"].clone()),
            (403, json!("coupon_policy_required"))
        );
    }
    assert_eq!(
        e.patch(
            &u,
            &format!("/v1/me/grants/{gid}"),
            json!({"policy": {"mode": "ask"}})
        )
        .await
        .0,
        200
    );
    let (s, _, rid1) = e.chat(&tok, "hello one", None).await;
    assert_eq!(s, 202, "every sponsored call is held for approval");
    let (s, v) = e
        .post(
            &u,
            &format!("/v1/me/requests/{rid1}/approve"),
            json!({"policy": {"mode": "trust"}}),
        )
        .await;
    assert_eq!(
        (s, v["error"]["type"].clone()),
        (403, json!("coupon_policy_required"))
    );
    let (_, pending) = e.get(&u, &format!("/v1/me/requests/{rid1}")).await;
    assert_eq!(pending["status"], "pending");
    assert_eq!(
        pending["payload"]["messages"][0]["content"], "hello one",
        "the full prompt stays visible"
    );
    assert_eq!(pending["coupon"]["id"], g["coupon"]["id"]);
    let (s, _) = e
        .post(&u, &format!("/v1/me/requests/{rid1}/approve"), json!({}))
        .await;
    assert_eq!(s, 200);
    let done = e.wait(&u, &rid1, "completed").await;
    // the mock echoes the last 4 characters of the key that was used: the batch's, never the user's
    let (_, poll) = jv(e
        .c
        .get(format!("{}/v1/requests/{rid1}", e.base))
        .bearer_auth(&tok)
        .send()
        .await
        .unwrap())
    .await;
    assert_eq!(poll["response"]["mock_key_last4"], "9999");
    assert!(done["actual_usd"].as_f64().unwrap() > 0.0);
    // the next call is pending again, whatever happened before
    let (s, _, rid2) = e.chat(&tok, "hello two", None).await;
    assert_eq!(s, 202);
    assert_eq!(e.wait(&u, &rid2, "pending").await["status"], "pending");
    // the coupon was charged the real cost; the rest stayed available
    let (_, mine) = e.get(&u, "/v1/me/coupons").await;
    let spent = done["actual_usd"].as_f64().unwrap();
    assert!((mine[0]["remaining_usd"].as_f64().unwrap() - (1.0 - spent)).abs() < 1e-9);
    e.invariant(b["id"].as_str().unwrap()).await;
    // deleting the sponsored key situation: no personal key is needed and none was touched
    assert_eq!(e.q::<i64>("SELECT COUNT(*) FROM api_keys WHERE user_id=(SELECT id FROM users WHERE email='r@x.co')", "").await, 1);
}

#[tokio::test]
async fn sponsored_connect_works_for_a_user_with_no_keys_at_all() {
    let (e, b, u, tok, _cid, _g) = funded(0.5, 1.0).await;
    assert_eq!(e.q::<i64>("SELECT COUNT(*) FROM api_keys", "").await, 0);
    let (s, _, rid) = e.chat(&tok, "no key anywhere", None).await;
    assert_eq!(s, 202);
    assert_eq!(
        e.post(&u, &format!("/v1/me/requests/{rid}/approve"), json!({}))
            .await
            .0,
        200
    );
    e.wait(&u, &rid, "completed").await;
    e.invariant(b["id"].as_str().unwrap()).await;
}

#[tokio::test]
async fn binding_to_app_vendor_model_and_account() {
    let e = env().await;
    let b = e.create_batch(1.0, 3.0).await;
    let code = b["coupon_code"].as_str().unwrap();
    // another app cannot attach this coupon
    let app2: Value =
        e.c.post(format!("{}/v1/apps", e.base))
            .json(&json!({"name": "Other"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    let s2 = app2["app_secret"].as_str().unwrap();
    let (s, v) = jv(e
        .c
        .post(format!("{}/v1/connect", e.base))
        .bearer_auth(s2)
        .json(&json!({"vendor": "openai", "coupon_code": code}))
        .send()
        .await
        .unwrap())
    .await;
    assert_eq!(
        (s, v["error"]["type"].clone()),
        (404, json!("coupon_not_found"))
    );
    // the right app, the wrong vendor
    let (s, v) = jv(e
        .c
        .post(format!("{}/v1/connect", e.base))
        .bearer_auth(&e.app_secret)
        .json(&json!({"vendor": "anthropic", "coupon_code": code}))
        .send()
        .await
        .unwrap())
    .await;
    assert_eq!(
        (s, v["error"]["type"].clone()),
        (404, json!("coupon_not_found"))
    );
    assert_eq!(
        jv(e.c
            .post(format!("{}/v1/connect", e.base))
            .bearer_auth(&e.app_secret)
            .json(&json!({"vendor": "openai", "coupon_code": "KC-AAAAAAAAAAAAAAAAAAAAAA"}))
            .send()
            .await
            .unwrap())
        .await
        .0,
        404
    );
    let u = sess(&e.c, &e.base, "r@x.co").await;
    e.claim(&u, code).await;
    let (_g, tok) = e.connect(&u, code).await;
    // a model outside the batch list is refused before anything is held
    let r = e.c.post(format!("{}/v1/openai/chat/completions", e.base)).bearer_auth(&tok).header("X-Keyala-Wait", "0")
        .json(&json!({"model": "gpt-4o", "messages": [{"role": "user", "content": "hi"}], "max_tokens": 50})).send().await.unwrap();
    let (s, v) = jv(r).await;
    assert_eq!(
        (s, v["error"]["type"].clone()),
        (403, json!("coupon_model_not_allowed"))
    );
    // the anthropic route with an openai grant: vendor mismatch as before
    let r = e.c.post(format!("{}/v1/anthropic/messages", e.base)).bearer_auth(&tok).json(&json!({"model": "claude-haiku-5-5", "max_tokens": 5, "messages": [{"role": "user", "content": "x"}]})).send().await.unwrap();
    assert_eq!(r.status(), 403);
    // too big for the coupon: refused up front
    let r = e.c.post(format!("{}/v1/openai/chat/completions", e.base)).bearer_auth(&tok).header("X-Keyala-Wait", "0")
        .json(&json!({"model": "gpt-4o-mini", "messages": [{"role": "user", "content": "hi"}], "max_tokens": 4096})).send().await.unwrap();
    assert_eq!(r.status(), 202, "0.0025 USD fits in a 1 USD coupon");
    // an unrelated account has no coupon and cannot see or approve it
    let thief = sess(&e.c, &e.base, "t@x.co").await;
    assert_eq!(
        e.get(&thief, "/v1/me/coupons")
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let (_, pending) = e.get(&u, "/v1/me/requests?status=pending").await;
    let rid = pending[0]["id"].as_str().unwrap();
    assert_eq!(
        e.post(&thief, &format!("/v1/me/requests/{rid}/approve"), json!({}))
            .await
            .0,
        404
    );
    // a grant is bound to its coupon's account: tampering with the row means the call is refused at send time
    sqlx::query(
        "UPDATE coupons SET user_id=(SELECT id FROM users WHERE email='t@x.co') WHERE batch_id=?",
    )
    .bind(b["id"].as_str().unwrap())
    .execute(&e.st.db)
    .await
    .unwrap();
    let (s, v) = e
        .post(&u, &format!("/v1/me/requests/{rid}/approve"), json!({}))
        .await;
    assert_eq!(
        (s, v["error"]["type"].clone()),
        (404, json!("coupon_not_found")),
        "{v}"
    );
}

#[tokio::test]
async fn coupon_balance_is_checked_at_creation_and_reserved_at_approval() {
    let (e, b, u, tok, cid, _g) = funded(0.001, 0.001).await;
    // 0.001 USD cannot fund a 4096-token answer
    let r = e.c.post(format!("{}/v1/openai/chat/completions", e.base)).bearer_auth(&tok).header("X-Keyala-Wait", "0")
        .json(&json!({"model": "gpt-4o-mini", "messages": [{"role": "user", "content": "hi"}], "max_tokens": 4096})).send().await.unwrap();
    let (s, v) = jv(r).await;
    assert_eq!(
        (s, v["error"]["type"].clone()),
        (402, json!("coupon_exhausted"))
    );
    // two pending calls fit one at a time: approving the first holds its estimate, the second may then be refused
    let big = |c: &str| json!({"model": "gpt-4o-mini", "messages": [{"role": "user", "content": c}], "max_tokens": 1000});
    let mut rids = vec![];
    for c in ["a", "b"] {
        let r =
            e.c.post(format!("{}/v1/openai/chat/completions", e.base))
                .bearer_auth(&tok)
                .header("X-Keyala-Wait", "0")
                .json(&big(c))
                .send()
                .await
                .unwrap();
        rids.push(
            r.headers()
                .get("x-keyala-request-id")
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
        );
    }
    e.pause();
    let (s, a) = e
        .post(
            &u,
            &format!("/v1/me/requests/{}/approve", rids[0]),
            json!({}),
        )
        .await;
    assert_eq!(s, 200, "{a}");
    let est = a["estimated_usd"].as_f64().unwrap();
    assert!(est > 0.0005 && est < 0.001);
    assert_eq!(
        e.q::<i64>("SELECT held_micro FROM coupons WHERE id=?", &cid)
            .await,
        (est * 1e6).round() as i64
    );
    // the second call's estimate no longer fits what is left
    let (s, v) = e
        .post(
            &u,
            &format!("/v1/me/requests/{}/approve", rids[1]),
            json!({}),
        )
        .await;
    assert_eq!(
        (s, v["error"]["type"].clone()),
        (402, json!("coupon_exhausted"))
    );
    assert_eq!(
        e.get(&u, &format!("/v1/me/requests/{}", rids[1])).await.1["status"],
        "pending",
        "a refused approval changes nothing"
    );
    // approving the same request again never reserves twice
    let (s, _) = e
        .post(
            &u,
            &format!("/v1/me/requests/{}/approve", rids[0]),
            json!({}),
        )
        .await;
    assert_eq!(s, 409);
    assert_eq!(
        e.q::<i64>("SELECT COUNT(*) FROM coupon_ledger WHERE kind='hold'", "")
            .await,
        1
    );
    e.invariant(b["id"].as_str().unwrap()).await;
}

#[tokio::test]
async fn revoke_and_expiry_race_with_pending_and_approved_calls() {
    let (e, b, u, tok, cid, _g) = funded(1.0, 1.0).await;
    let bid = b["id"].as_str().unwrap();
    let (_, _, pending_id) = e.chat(&tok, "stays pending", None).await;
    let (_, _, approved_id) = e.chat(&tok, "approved but unsent", None).await;
    e.pause(); // approval holds the estimate but nothing is sent
    assert_eq!(
        e.post(
            &u,
            &format!("/v1/me/requests/{approved_id}/approve"),
            json!({})
        )
        .await
        .0,
        200
    );
    assert!(
        e.q::<i64>("SELECT held_micro FROM coupons WHERE id=?", &cid)
            .await
            > 0
    );
    // expiry: the sweep cancels both waiting calls, releases the hold and returns the balance, exactly once
    e.expire_coupon(&cid).await;
    sweep_coupons(&e.st).await.unwrap();
    sweep_coupons(&e.st).await.unwrap();
    assert_eq!(
        e.get(&u, &format!("/v1/me/requests/{pending_id}")).await.1["status"],
        "denied"
    );
    assert_eq!(
        e.get(&u, &format!("/v1/me/requests/{approved_id}")).await.1["status"],
        "denied"
    );
    assert_eq!(
        e.q::<i64>("SELECT available_micro FROM coupon_batches WHERE id=?", bid)
            .await,
        1_000_000
    );
    assert_eq!(
        e.q::<i64>("SELECT held_micro FROM coupons WHERE id=?", &cid)
            .await,
        0
    );
    assert_eq!(
        e.q::<i64>(
            "SELECT COUNT(*) FROM coupon_ledger WHERE kind IN ('return','release')",
            ""
        )
        .await,
        2
    );
    e.invariant(bid).await;
    // approving on an expired coupon is refused and creating a call is refused
    std::fs::remove_file(e.dir.join("PAUSE_UPSTREAM")).unwrap();
    let (s, v, _) = e.chat(&tok, "too late", None).await;
    assert_eq!(
        (s, v["error"]["type"].clone()),
        (410, json!("coupon_expired"))
    );
}

#[tokio::test]
async fn approve_after_expiry_or_revoke_is_refused_and_leaves_the_call_pending() {
    let (e, b, u, tok, cid, _g) = funded(1.0, 1.0).await;
    let (_, _, rid) = e.chat(&tok, "x", None).await;
    e.expire_coupon(&cid).await; // expired but not yet swept: the guarded hold still refuses
    let (s, v) = e
        .post(&u, &format!("/v1/me/requests/{rid}/approve"), json!({}))
        .await;
    assert_eq!(
        (s, v["error"]["type"].clone()),
        (410, json!("coupon_expired"))
    );
    assert_eq!(
        e.get(&u, &format!("/v1/me/requests/{rid}")).await.1["status"],
        "pending"
    );
    assert_eq!(
        e.q::<i64>("SELECT COUNT(*) FROM coupon_ledger WHERE kind='hold'", "")
            .await,
        0,
        "the refused hold left no ledger row"
    );
    // revoke cancels the waiting call
    let bid = b["id"].as_str().unwrap();
    let (s, _) = e
        .delete(&e.owner, &format!("/v1/me/apps/{}/coupons/{bid}", e.app_id))
        .await;
    assert_eq!(s, 200);
    assert_eq!(
        e.get(&u, &format!("/v1/me/requests/{rid}")).await.1["status"],
        "denied"
    );
    e.invariant(bid).await;
}

#[tokio::test]
async fn revoke_before_send_denies_an_approved_call_and_releases_its_hold() {
    let (e, b, u, tok, cid, _g) = funded(1.0, 1.0).await;
    let bid = b["id"].as_str().unwrap();
    let (_, _, rid) = e.chat(&tok, "x", None).await;
    e.pause();
    assert_eq!(
        e.post(&u, &format!("/v1/me/requests/{rid}/approve"), json!({}))
            .await
            .0,
        200
    );
    assert_eq!(
        e.get(&u, &format!("/v1/me/requests/{rid}")).await.1["status"],
        "approved"
    );
    let (s, _) = e
        .delete(&e.owner, &format!("/v1/me/apps/{}/coupons/{bid}", e.app_id))
        .await;
    assert_eq!(s, 200);
    assert_eq!(
        e.get(&u, &format!("/v1/me/requests/{rid}")).await.1["status"],
        "denied"
    );
    assert_eq!(
        e.q::<i64>("SELECT held_micro FROM coupons WHERE id=?", &cid)
            .await,
        0
    );
    assert_eq!(
        e.q::<i64>("SELECT available_micro FROM coupon_batches WHERE id=?", bid)
            .await,
        1_000_000
    );
    // unpausing never sends it: it is denied
    std::fs::remove_file(e.dir.join("PAUSE_UPSTREAM")).unwrap();
    keyala_server::send_request(e.st.clone(), rid.clone()).await;
    assert_eq!(
        e.get(&u, &format!("/v1/me/requests/{rid}")).await.1["status"],
        "denied"
    );
    assert_eq!(
        e.q::<i64>(
            "SELECT COUNT(*) FROM coupon_ledger WHERE kind='release'",
            ""
        )
        .await,
        1
    );
    e.invariant(bid).await;
}

#[tokio::test]
async fn repeating_a_call_never_spends_or_refunds_twice() {
    let (e, b, u, tok, cid, _g) = funded(1.0, 1.0).await;
    let (s1, _, r1) = e.chat(&tok, "same body", Some("idem-1")).await;
    let (s2, _, r2) = e.chat(&tok, "same body", Some("idem-1")).await;
    assert_eq!((s1, s2), (202, 202));
    assert_eq!(r1, r2);
    // a different body under the same key is a conflict
    assert_eq!(e.chat(&tok, "other body", Some("idem-1")).await.0, 409);
    assert_eq!(
        e.post(&u, &format!("/v1/me/requests/{r1}/approve"), json!({}))
            .await
            .0,
        200
    );
    let done = e.wait(&u, &r1, "completed").await;
    // retries after completion return the stored outcome and charge nothing more
    let (s3, v3, r3) = e.chat(&tok, "same body", Some("idem-1")).await;
    assert_eq!((s3, r3), (200, r1.clone()));
    assert_eq!(v3["mock_key_last4"], "9999");
    let (s, _) = e
        .post(&u, &format!("/v1/me/requests/{r1}/approve"), json!({}))
        .await;
    assert_eq!(s, 409);
    let charged = (done["actual_usd"].as_f64().unwrap() * 1e6).round() as i64;
    assert_eq!(
        e.q::<i64>("SELECT spent_micro FROM coupons WHERE id=?", &cid)
            .await,
        charged
    );
    assert_eq!(
        e.q::<i64>(
            "SELECT spent_micro FROM coupon_batches WHERE id=?",
            b["id"].as_str().unwrap()
        )
        .await,
        charged
    );
    assert_eq!(
        e.q::<i64>("SELECT COUNT(*) FROM coupon_ledger WHERE kind='settle'", "")
            .await,
        1
    );
    assert_eq!(
        e.q::<i64>("SELECT COUNT(*) FROM coupon_ledger WHERE kind='hold'", "")
            .await,
        1
    );
    e.invariant(b["id"].as_str().unwrap()).await;
}

#[tokio::test]
async fn missing_usage_charges_the_estimate_and_failures_release_it() {
    let (e, b, u, tok, cid, _g) = funded(1.0, 1.0).await;
    let bid = b["id"].as_str().unwrap();
    // success without a usage block: the estimate is charged in full
    let (_, _, r1) = e.chat(&tok, "KEYALA_MOCK_NOUSAGE please", None).await;
    let (_, a) = e
        .post(&u, &format!("/v1/me/requests/{r1}/approve"), json!({}))
        .await;
    let est = (a["estimated_usd"].as_f64().unwrap() * 1e6).round() as i64;
    let done = e.wait(&u, &r1, "completed").await;
    assert!(done["actual_usd"].is_null());
    assert_eq!(
        e.q::<i64>("SELECT spent_micro FROM coupons WHERE id=?", &cid)
            .await,
        est
    );
    assert_eq!(
        e.q::<i64>("SELECT remaining_micro FROM coupons WHERE id=?", &cid)
            .await,
        1_000_000 - est
    );
    // a provider failure costs nothing: the hold goes back
    let (_, _, r2) = e.chat(&tok, "KEYALA_MOCK_FAIL please", None).await;
    e.post(&u, &format!("/v1/me/requests/{r2}/approve"), json!({}))
        .await;
    e.wait(&u, &r2, "failed").await;
    assert_eq!(
        e.q::<i64>("SELECT spent_micro FROM coupons WHERE id=?", &cid)
            .await,
        est
    );
    assert_eq!(
        e.q::<i64>("SELECT held_micro FROM coupons WHERE id=?", &cid)
            .await,
        0
    );
    assert_eq!(
        e.q::<i64>("SELECT remaining_micro FROM coupons WHERE id=?", &cid)
            .await,
        1_000_000 - est
    );
    e.invariant(bid).await;
}

#[tokio::test]
async fn uncertain_calls_stay_held_and_late_completion_settles_once() {
    let (e, b, u, tok, cid, _g) = funded(1.0, 1.0).await;
    let bid = b["id"].as_str().unwrap();
    let (_, _, rid) = e.chat(&tok, "maybe sent", None).await;
    e.pause();
    let (_, a) = e
        .post(&u, &format!("/v1/me/requests/{rid}/approve"), json!({}))
        .await;
    let est = (a["estimated_usd"].as_f64().unwrap() * 1e6).round() as i64;
    // what a crash mid-send leaves behind
    sqlx::query("UPDATE requests SET status='uncertain' WHERE id=?")
        .bind(&rid)
        .execute(&e.st.db)
        .await
        .unwrap();
    // the coupon expires and is swept: only the unused part returns, the uncertain hold is retained
    e.expire_coupon(&cid).await;
    sweep_coupons(&e.st).await.unwrap();
    sweep_coupons(&e.st).await.unwrap();
    assert_eq!(
        e.q::<i64>("SELECT held_micro FROM coupons WHERE id=?", &cid)
            .await,
        est
    );
    assert_eq!(
        e.q::<i64>("SELECT available_micro FROM coupon_batches WHERE id=?", bid)
            .await,
        1_000_000 - est
    );
    e.invariant(bid).await;
    let report = format!("/v1/me/apps/{}/coupons/{bid}/report", e.app_id);
    let (_, rep) = e.get(&e.owner, &report).await;
    assert_eq!(
        (rep["money"]["uncertain_held_usd"].as_f64().unwrap() * 1e6).round() as i64,
        est,
        "{rep}"
    );
    assert_eq!(rep["money"]["actual_cost_usd"], 0.0);
    assert_eq!(rep["requests"]["uncertain"], 1);
    sqlx::query("UPDATE requests SET http_status=504, response='uncertain-body' WHERE id=?")
        .bind(&rid)
        .execute(&e.st.db)
        .await
        .unwrap();
    let http_before = 504;
    // it turns out to have completed for less than the estimate: the difference goes straight back to the batch
    let actual = est / 2;
    assert!(reconcile_hold(&e.st, &rid, true, Some(actual))
        .await
        .unwrap());
    assert!(
        !reconcile_hold(&e.st, &rid, true, Some(actual))
            .await
            .unwrap(),
        "second completion is a no-op"
    );
    assert!(
        !reconcile_hold(&e.st, &rid, false, None).await.unwrap(),
        "and so is a contradicting one"
    );
    assert_eq!(
        e.q::<i64>("SELECT held_micro FROM coupons WHERE id=?", &cid)
            .await,
        0
    );
    assert_eq!(
        e.q::<i64>("SELECT spent_micro FROM coupon_batches WHERE id=?", bid)
            .await,
        actual
    );
    assert_eq!(
        e.q::<i64>("SELECT available_micro FROM coupon_batches WHERE id=?", bid)
            .await,
        1_000_000 - actual
    );
    e.invariant(bid).await;
    // delivery stays unknown: the request keeps its uncertain status, 504 and body; only the money is settled
    let (_, r) = e.get(&u, &format!("/v1/me/requests/{rid}")).await;
    assert_eq!(r["status"], "uncertain", "{r}");
    let (st, http): (String, i64) =
        sqlx::query_as("SELECT status, http_status FROM requests WHERE id=?")
            .bind(&rid)
            .fetch_one(&e.st.db)
            .await
            .unwrap();
    assert_eq!((st.as_str(), http), ("uncertain", http_before));
    assert_eq!(
        e.q::<String>("SELECT response FROM requests WHERE id=?", &rid)
            .await,
        "uncertain-body",
        "the delivery result is not rewritten"
    );
    assert_eq!(
        e.q::<i64>("SELECT spent_micro FROM grants WHERE id=?", &_g)
            .await,
        actual,
        "the grant total reflects the settled cost"
    );
    let (_, rep) = e.get(&e.owner, &report).await;
    assert_eq!(rep["money"]["uncertain_held_usd"], 0.0, "{rep}");
    assert_eq!(
        (rep["money"]["actual_cost_usd"].as_f64().unwrap() * 1e6).round() as i64,
        actual
    );
    assert_eq!(rep["requests"]["uncertain"], 1, "delivery is still unknown");
    assert_eq!(rep["uncertain_requests_accounted"], 1);
    assert_eq!(rep["first_successful_users"], 1);
}

#[tokio::test]
async fn late_failure_refunds_and_overshoot_is_recorded_and_stops_the_batch() {
    let (e, b, u, tok, cid, _g) = funded(0.5, 2.0).await;
    let bid = b["id"].as_str().unwrap();
    // a late failure: nothing was done, everything is back
    let (_, _, r1) = e.chat(&tok, "never reached the provider", None).await;
    e.pause();
    e.post(&u, &format!("/v1/me/requests/{r1}/approve"), json!({}))
        .await;
    sqlx::query("UPDATE requests SET status='uncertain' WHERE id=?")
        .bind(&r1)
        .execute(&e.st.db)
        .await
        .unwrap();
    assert!(reconcile_hold(&e.st, &r1, false, None).await.unwrap());
    assert_eq!(
        e.q::<i64>("SELECT remaining_micro FROM coupons WHERE id=?", &cid)
            .await,
        500_000
    );
    assert_eq!(
        e.q::<i64>("SELECT spent_micro FROM coupon_batches WHERE id=?", bid)
            .await,
        0
    );
    e.invariant(bid).await;
    // an actual far above its held estimate: charged truthfully, taken from the coupon then the batch, batch stopped
    std::fs::remove_file(e.dir.join("PAUSE_UPSTREAM")).unwrap();
    let (_, _, r2) = e.chat(&tok, "costs more than held", None).await;
    e.pause();
    e.post(&u, &format!("/v1/me/requests/{r2}/approve"), json!({}))
        .await;
    sqlx::query("UPDATE requests SET status='uncertain' WHERE id=?")
        .bind(&r2)
        .execute(&e.st.db)
        .await
        .unwrap();
    let held: i64 = e.q("SELECT held_micro FROM coupons WHERE id=?", &cid).await;
    let actual = 900_000;
    assert!(reconcile_hold(&e.st, &r2, true, Some(actual))
        .await
        .unwrap());
    let over = actual - held;
    assert_eq!(
        e.q::<i64>("SELECT spent_micro FROM coupon_batches WHERE id=?", bid)
            .await,
        actual
    );
    assert_eq!(
        e.q::<i64>("SELECT overshoot_micro FROM coupon_batches WHERE id=?", bid)
            .await,
        over
    );
    assert_eq!(
        e.q::<i64>("SELECT remaining_micro FROM coupons WHERE id=?", &cid)
            .await,
        0
    );
    assert_eq!(
        e.q::<i64>(
            "SELECT stopped_at IS NOT NULL FROM coupon_batches WHERE id=?",
            bid
        )
        .await,
        1
    );
    e.invariant(bid).await;
    // the stopped batch accepts no claims and no calls
    let u2 = sess(&e.c, &e.base, "late@x.co").await;
    assert_eq!(
        e.claim(&u2, b["coupon_code"].as_str().unwrap()).await.1["error"]["type"],
        "coupon_exhausted"
    );
    std::fs::remove_file(e.dir.join("PAUSE_UPSTREAM")).unwrap();
    let (s, v, _) = e.chat(&tok, "again", None).await;
    assert_eq!(
        (s, v["error"]["type"].clone()),
        (402, json!("coupon_exhausted"))
    );
    // a stop below the total says "stopped", never "fully spent" (2.0 USD total, 0.9 USD charged)
    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT kind FROM coupon_outbox WHERE batch_id=? ORDER BY kind")
            .bind(bid)
            .fetch_all(&e.st.db)
            .await
            .unwrap();
    assert!(kinds.contains(&"stopped".to_string()), "{kinds:?}");
    assert!(!kinds.contains(&"exhausted".to_string()), "{kinds:?}");
}

#[tokio::test]
async fn deleting_a_recipient_keeps_unknown_money_held_anonymously_and_reconciles_by_request_id() {
    let (e, b, u, tok, cid, _g) = funded(1.0, 2.0).await;
    let bid = b["id"].as_str().unwrap();
    let (_, _, waiting) = e.chat(&tok, "waiting", None).await;
    let (_, _, unknown) = e.chat(&tok, "outcome unknown", None).await;
    e.pause();
    let (_, a) = e
        .post(&u, &format!("/v1/me/requests/{unknown}/approve"), json!({}))
        .await;
    let est = (a["estimated_usd"].as_f64().unwrap() * 1e6).round() as i64;
    sqlx::query("UPDATE requests SET status='uncertain' WHERE id=?")
        .bind(&unknown)
        .execute(&e.st.db)
        .await
        .unwrap();
    let _ = waiting;
    let uid: String = e.q("SELECT id FROM users WHERE email=?", "r@x.co").await;
    let (s, v) = e
        .post(
            &u,
            "/v1/me/delete",
            json!({"confirm": "DELETE", "email": "r@x.co"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(e.q::<i64>("SELECT COUNT(*) FROM requests", "").await, 0);
    assert_eq!(e.q::<i64>("SELECT COUNT(*) FROM grants", "").await, 0);
    // the unknown call is NOT classified as spent: its hold survives on a coupon without a user
    let (uid_left, held, rem, closed): (Option<String>, i64, i64, bool) = sqlx::query_as(
        "SELECT user_id, held_micro, remaining_micro, closed_at IS NOT NULL FROM coupons WHERE id=?",
    )
    .bind(&cid)
    .fetch_one(&e.st.db)
    .await
    .unwrap();
    assert_eq!((uid_left, held, rem, closed), (None, est, 0, true));
    assert_eq!(e.q::<i64>("SELECT COUNT(*) FROM coupons", "").await, 1);
    assert_eq!(
        e.q::<i64>("SELECT spent_micro FROM coupon_batches WHERE id=?", bid)
            .await,
        0
    );
    assert_eq!(
        e.q::<i64>("SELECT available_micro FROM coupon_batches WHERE id=?", bid)
            .await,
        2_000_000 - est,
        "only the unused part returned"
    );
    e.invariant(bid).await;
    // nothing identifies the person any more
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT request_id FROM coupon_ledger WHERE request_id IS NOT NULL AND ukey LIKE 'hold:%'",
    )
    .fetch_all(&e.st.db)
    .await
    .unwrap();
    assert!(rows.iter().all(|r| !r.contains(&uid)));
    // the issuer's report keeps the money truthful: held, delivery unknown, not spent
    let report = format!("/v1/me/apps/{}/coupons/{bid}/report", e.app_id);
    let (_, rep) = e.get(&e.owner, &report).await;
    assert_eq!(
        (rep["money"]["uncertain_held_usd"].as_f64().unwrap() * 1e6).round() as i64,
        est,
        "{rep}"
    );
    assert_eq!(rep["money"]["spent_usd"], 0.0);
    assert_eq!(
        rep["coupons"]["claimed"], 1,
        "claims are counted from the ledger"
    );
    // a late result still reconciles by request id although the request row is long gone; twice is a no-op
    assert!(reconcile_hold(&e.st, &unknown, true, Some(est / 2))
        .await
        .unwrap());
    assert!(!reconcile_hold(&e.st, &unknown, true, Some(est / 2))
        .await
        .unwrap());
    assert_eq!(
        e.q::<i64>("SELECT spent_micro FROM coupon_batches WHERE id=?", bid)
            .await,
        est / 2
    );
    assert_eq!(
        e.q::<i64>("SELECT available_micro FROM coupon_batches WHERE id=?", bid)
            .await,
        2_000_000 - est / 2
    );
    assert_eq!(
        e.q::<i64>("SELECT COUNT(*) FROM coupons", "").await,
        0,
        "nothing left to remember"
    );
    e.invariant(bid).await;
    let (_, rep) = e.get(&e.owner, &report).await;
    assert_eq!(rep["money"]["uncertain_held_usd"], 0.0);
    assert_eq!(rep["first_successful_users"], 1);
    // a deleted account can claim nothing again
    assert_eq!(e.get(&u, "/v1/me/coupons").await.0, 401);
}

#[tokio::test]
async fn deleting_the_issuer_or_the_app_ends_its_batches_cleanly() {
    let (e, b, u, tok, cid, _g) = funded(1.0, 2.0).await;
    let bid = b["id"].as_str().unwrap();
    let (_, _, rid) = e.chat(&tok, "waiting", None).await;
    // app deletion: batch revoked, key wiped, waiting call denied, unused balance back
    let (s, v) = e
        .delete(&e.owner, &format!("/v1/me/apps/{}", e.app_id))
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(
        e.get(&u, &format!("/v1/me/requests/{rid}")).await.1["status"],
        "denied"
    );
    assert_eq!(
        e.q::<i64>(
            "SELECT revoked_at IS NOT NULL AND key_ct IS NULL FROM coupon_batches WHERE id=?",
            bid
        )
        .await,
        1
    );
    assert_eq!(
        e.q::<i64>("SELECT available_micro FROM coupon_batches WHERE id=?", bid)
            .await,
        2_000_000
    );
    e.invariant(bid).await;
    let _ = cid;
    // issuer account deletion with a second live batch: revoked, detached from the account, no identity kept
    let e2 = env().await;
    let b2 = e2.create_batch(1.0, 2.0).await;
    let bid2 = b2["id"].as_str().unwrap();
    let r = e2
        .post(
            &e2.owner,
            "/v1/me/delete",
            json!({"confirm": "DELETE", "email": "owner@x.co"}),
        )
        .await;
    assert_eq!(r.0, 200, "{}", r.1);
    assert_eq!(e2.q::<i64>("SELECT revoked_at IS NOT NULL AND key_ct IS NULL AND owner_user_id IS NULL FROM coupon_batches WHERE id=?", bid2).await, 1);
    let code = b2["coupon_code"].as_str().unwrap();
    let u2 = sess(&e2.c, &e2.base, "x@x.co").await;
    assert_eq!(
        e2.claim(&u2, code).await.1["error"]["type"],
        "coupon_revoked"
    );
    e2.invariant(bid2).await;
}

#[tokio::test]
async fn report_has_aggregates_only() {
    let (e, b, u, tok, _cid, _g) = funded(1.0, 2.0).await;
    let bid = b["id"].as_str().unwrap();
    let (_, _, rid) = e.chat(&tok, "SECRETPROMPT-12345", None).await;
    e.post(&u, &format!("/v1/me/requests/{rid}/approve"), json!({}))
        .await;
    e.wait(&u, &rid, "completed").await;
    let (s, rep) = e
        .get(
            &e.owner,
            &format!("/v1/me/apps/{}/coupons/{bid}/report", e.app_id),
        )
        .await;
    assert_eq!(s, 200, "{rep}");
    assert_eq!(rep["coupons"]["claimed"], 1);
    assert_eq!(rep["requests"]["completed"], 1);
    assert!(rep["money"]["spent_usd"].as_f64().unwrap() > 0.0);
    // the batch list carries the same claim count and spend, so the console needs no report per row
    let (_, list) = e
        .get(&e.owner, &format!("/v1/me/apps/{}/coupons", e.app_id))
        .await;
    assert_eq!(list[0]["claimed"], 1);
    assert_eq!(list[0]["spent_usd"], rep["money"]["spent_usd"]);
    let text = rep.to_string();
    let uid: String = e.q("SELECT id FROM users WHERE email=?", "r@x.co").await;
    for leak in [
        "SECRETPROMPT",
        "r@x.co",
        uid.as_str(),
        "BATCHKEY",
        "sk-test",
        "Hello from the Keyala mock vendor",
        "owner@x.co",
    ] {
        assert!(!text.contains(leak), "report leaks {leak}");
    }
    // only the owner of the app can read it
    assert_eq!(
        e.get(
            &u,
            &format!("/v1/me/apps/{}/coupons/{bid}/report", e.app_id)
        )
        .await
        .0,
        404
    );
}

#[tokio::test]
async fn email_transitions_retry_with_backoff_and_dedupe_per_episode() {
    let e = env().await;
    let b = e.create_batch(0.1, 0.25).await;
    let (bid, code) = (
        b["id"].as_str().unwrap(),
        b["coupon_code"].as_str().unwrap(),
    );
    let outbox = || async {
        sqlx::query_as::<_, (String, String, i64, Option<String>, Option<String>)>(
            "SELECT kind, status, attempts, provider, last_error FROM coupon_outbox WHERE batch_id=? ORDER BY created_at",
        )
        .bind(bid)
        .fetch_all(&e.st.db)
        .await
        .unwrap()
    };
    let a = sess(&e.c, &e.base, "a@x.co").await;
    let bb = sess(&e.c, &e.base, "b@x.co").await;
    let c = sess(&e.c, &e.base, "c@x.co").await;
    assert_eq!(e.claim(&a, code).await.0, 200);
    assert!(
        outbox().await.is_empty(),
        "0.15 still fits another coupon: no event"
    );
    // the first delivery attempt (started right after the claim, off the request path) fails
    e.st.mail_mock.lock().unwrap().outcomes.push_back(500);
    let ca = e.claim(&bb, code).await.1["id"]
        .as_str()
        .unwrap()
        .to_string(); // 0.05 left: below one full coupon
    let rows = outbox().await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "capacity");
    // sold out claims neither add events nor change money
    assert_eq!(e.claim(&c, code).await.0, 409);
    assert_eq!(e.claim(&c, code).await.0, 409);
    assert_eq!(outbox().await.len(), 1);
    // delivery: the first attempt failed (mock) without blocking the claim, is retried after a backoff, then succeeds
    for _ in 0..100 {
        if outbox().await[0].1 == "pending" && outbox().await[0].2 == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let r = &outbox().await[0];
    assert_eq!((r.1.as_str(), r.2), ("pending", 1), "{r:?}");
    assert!(r.4.is_some());
    // the message is bound now: sender, the issuer's address, fixed text; the outbox id is the Idempotency-Key
    let (oid, to, from): (String, String, String) =
        sqlx::query_as("SELECT id, recipient, from_addr FROM coupon_outbox WHERE batch_id=?")
            .bind(bid)
            .fetch_one(&e.st.db)
            .await
            .unwrap();
    assert_eq!(to, "owner@x.co");
    process_outbox(&e.st).await; // backoff not elapsed: no new attempt
    assert_eq!(outbox().await[0].2, 1);
    sqlx::query("UPDATE coupon_outbox SET next_attempt_at='2000-01-01T00:00:00.000Z'")
        .execute(&e.st.db)
        .await
        .unwrap();
    process_outbox(&e.st).await;
    let r = &outbox().await[0];
    assert_eq!(
        (r.1.as_str(), r.2, r.3.as_deref()),
        ("sent", 2, Some("mock")),
        "{r:?}"
    );
    process_outbox(&e.st).await;
    assert_eq!(outbox().await[0].2, 2, "a sent event is never sent again");
    {
        let m = e.st.mail_mock.lock().unwrap();
        assert_eq!(m.sent.len(), 2, "one failed attempt + one success");
        // same key and byte-identical message on every attempt
        assert!(m.sent.iter().all(|x| x.idempotency_key == oid));
        assert_eq!(m.sent[0].body, m.sent[1].body);
        let body = &m.sent[1].body;
        assert_eq!(body["to"], json!(["owner@x.co"]));
        assert_eq!(body["from"], json!(from));
        assert!(body["text"].as_str().unwrap().contains(code));
        assert_eq!(
            body.as_object().unwrap().len(),
            4,
            "from,to,subject,text only"
        );
    }
    // a finished row keeps no address or text
    let kept: i64 = e
        .q("SELECT recipient IS NOT NULL OR body IS NOT NULL OR from_addr IS NOT NULL FROM coupon_outbox WHERE id=?", &oid)
        .await;
    assert_eq!(kept, 0);
    // recovery clears the episode; falling again is a new episode and a new event, once
    e.expire_coupon(&ca).await;
    sweep_coupons(&e.st).await.unwrap();
    assert_eq!(outbox().await.len(), 1);
    e.st.mail_mock.lock().unwrap().outcomes.extend([503; 100]); // every delivery from here on fails
    assert_eq!(e.claim(&c, code).await.0, 200); // 0.15 -> 0.05
    let rows = outbox().await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(rows.iter().all(|r| r.0 == "capacity"));
    // bounded retries: after five failed attempts it stays failed
    for _ in 0..8 {
        for _ in 0..100 {
            let busy: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM coupon_outbox WHERE status='sending'")
                    .fetch_one(&e.st.db)
                    .await
                    .unwrap();
            if busy == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        sqlx::query("UPDATE coupon_outbox SET next_attempt_at='2000-01-01T00:00:00.000Z' WHERE status='pending'").execute(&e.st.db).await.unwrap();
        process_outbox(&e.st).await;
    }
    let rows = outbox().await;
    assert_eq!((rows[1].1.as_str(), rows[1].2), ("failed", 5), "{rows:?}");
    assert_eq!(rows[0].1, "sent");
    e.invariant(bid).await;
}

#[tokio::test]
async fn email_is_off_without_a_transport_and_never_goes_to_synthetic_addresses() {
    use sha2::{Digest, Sha256};
    // no transport configured: batches say so and events are recorded as skipped, never sent
    let e = env_with(cfg(false)).await;
    let b = e.create_batch(0.1, 0.1).await;
    assert_eq!(b["email_enabled"], false);
    let u = sess(&e.c, &e.base, "a@x.co").await;
    assert_eq!(e.claim(&u, b["coupon_code"].as_str().unwrap()).await.0, 200);
    let (mut st, mut why): (String, Option<String>) = (String::new(), None);
    for _ in 0..100 {
        process_outbox(&e.st).await;
        (st, why) = sqlx::query_as("SELECT status, last_error FROM coupon_outbox")
            .fetch_one(&e.st.db)
            .await
            .unwrap();
        if st != "pending" && st != "sending" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        (st.as_str(), why.as_deref()),
        ("skipped", Some("email_disabled"))
    );
    let s: Value =
        e.c.get(format!("{}/v1/status", e.base))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    assert_eq!(s["coupon_email"], "off");
    // a real transport is configured but the issuer is the operator password account (…@password.invalid): refused
    let (mut c, dir) = cfg(true);
    c.password_login = Some(("qa".into(), Sha256::digest("pw-long-random-1").into()));
    let st = build_state(c).await.unwrap();
    let app = router(st.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(l, app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .unwrap()
    });
    let cl = Client::new();
    let tok = jv(cl
        .post(format!("{base}/v1/auth/password"))
        .json(&json!({"username": "qa", "password": "pw-long-random-1"}))
        .send()
        .await
        .unwrap())
    .await
    .1["session_token"]
        .as_str()
        .unwrap()
        .to_string();
    let a: Value = cl
        .post(format!("{base}/v1/me/apps"))
        .bearer_auth(&tok)
        .json(&json!({"name": "P"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let app_id = a["app_id"].as_str().unwrap();
    let (s, b) = jv(cl.post(format!("{base}/v1/me/apps/{app_id}/coupons")).bearer_auth(&tok)
        .json(&json!({"vendor": "openai", "models": ["gpt-4o-mini"], "api_key": "sk-test-BATCHKEY9999", "per_coupon_usd": 0.1, "total_usd": 0.1, "claim_deadline": deadline()})).send().await.unwrap()).await;
    assert_eq!(s, 200);
    assert_eq!(
        b["email_enabled"], false,
        "synthetic address is never a recipient"
    );
    let _ = dir;
}

async fn subjects(e: &E) -> Vec<String> {
    process_outbox(&e.st).await;
    flush(e).await; // a background worker may hold a row: wait for it before reading what was sent
    e.st.mail_mock
        .lock()
        .unwrap()
        .sent
        .iter()
        .map(|m| m.body["subject"].as_str().unwrap().to_string())
        .collect()
}
async fn flush(e: &E) {
    // deliveries started by claims/settlements run in the background: let them finish first
    for _ in 0..100 {
        let busy: i64 = e
            .q(
                "SELECT COUNT(*) FROM coupon_outbox WHERE status='sending'",
                "",
            )
            .await;
        if busy == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn claims_blocked_is_not_fully_spent_and_held_money_is_not_spent() {
    // 1.10 total, 1.00 per coupon: after one claim only 0.10 is free (no full claim slot), but nothing is spent
    let (e, b, u, tok, _cid, _g) = funded(1.0, 1.1).await;
    let bid = b["id"].as_str().unwrap();
    let (_, _, rid) = e.chat(&tok, "KEYALA_MOCK_NOUSAGE", None).await;
    e.pause();
    e.post(&u, &format!("/v1/me/requests/{rid}/approve"), json!({}))
        .await;
    flush(&e).await;
    let kinds = |e: &E| {
        let db = e.st.db.clone();
        let bid = bid.to_string();
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT kind FROM coupon_outbox WHERE batch_id=? ORDER BY kind",
            )
            .bind(bid)
            .fetch_all(&db)
            .await
            .unwrap()
        }
    };
    assert_eq!(
        kinds(&e).await,
        ["capacity"],
        "held money does not exhaust the batch"
    );
    std::fs::remove_file(e.dir.join("PAUSE_UPSTREAM")).unwrap();
    keyala_server::send_request(e.st.clone(), rid.clone()).await;
    e.wait(&u, &rid, "completed").await;
    flush(&e).await;
    assert_eq!(
        kinds(&e).await,
        ["capacity"],
        "0.10 is still unspent: capacity only, never fully spent"
    );
    let mails = subjects(&e).await;
    assert_eq!(mails.len(), 1);
    assert!(mails[0].contains("not enough budget"), "{mails:?}");
    e.invariant(bid).await;
}

#[tokio::test]
async fn fully_spent_is_announced_once_and_only_when_spend_reaches_the_total() {
    let (e, b, u, tok, cid, _g) = funded(1.0, 1.0).await;
    let bid = b["id"].as_str().unwrap();
    let (_, _, rid) = e.chat(&tok, "KEYALA_MOCK_NOUSAGE", None).await;
    let (_, r) = e.get(&u, &format!("/v1/me/requests/{rid}")).await;
    let est = (r["estimated_usd"].as_f64().unwrap() * 1e6).round() as i64;
    // leave exactly one call's worth in the coupon; everything else counts as already spent
    sqlx::query("UPDATE coupons SET remaining_micro=? WHERE id=?")
        .bind(est)
        .bind(&cid)
        .execute(&e.st.db)
        .await
        .unwrap();
    sqlx::query("UPDATE coupon_batches SET spent_micro=total_micro-? WHERE id=?")
        .bind(est)
        .bind(bid)
        .execute(&e.st.db)
        .await
        .unwrap();
    e.invariant(bid).await;
    e.pause();
    e.post(&u, &format!("/v1/me/requests/{rid}/approve"), json!({}))
        .await;
    let n: i64 = e
        .q(
            "SELECT COUNT(*) FROM coupon_outbox WHERE kind='exhausted'",
            "",
        )
        .await;
    assert_eq!(n, 0, "the last call is only held: not fully spent yet");
    std::fs::remove_file(e.dir.join("PAUSE_UPSTREAM")).unwrap();
    keyala_server::send_request(e.st.clone(), rid.clone()).await;
    e.wait(&u, &rid, "completed").await;
    flush(&e).await;
    let n: i64 = e
        .q(
            "SELECT COUNT(*) FROM coupon_outbox WHERE kind='exhausted'",
            "",
        )
        .await;
    assert_eq!(n, 1);
    let mails = subjects(&e).await;
    assert!(mails.iter().any(|m| m.contains("fully spent")), "{mails:?}");
    assert_eq!(
        e.q::<i64>(
            "SELECT spent_micro = total_micro FROM coupon_batches WHERE id=?",
            bid
        )
        .await,
        1
    );
    e.invariant(bid).await;
}

#[tokio::test]
async fn restart_resends_only_inside_the_attempt_budget_and_the_24h_window() {
    let (c1, dir) = cfg(true);
    let url = c1.database_url.clone();
    let e = env_with((c1, dir)).await;
    let bid: String = {
        let b = e.create_batch(0.5, 1.0).await;
        b["id"].as_str().unwrap().to_string()
    };
    let put = |id: &'static str, attempts: i64, first: String| {
        let (db, bid) = (e.st.db.clone(), bid.clone());
        async move {
            // Keep delivery in the future: this test observes restart recovery, not a race with its worker.
            let next = (chrono::Utc::now() + chrono::Duration::hours(1))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            sqlx::query("INSERT INTO coupon_outbox (id,batch_id,kind,dedup_key,status,attempts,first_attempt_at,next_attempt_at,created_at,recipient,from_addr,subject,body,provider) VALUES (?1,?2,'capacity',?1,'sending',?3,?4,?5,'t','owner@x.co','f@x.co','s','b','mock')")
                .bind(id).bind(bid).bind(attempts).bind(first).bind(next).execute(&db).await.unwrap();
        }
    };
    let ago = |h: i64| {
        (chrono::Utc::now() - chrono::Duration::hours(h))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    };
    put("fresh", 2, ago(1)).await;
    put("old", 2, ago(25)).await;
    put("spent", 5, ago(1)).await;
    // the process restarts on the same database
    let (mut c2, _) = cfg(true);
    c2.database_url = url;
    let st2 = build_state(c2).await.unwrap();
    let status = |id: &'static str| {
        let db = st2.db.clone();
        async move {
            sqlx::query_as::<_, (String, Option<String>)>(
                "SELECT status, recipient FROM coupon_outbox WHERE id=?",
            )
            .bind(id)
            .fetch_one(&db)
            .await
            .unwrap()
        }
    };
    assert_eq!(
        status("fresh").await,
        ("pending".into(), Some("owner@x.co".into())),
        "same verified recipient and message recovered for its scheduled retry"
    );
    assert_eq!(
        status("old").await,
        ("failed".into(), None),
        "past the provider's de-duplication window: never again"
    );
    assert_eq!(
        status("spent").await,
        ("failed".into(), None),
        "attempt budget used up"
    );
}

#[tokio::test]
async fn only_a_verified_identity_address_can_receive_mail() {
    // real-looking transport configured, but the issuer is a dev-login account: no verified address, no mail, no call out
    let (mut c, dir) = cfg(false);
    c.resend_api_key = Some("re_test_not_real".into());
    c.email_from = Some("Keyala <noreply@mail.test>".into());
    let e = env_with((c, dir)).await;
    let b = e.create_batch(0.1, 0.1).await;
    assert_eq!(
        b["email_enabled"], false,
        "dev accounts only count under the mock transport"
    );
    let u = sess(&e.c, &e.base, "a@x.co").await;
    assert_eq!(e.claim(&u, b["coupon_code"].as_str().unwrap()).await.0, 200);
    for _ in 0..50 {
        process_outbox(&e.st).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let (st, why): (String, Option<String>) =
        sqlx::query_as("SELECT status, last_error FROM coupon_outbox")
            .fetch_one(&e.st.db)
            .await
            .unwrap();
    assert_eq!(
        (st.as_str(), why.as_deref()),
        ("skipped", Some("no_verified_recipient"))
    );

    // mock transport: an Apple/Google account is only mailable with a verified-token address (NULL = historical, fail closed)
    let e = env().await;
    let list = format!("/v1/me/apps/{}/coupons", e.app_id);
    let enabled = |e: &E| {
        let (c, base, tok, list) = (e.c.clone(), e.base.clone(), e.owner.clone(), list.clone());
        async move {
            let v: Value = c
                .get(format!("{base}{list}"))
                .bearer_auth(tok)
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            v[0]["email_enabled"].clone()
        }
    };
    e.create_batch(0.1, 0.1).await;
    assert_eq!(enabled(&e).await, true, "mock: dev account");
    sqlx::query("UPDATE users SET apple_sub='001.x' WHERE email='owner@x.co'")
        .execute(&e.st.db)
        .await
        .unwrap();
    assert_eq!(
        enabled(&e).await,
        false,
        "an Apple account that never signed in with a verified address"
    );
    sqlx::query("UPDATE users SET email='owner@x.co', notify_email='real@icloud.com' WHERE email='owner@x.co'").execute(&e.st.db).await.unwrap();
    assert_eq!(enabled(&e).await, true);
    sqlx::query("UPDATE users SET notify_email='bad@placeholder.invalid' WHERE email='owner@x.co'")
        .execute(&e.st.db)
        .await
        .unwrap();
    assert_eq!(enabled(&e).await, false);
    sqlx::query("UPDATE users SET notify_email='a@b.co, c@d.co' WHERE email='owner@x.co'")
        .execute(&e.st.db)
        .await
        .unwrap();
    assert_eq!(
        enabled(&e).await,
        false,
        "never more than one plain address"
    );
}

#[tokio::test]
async fn deleting_the_issuer_erases_outbox_addresses_and_skips_unsent_mail() {
    let e = env().await;
    let b = e.create_batch(0.1, 0.25).await;
    let bid = b["id"].as_str().unwrap();
    e.st.mail_mock.lock().unwrap().outcomes.extend([500; 3]);
    let a = sess(&e.c, &e.base, "a@x.co").await;
    let bb = sess(&e.c, &e.base, "b@x.co").await;
    e.claim(&a, b["coupon_code"].as_str().unwrap()).await;
    e.claim(&bb, b["coupon_code"].as_str().unwrap()).await;
    for _ in 0..200 {
        let n: i64 = e
            .q(
                "SELECT COUNT(*) FROM coupon_outbox WHERE attempts>=1 AND status='pending'",
                "",
            )
            .await;
        if n == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let (st, to): (String, Option<String>) =
        sqlx::query_as("SELECT status, recipient FROM coupon_outbox WHERE batch_id=?")
            .bind(bid)
            .fetch_one(&e.st.db)
            .await
            .unwrap();
    assert_eq!(
        (st.as_str(), to.as_deref()),
        ("pending", Some("owner@x.co")),
        "waiting for its retry"
    );
    let r = e
        .post(
            &e.owner,
            "/v1/me/delete",
            json!({"confirm": "DELETE", "email": "owner@x.co"}),
        )
        .await;
    assert_eq!(r.0, 200, "{}", r.1);
    let (st, to, body, why): (String, Option<String>, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT status, recipient, body, last_error FROM coupon_outbox WHERE batch_id=?",
        )
        .bind(bid)
        .fetch_one(&e.st.db)
        .await
        .unwrap();
    assert_eq!(
        (st.as_str(), to, body, why.as_deref()),
        ("skipped", None, None, Some("issuer_deleted"))
    );
    let before = e.st.mail_mock.lock().unwrap().sent.len();
    sqlx::query("UPDATE coupon_outbox SET next_attempt_at='2000-01-01T00:00:00.000Z'")
        .execute(&e.st.db)
        .await
        .unwrap();
    process_outbox(&e.st).await;
    assert_eq!(
        e.st.mail_mock.lock().unwrap().sent.len(),
        before,
        "nothing is sent after the issuer is gone"
    );
    // no mail address anywhere in the coupon tables
    let all: String = sqlx::query_scalar(
        "SELECT COALESCE(group_concat(recipient||from_addr||subject||body), '') FROM coupon_outbox",
    )
    .fetch_one(&e.st.db)
    .await
    .unwrap();
    assert!(!all.contains("owner@x.co"));
}

#[tokio::test]
async fn report_separates_actual_estimated_and_uncertain_in_one_snapshot() {
    let (e, b, u, tok, cid, _g) = funded(1.0, 3.0).await;
    let bid = b["id"].as_str().unwrap();
    let report = format!("/v1/me/apps/{}/coupons/{bid}/report", e.app_id);
    // one call with provider usage, one without, one failed, one left uncertain
    let (_, _, r1) = e.chat(&tok, "with usage", None).await;
    e.post(&u, &format!("/v1/me/requests/{r1}/approve"), json!({}))
        .await;
    let done = e.wait(&u, &r1, "completed").await;
    let actual = (done["actual_usd"].as_f64().unwrap() * 1e6).round() as i64;
    let (_, _, r2) = e.chat(&tok, "KEYALA_MOCK_NOUSAGE", None).await;
    let (_, a2) = e
        .post(&u, &format!("/v1/me/requests/{r2}/approve"), json!({}))
        .await;
    let est2 = (a2["estimated_usd"].as_f64().unwrap() * 1e6).round() as i64;
    e.wait(&u, &r2, "completed").await;
    let (_, _, r3) = e.chat(&tok, "KEYALA_MOCK_FAIL", None).await;
    e.post(&u, &format!("/v1/me/requests/{r3}/approve"), json!({}))
        .await;
    e.wait(&u, &r3, "failed").await;
    let (_, _, r4) = e.chat(&tok, "unknown", None).await;
    let (_, _, _r5) = e.chat(&tok, "still waiting", None).await;
    e.pause();
    let (_, a4) = e
        .post(&u, &format!("/v1/me/requests/{r4}/approve"), json!({}))
        .await;
    let est4 = (a4["estimated_usd"].as_f64().unwrap() * 1e6).round() as i64;
    sqlx::query("UPDATE requests SET status='uncertain' WHERE id=?")
        .bind(&r4)
        .execute(&e.st.db)
        .await
        .unwrap();
    let (s, rep) = e.get(&e.owner, &report).await;
    assert_eq!(s, 200, "{rep}");
    let micro = |v: &Value| (v.as_f64().unwrap() * 1e6).round() as i64;
    let m = &rep["money"];
    assert_eq!(micro(&m["actual_cost_usd"]), actual, "{rep}");
    assert_eq!(micro(&m["completed_estimated_cost_usd"]), est2);
    assert_eq!(micro(&m["uncertain_held_usd"]), est4);
    assert_eq!(
        micro(&m["spent_usd"]),
        actual + est2,
        "spent = actual + estimate-charged; uncertain is not in it"
    );
    assert_eq!(micro(&m["held_usd"]), est4);
    assert_eq!(micro(&m["in_flight_held_usd"]), 0);
    // snapshot invariant straight from the report: total = free + unspent + held + spent
    assert_eq!(
        micro(&m["total_usd"]),
        micro(&m["available_usd"])
            + micro(&m["unspent_in_coupons_usd"])
            + micro(&m["held_usd"])
            + micro(&m["spent_usd"])
    );
    assert_eq!(rep["first_successful_users"], 1);
    assert_eq!(rep["coupons"]["claimed"], 1);
    for (k, n) in [
        ("completed", 2),
        ("failed", 1),
        ("uncertain", 1),
        ("pending", 1),
        ("denied", 0),
    ] {
        assert_eq!(rep["requests"][k], n, "{k}: {rep}");
    }
    assert!(rep["input_tokens"].as_i64().unwrap() > 0);
    let models = rep["models"].as_array().unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0]["model"], "gpt-4o-mini");
    assert_eq!(models[0]["requests"], 2);
    assert_eq!(micro(&models[0]["actual_cost_usd"]), actual);
    assert_eq!(micro(&models[0]["completed_estimated_cost_usd"]), est2);
    // the coupon expires: the unused part returns, and a late result's unused refund is "returned" too
    e.expire_coupon(&cid).await;
    sweep_coupons(&e.st).await.unwrap();
    let (_, rep) = e.get(&e.owner, &report).await;
    let returned1 = micro(&rep["money"]["returned_usd"]);
    assert!(returned1 > 0);
    assert!(reconcile_hold(&e.st, &r4, true, Some(est4 / 2))
        .await
        .unwrap());
    let (_, rep) = e.get(&e.owner, &report).await;
    assert_eq!(
        micro(&rep["money"]["returned_usd"]),
        returned1 + (est4 - est4 / 2),
        "{rep}"
    );
    assert_eq!(micro(&rep["money"]["actual_cost_usd"]), actual + est4 / 2);
    assert_eq!(micro(&rep["money"]["uncertain_held_usd"]), 0);
    assert_eq!(
        rep["first_successful_users"], 1,
        "one person, however many calls"
    );
    e.invariant(bid).await;
}

// ---------- docs/ops/replay-deletions.sh, the real script against real backups ----------

fn script() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/ops/replay-deletions.sh")
}
fn sqlite3_ok() -> bool {
    std::process::Command::new("sqlite3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
fn replay(db: &std::path::Path, log: &std::path::Path) -> String {
    let o = std::process::Command::new("bash")
        .arg(script())
        .arg(db)
        .arg(log)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "replay failed: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8_lossy(&o.stdout).to_string()
}

/// Everything about a batch's money and its people that a deletion changes, as comparable values.
async fn coupon_state(db: &sqlx::SqlitePool) -> Value {
    let batches: Vec<(i64, i64, bool, bool, bool)> = sqlx::query_as("SELECT available_micro, spent_micro, revoked_at IS NOT NULL, key_ct IS NULL, owner_user_id IS NULL FROM coupon_batches ORDER BY id").fetch_all(db).await.unwrap();
    let coupons: Vec<(i64, i64, i64, bool, bool)> = sqlx::query_as("SELECT remaining_micro, held_micro, spent_micro, user_id IS NULL, closed_at IS NOT NULL FROM coupons ORDER BY id").fetch_all(db).await.unwrap();
    let ukeys: Vec<String> = sqlx::query_scalar("SELECT ukey FROM coupon_ledger ORDER BY ukey")
        .fetch_all(db)
        .await
        .unwrap();
    // mail: nothing may still be waiting and no address or text may be kept. (Live deletions may enqueue and then skip
    // a notification the backup never saw; the replay sends no mail, so rows themselves are not compared.)
    let outbox: (i64, i64) = sqlx::query_as("SELECT COALESCE(SUM(recipient IS NOT NULL OR body IS NOT NULL OR from_addr IS NOT NULL OR subject IS NOT NULL),0), COALESCE(SUM(status IN ('pending','sending')),0) FROM coupon_outbox").fetch_one(db).await.unwrap();
    let reqs: Vec<(String,)> = sqlx::query_as("SELECT status FROM requests ORDER BY id")
        .fetch_all(db)
        .await
        .unwrap();
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(db)
        .await
        .unwrap();
    json!({"batches": batches, "coupons": coupons, "ukeys": ukeys, "outbox": outbox, "requests": reqs, "users": users})
}

#[tokio::test]
async fn replay_script_repeats_recipient_and_issuer_deletion_with_held_money_after_restore() {
    assert!(
        sqlite3_ok(),
        "the sqlite3 CLI is required for the restore regression test"
    );
    let (e, b, u, tok, cid, _g) = funded(1.0, 2.0).await;
    let bid = b["id"].as_str().unwrap();
    // a second recipient with a call approved but not yet sent (denied when the issuer goes)
    let u2 = sess(&e.c, &e.base, "r2@x.co").await;
    assert_eq!(
        e.claim(&u2, b["coupon_code"].as_str().unwrap()).await.0,
        200
    );
    let (_g2, tok2) = e.connect(&u2, b["coupon_code"].as_str().unwrap()).await;
    // recipient 1: a call whose outcome is unknown (held money)
    let (_, _, unknown) = e.chat(&tok, "outcome unknown", None).await;
    let (_, _, unsent) = e.chat(&tok2, "approved, never sent", None).await;
    e.pause();
    let (_, a) = e
        .post(&u, &format!("/v1/me/requests/{unknown}/approve"), json!({}))
        .await;
    let est = (a["estimated_usd"].as_f64().unwrap() * 1e6).round() as i64;
    e.post(&u2, &format!("/v1/me/requests/{unsent}/approve"), json!({}))
        .await;
    sqlx::query("UPDATE requests SET status='uncertain' WHERE id=?")
        .bind(&unknown)
        .execute(&e.st.db)
        .await
        .unwrap();
    // a mail waiting for its retry holds the issuer's address (as the outbox does between attempts)
    sqlx::query("INSERT INTO coupon_outbox (id,batch_id,kind,dedup_key,status,attempts,first_attempt_at,next_attempt_at,created_at,recipient,from_addr,subject,body) VALUES ('out_bound',?1,'capacity','capacity:x:1','pending',1,?2,'2099-01-01T00:00:00.000Z',?2,'owner@x.co','f@x.co','s','text')")
        .bind(bid).bind(now_s()).execute(&e.st.db).await.unwrap();
    flush(&e).await;
    // BACKUP (before any deletion)
    let backup = e.dir.join("backup.db");
    sqlx::query(&format!("VACUUM INTO '{}'", backup.display()))
        .execute(&e.st.db)
        .await
        .unwrap();
    // live: both accounts are deleted (recipient first, then the issuer)
    let (s1, v) = e
        .post(
            &u,
            "/v1/me/delete",
            json!({"confirm": "DELETE", "email": "r@x.co"}),
        )
        .await;
    assert_eq!(s1, 200, "{v}");
    let (s2, v) = e
        .post(
            &e.owner,
            "/v1/me/delete",
            json!({"confirm": "DELETE", "email": "owner@x.co"}),
        )
        .await;
    assert_eq!(s2, 200, "{v}");
    e.invariant(bid).await;
    let live = coupon_state(&e.st.db).await;
    // the held money is still there on the live side
    assert_eq!(
        e.q::<i64>("SELECT held_micro FROM coupons WHERE id=?", &cid)
            .await,
        est
    );
    let log = e.dir.join("deleted-accounts.log");
    assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 2);

    // RESTORE the backup elsewhere and replay with the real script (twice: it is safe to repeat)
    let restored = e.dir.join("restored.db");
    std::fs::copy(&backup, &restored).unwrap();
    assert!(replay(&restored, &log).contains("replayed 2 deletion(s)"));
    replay(&restored, &log);
    let rdb = sqlx::SqlitePool::connect(&format!("sqlite://{}", restored.display()))
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&rdb)
        .await
        .unwrap();
    let fk: Vec<(String,)> = sqlx::query_as("SELECT \"table\" FROM pragma_foreign_key_check")
        .fetch_all(&rdb)
        .await
        .unwrap();
    assert!(fk.is_empty(), "foreign key violations after replay: {fk:?}");
    let replayed = coupon_state(&rdb).await;
    assert_eq!(
        replayed, live,
        "the replay must end in the same state as the live deletions"
    );
    // and the facts that matter, stated outright
    let (held, spent, rem, uid_null): (i64, i64, i64, bool) = sqlx::query_as(
        "SELECT held_micro, spent_micro, remaining_micro, user_id IS NULL FROM coupons WHERE id=?",
    )
    .bind(&cid)
    .fetch_one(&rdb)
    .await
    .unwrap();
    assert_eq!(
        (held, spent, rem, uid_null),
        (est, 0, 0, true),
        "unknown money stays held, anonymously, never spent"
    );
    let (rev, wiped, ownerless): (bool, bool, bool) = sqlx::query_as("SELECT revoked_at IS NOT NULL, key_ct IS NULL AND key_nonce IS NULL, owner_user_id IS NULL FROM coupon_batches WHERE id=?").bind(bid).fetch_one(&rdb).await.unwrap();
    assert!(
        (rev, wiped, ownerless) == (true, true, true),
        "sponsor revoked, key wiped"
    );
    let st: String = sqlx::query_scalar("SELECT status FROM requests WHERE id=?")
        .bind(&unsent)
        .fetch_one(&rdb)
        .await
        .unwrap();
    assert_eq!(st, "denied", "the other recipient's unsent call was denied");
    let users: Vec<String> = sqlx::query_scalar("SELECT email FROM users")
        .fetch_all(&rdb)
        .await
        .unwrap();
    assert!(
        users.iter().all(|m| m != "r@x.co" && m != "owner@x.co"),
        "{users:?}"
    );
    // the hold row that makes reconciliation by request id possible survived the replay
    let hold: i64 = sqlx::query_scalar("SELECT micro FROM coupon_ledger WHERE ukey=?")
        .bind(format!("hold:{unknown}"))
        .fetch_one(&rdb)
        .await
        .unwrap();
    assert_eq!(hold, est);
    // reconcile on the live side works the same way
    assert!(reconcile_hold(&e.st, &unknown, false, None).await.unwrap());
    e.invariant(bid).await;
}

fn now_s() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[tokio::test]
async fn replay_script_handles_a_backup_from_before_the_coupon_migration() {
    assert!(
        sqlite3_ok(),
        "the sqlite3 CLI is required for the restore regression test"
    );
    let dir = std::env::temp_dir().join(format!("mk-replay-{}", keyala_server::rand_b64(8)));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("old.db");
    // a database exactly as migrations 0001..0017 leave it (the state before coupons existed)
    let mut sql = String::from("PRAGMA foreign_keys=ON;\n");
    let mut files: Vec<_> =
        std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations"))
            .unwrap()
            .map(|f| f.unwrap().path())
            .collect();
    files.sort();
    for f in files
        .iter()
        .filter(|f| !f.file_name().unwrap().to_str().unwrap().starts_with("0018"))
    {
        sql.push_str(&std::fs::read_to_string(f).unwrap());
        sql.push('\n');
    }
    sql.push_str("INSERT INTO users (id,email,created_at) VALUES ('usr_gone','gone@x.co','t'),('usr_stay','stay@x.co','t');
        INSERT INTO sessions (token_hash,user_id,created_at) VALUES ('h1','usr_gone','t'),('h2','usr_stay','t');
        INSERT INTO api_keys (user_id,vendor,ciphertext,nonce,last4,created_at) VALUES ('usr_gone','openai',x'01',x'02','1234','t');
        INSERT INTO apps (id,name,secret_hash,created_at,owner_user_id,plaza_status,plaza_at) VALUES ('app_1','A','h','t','usr_gone','approved','t');
        INSERT INTO grants (id,user_id,app_id,vendor,mode,created_at) VALUES ('grt_1','usr_gone','app_1','openai','ask','t');\n");
    let mut p = std::process::Command::new("sqlite3")
        .arg(&db)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::io::Write::write_all(&mut p.stdin.take().unwrap(), sql.as_bytes()).unwrap();
    assert!(p.wait().unwrap().success());
    let tables = std::process::Command::new("sqlite3")
        .arg(&db)
        .arg("SELECT COUNT(*) FROM sqlite_master WHERE name LIKE 'coupon%'")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&tables.stdout).trim(),
        "0",
        "really a pre-coupon database"
    );
    let log = dir.join("deleted-accounts.log");
    std::fs::write(&log, "deleted usr_gone 2026-01-01T00:00:00.000Z\ndeleted usr_back 2026-01-01T00:00:00.000Z\nundeleted usr_back 2026-01-01T00:00:01.000Z\n").unwrap();
    assert!(replay(&db, &log).contains("replayed 1 deletion(s)"));
    let q = |sql: &str| {
        let o = std::process::Command::new("sqlite3")
            .arg(&db)
            .arg(sql)
            .output()
            .unwrap();
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    assert_eq!(q("SELECT group_concat(id) FROM users"), "usr_stay");
    assert_eq!(q("SELECT COUNT(*) FROM grants"), "0");
    assert_eq!(q("SELECT COUNT(*) FROM api_keys"), "0");
    assert_eq!(
        q("SELECT owner_user_id IS NULL FROM apps WHERE id='app_1'"),
        "1",
        "apps outlive their owner"
    );
    assert_eq!(
        q("SELECT plaza_status IS NULL FROM apps WHERE id='app_1'"),
        "1",
        "a deleted owner's Plaza listing is erased, as in delete_account"
    );
}

#[tokio::test]
async fn landing_page_qr_and_universal_link() {
    let e = env().await;
    let b = e.create_batch(0.5, 1.0).await;
    let code = b["coupon_code"].as_str().unwrap();
    let r =
        e.c.get(format!("{}/coupon/{code}", e.base))
            .send()
            .await
            .unwrap();
    assert_eq!(r.status(), 200);
    let html = r.text().await.unwrap();
    assert!(html.contains(&format!("keyala://coupon/{code}")));
    assert!(
        !html.contains("Sponsor App"),
        "the landing page shows nothing about the batch"
    );
    let q =
        e.c.get(format!("{}/coupon/{code}/qr.svg", e.base))
            .send()
            .await
            .unwrap();
    assert_eq!(q.status(), 200);
    assert!(q.text().await.unwrap().contains("<svg"));
    assert_eq!(
        e.c.get(format!("{}/coupon/bad", e.base))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let aasa =
        e.c.get(format!("{}/.well-known/apple-app-site-association", e.base))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
    assert!(aasa.contains("/coupon/*"));
}

/// An overshoot settlement (actual cost far above the held estimate) on a fresh batch; returns the mail kinds/subjects.
async fn overshoot_mails(per: f64, total: f64, actual: i64) -> (Vec<String>, Vec<String>) {
    let (e, b, u, tok, _cid, _g) = funded(per, total).await;
    let bid = b["id"].as_str().unwrap();
    let (_, _, r) = e.chat(&tok, "costs more than held", None).await;
    e.pause();
    e.post(&u, &format!("/v1/me/requests/{r}/approve"), json!({}))
        .await;
    sqlx::query("UPDATE requests SET status='uncertain' WHERE id=?")
        .bind(&r)
        .execute(&e.st.db)
        .await
        .unwrap();
    assert!(reconcile_hold(&e.st, &r, true, Some(actual)).await.unwrap());
    let mails = subjects(&e).await;
    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT kind FROM coupon_outbox WHERE batch_id=? ORDER BY kind")
            .bind(bid)
            .fetch_all(&e.st.db)
            .await
            .unwrap();
    (kinds, mails)
}

#[tokio::test]
async fn overshoot_sends_one_truthful_stopped_mail_and_never_a_contradictory_exhausted() {
    // 1 USD total, settled at 2 USD: beyond the total. 0.9 of 2 USD: below it. Either way: stopped only.
    for (per, total, actual) in [(1.0, 1.0, 2_000_000), (0.5, 2.0, 900_000)] {
        let (kinds, mails) = overshoot_mails(per, total, actual).await;
        assert!(kinds.contains(&"stopped".to_string()), "{kinds:?}");
        assert!(!kinds.contains(&"exhausted".to_string()), "{kinds:?}");
        assert!(
            !mails.iter().any(|m| m.contains("fully spent")),
            "{mails:?}"
        );
    }
}

#[tokio::test]
async fn stopped_mail_text_does_not_claim_unspent_budget() {
    let (e, b, u, tok, _cid, _g) = funded(1.0, 1.0).await;
    let (_, _, r) = e.chat(&tok, "x", None).await;
    e.pause();
    e.post(&u, &format!("/v1/me/requests/{r}/approve"), json!({}))
        .await;
    sqlx::query("UPDATE requests SET status='uncertain' WHERE id=?")
        .bind(&r)
        .execute(&e.st.db)
        .await
        .unwrap();
    assert!(reconcile_hold(&e.st, &r, true, Some(2_000_000))
        .await
        .unwrap());
    subjects(&e).await;
    let sent = e.st.mail_mock.lock().unwrap().sent.clone();
    let stopped: Vec<_> = sent
        .iter()
        .filter(|m| m.body["subject"].as_str().unwrap().contains("stopped"))
        .collect();
    assert_eq!(stopped.len(), 1, "{sent:?}");
    assert!(
        sent.len() <= 2,
        "capacity may accompany it, never exhausted: {sent:?}"
    );
    let text = stopped[0].body["text"].as_str().unwrap();
    assert!(
        text.contains("stopped") && text.contains("unexpectedly"),
        "{text}"
    );
    assert!(
        !text.contains("not fully spent") && !text.contains("fully spent"),
        "{text}"
    );
    let _ = b;
}

/// Leaves one mock-bound, pending (retry-waiting, due) row for the owner.
async fn mock_bound_pending(e: &E) {
    e.st.mail_mock.lock().unwrap().outcomes.extend([500; 5]);
    let b = e.create_batch(0.1, 0.25).await;
    let a = sess(&e.c, &e.base, "a@x.co").await;
    let a2 = sess(&e.c, &e.base, "b@x.co").await;
    e.claim(&a, b["coupon_code"].as_str().unwrap()).await;
    e.claim(&a2, b["coupon_code"].as_str().unwrap()).await;
    for _ in 0..200 {
        let n: i64 = e
            .q(
                "SELECT COUNT(*) FROM coupon_outbox WHERE attempts>=1 AND status='pending'",
                "",
            )
            .await;
        if n == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let (st, to, prov): (String, Option<String>, Option<String>) =
        sqlx::query_as("SELECT status, recipient, provider FROM coupon_outbox")
            .fetch_one(&e.st.db)
            .await
            .unwrap();
    assert_eq!(
        (st.as_str(), to.as_deref(), prov.as_deref()),
        ("pending", Some("owner@x.co"), Some("mock")),
        "bound at the first attempt, transport pinned"
    );
    sqlx::query("UPDATE coupon_outbox SET next_attempt_at='2000-01-01T00:00:00.000Z'")
        .execute(&e.st.db)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_mock_bound_message_is_never_handed_to_a_real_provider_after_restart() {
    let (c1, dir) = cfg(true);
    let url = c1.database_url.clone();
    let e = env_with((c1, dir)).await;
    mock_bound_pending(&e).await;
    // restart: mock off, Resend configured. The dev recipient must not receive real mail; nothing goes out.
    let (mut c2, _) = cfg(false);
    c2.database_url = url;
    c2.resend_api_key = Some("re_test_not_real".into());
    c2.email_from = Some("Keyala <noreply@mail.test>".into());
    let st2 = build_state(c2).await.unwrap();
    // make the dev address "verified" too: the provider pin alone must stop it
    sqlx::query(
        "UPDATE users SET apple_sub='001.x', notify_email='owner@x.co' WHERE email='owner@x.co'",
    )
    .execute(&st2.db)
    .await
    .unwrap();
    process_outbox(&st2).await;
    let (st, to, body, why): (String, Option<String>, Option<String>, Option<String>) =
        sqlx::query_as("SELECT status, recipient, body, last_error FROM coupon_outbox")
            .fetch_one(&st2.db)
            .await
            .unwrap();
    assert_eq!(
        (st.as_str(), to, body, why.as_deref()),
        ("skipped", None, None, Some("provider_changed")),
        "skipped and erased before any send path"
    );
    assert!(
        st2.mail_mock.lock().unwrap().sent.is_empty(),
        "and nothing was recorded by a mock either"
    );
}

#[tokio::test]
async fn a_retry_needs_the_same_verified_address_still_in_place() {
    // legitimate retry first: same sender and body, same key
    let e = env().await;
    mock_bound_pending(&e).await;
    let first = e.st.mail_mock.lock().unwrap().sent.clone();
    process_outbox(&e.st).await;
    flush(&e).await;
    let sent = e.st.mail_mock.lock().unwrap().sent.clone();
    assert_eq!(sent.len(), first.len() + 1, "retried");
    let (a, b) = (&sent[sent.len() - 2], &sent[sent.len() - 1]);
    assert_eq!(a.idempotency_key, b.idempotency_key);
    assert_eq!(a.body, b.body, "exact sender, recipient and text preserved");

    // the verified address changes (federated account now verified under another address): pending mail is erased
    let e = env().await;
    mock_bound_pending(&e).await;
    let n = e.st.mail_mock.lock().unwrap().sent.len();
    sqlx::query("UPDATE users SET apple_sub='001.x', notify_email='new@icloud.com' WHERE email='owner@x.co'")
        .execute(&e.st.db)
        .await
        .unwrap();
    process_outbox(&e.st).await;
    let (st, to, why): (String, Option<String>, Option<String>) =
        sqlx::query_as("SELECT status, recipient, last_error FROM coupon_outbox")
            .fetch_one(&e.st.db)
            .await
            .unwrap();
    assert_eq!(
        (st.as_str(), to, why.as_deref()),
        ("skipped", None, Some("recipient_changed"))
    );
    assert_eq!(e.st.mail_mock.lock().unwrap().sent.len(), n, "nothing sent");

    // the marker removed: no verified address at all
    let e = env().await;
    mock_bound_pending(&e).await;
    sqlx::query("UPDATE users SET apple_sub='001.x', notify_email=NULL WHERE email='owner@x.co'")
        .execute(&e.st.db)
        .await
        .unwrap();
    process_outbox(&e.st).await;
    let (st, to, why): (String, Option<String>, Option<String>) =
        sqlx::query_as("SELECT status, recipient, last_error FROM coupon_outbox")
            .fetch_one(&e.st.db)
            .await
            .unwrap();
    assert_eq!(
        (st.as_str(), to, why.as_deref()),
        ("skipped", None, Some("no_verified_recipient"))
    );
}

/// Staging already runs migrations 0001..0017 (Plaza). Apply exactly those with SQLx's own Migrator
/// (so `_sqlx_migrations` holds their versions and checksums), add rows, then boot the current build,
/// which must apply only 0018 on top and keep everything.
#[tokio::test]
async fn sqlx_upgrade_from_17_plaza_to_18_coupon_keeps_data_and_adds_coupons() {
    let (cfg, dir) = cfg(true);
    let old = dir.join("published-17");
    std::fs::create_dir_all(&old).unwrap();
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for f in std::fs::read_dir(&src).unwrap() {
        let p = f.unwrap().path();
        if !p.file_name().unwrap().to_str().unwrap().starts_with("0018") {
            std::fs::copy(&p, old.join(p.file_name().unwrap())).unwrap();
        }
    }
    let opts = cfg
        .database_url
        .parse::<sqlx::sqlite::SqliteConnectOptions>()
        .unwrap()
        .foreign_keys(true);
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_with(opts)
        .await
        .unwrap();
    let m = sqlx::migrate::Migrator::new(old.as_path()).await.unwrap();
    assert_eq!(m.iter().map(|x| x.version).max(), Some(17));
    m.run(&db).await.unwrap();
    let applied: Vec<(i64, bool)> =
        sqlx::query_as("SELECT version, success FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&db)
            .await
            .unwrap();
    assert_eq!(applied.len(), 17);
    assert!(applied.iter().all(|(_, ok)| *ok));
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE name LIKE 'coupon%'")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(n, 0, "really the 17 schema");
    for q in [
        "INSERT INTO users (id,email,created_at) VALUES ('usr_old','old@x.co','t')",
        "INSERT INTO apps (id,name,secret_hash,created_at,owner_user_id,plaza_status,plaza_at) VALUES ('app_old','Old','h','t','usr_old','approved','t')",
        "INSERT INTO grants (id,user_id,app_id,vendor,mode,created_at) VALUES ('grt_old','usr_old','app_old','openai','ask','t')",
    ] {
        sqlx::query(q).execute(&db).await.unwrap();
    }
    db.close().await;

    // the current build on the same database file
    let st = build_state(cfg).await.unwrap();
    let versions: Vec<(i64, bool)> =
        sqlx::query_as("SELECT version, success FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&st.db)
            .await
            .unwrap();
    assert_eq!(versions.len(), 18);
    assert_eq!(versions.last(), Some(&(18, true)));
    assert!(versions.iter().all(|(_, ok)| *ok));
    let row: (String, Option<String>) =
        sqlx::query_as("SELECT email, notify_email FROM users WHERE id='usr_old'")
            .fetch_one(&st.db)
            .await
            .unwrap();
    assert_eq!(row, ("old@x.co".to_string(), None));
    let app: (String, Option<String>) =
        sqlx::query_as("SELECT owner_user_id, plaza_status FROM apps WHERE id='app_old'")
            .fetch_one(&st.db)
            .await
            .unwrap();
    assert_eq!(
        (app.0.as_str(), app.1.as_deref()),
        ("usr_old", Some("approved"))
    );
    let g: Option<String> = sqlx::query_scalar("SELECT coupon_id FROM grants WHERE id='grt_old'")
        .fetch_one(&st.db)
        .await
        .unwrap();
    assert_eq!(g, None);
    for t in [
        "coupon_batches",
        "coupons",
        "coupon_ledger",
        "coupon_outbox",
    ] {
        let c: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {t}"))
            .fetch_one(&st.db)
            .await
            .unwrap();
        assert_eq!(c, 0, "{t} exists and is empty");
    }
    // the upgraded database serves the coupon API for the old user and app
    let app = router(st.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(l, app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .unwrap()
    });
    let c = Client::new();
    let tok = sess(&c, &base, "old@x.co").await;
    let me: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email='old@x.co'")
        .fetch_one(&st.db)
        .await
        .unwrap();
    assert_eq!(me, 1, "dev login reuses the old user");
    let r = c
        .get(format!("{base}/v1/me/apps/app_old/coupons"))
        .bearer_auth(&tok)
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status().as_u16(),
        200,
        "coupon list works on the upgraded app"
    );
}

#[tokio::test]
async fn console_catalog_models_create_coupons_for_each_provider() {
    let e = env().await;
    let script =
        e.c.get(format!("{}/console.js", e.base))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
    assert!(!script.contains("/*COUPON_MODELS*/"));
    let json = script
        .split_once("const couponModels = ")
        .unwrap()
        .1
        .split(';')
        .next()
        .unwrap();
    let catalog: Vec<Value> = serde_json::from_str(json).unwrap();
    assert_eq!(catalog.len(), keyala_server::pricing::MODELS.len());
    for (vendor, key) in [
        ("openai", "sk-fake-picker-only"),
        ("anthropic", "sk-ant-fake-picker-only"),
        ("openrouter", "sk-or-fake-picker-only"),
    ] {
        let models: Vec<&str> = catalog
            .iter()
            .filter(|m| m["vendor"] == vendor)
            .map(|m| m["name"].as_str().unwrap())
            .collect();
        assert!(!models.is_empty());
        if vendor == "openrouter" {
            assert!(models.contains(&"openai/gpt-4o-mini"));
            assert!(!models.contains(&"gpt-4o-mini"));
        }
        let (status, batch) = e.post(&e.owner, &format!("/v1/me/apps/{}/coupons", e.app_id),
            json!({"vendor": vendor, "models": models, "api_key": key, "per_coupon_usd": 1, "total_usd": 3, "claim_deadline": deadline()})).await;
        assert_eq!(status, 200, "{batch}");
        assert_eq!(batch["vendor"], vendor);
        assert_eq!(batch["models"], json!(models));
    }
}
