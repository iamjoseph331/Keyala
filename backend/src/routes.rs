//! Developer-facing (apps, connect) and user-facing (auth, keys, grants, requests, events) routes.
use crate::*;
use axum::{
    extract::{ConnectInfo, Path, Query, State},
    response::{
        sse::{Event, KeepAlive, Sse},
        Html,
    },
};
use std::{net::SocketAddr, time::Instant};
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};

pub const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // no 0/O/1/I
pub fn vendor_ok(v: &str) -> R<()> {
    if matches!(v, "openai" | "anthropic" | "openrouter") {
        Ok(())
    } else {
        Err(bad("vendor must be openai, anthropic or openrouter"))
    }
}
pub fn not_found(what: &str) -> ApiError {
    api(404, "not_found", format!("{what} not found"))
}

// ---------- developer: apps & connect ----------

/// ponytail: in-memory per-IP sliding window, resets on restart and is per-instance; move to SQLite/edge if abused.
/// Behind a proxy (Render) the last X-Forwarded-For entry is the one the proxy added.
pub fn rate_limit(
    s: &AppState,
    peer: SocketAddr,
    headers: &HeaderMap,
    bucket: &str,
    max: usize,
    window: u64,
    what: &str,
) -> R<()> {
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .map(|v| v.trim().to_string())
        .unwrap_or_else(|| peer.ip().to_string());
    let mut l = s.app_limits.lock().unwrap();
    let hits = l.entry(format!("{bucket}:{ip}")).or_default();
    hits.retain(|t| t.elapsed() < Duration::from_secs(window));
    if hits.len() >= max {
        return Err(api(
            429,
            "rate_limited",
            format!("too many {what} from this address; try again later"),
        ));
    }
    hits.push(Instant::now());
    Ok(())
}

#[derive(Deserialize)]
pub struct NewApp {
    pub name: String,
    pub homepage: Option<String>,
    pub icon_url: Option<String>,
    /// browser (public) client: web origins allowed to start a connect with just the app id
    pub allowed_origins: Option<Vec<String>>,
}

pub async fn create_app(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    J(b): J<NewApp>,
) -> R<Json<Value>> {
    rate_limit(&s, peer, &headers, "apps", 10, 3600, "apps registered")?;
    register_app(&s, b, None).await
}

/// Name, homepage and origins rules shared by create and edit.
pub fn check_app(b: &NewApp) -> R<()> {
    let name = b.name.trim();
    if name.is_empty() || name.len() > 100 {
        return Err(bad("name is required (max 100 chars)"));
    }
    if b.allowed_origins.as_deref().is_some_and(|o| !origins_ok(o)) {
        return Err(bad("allowed_origins: 1-10 origins like https://example.com (http only for localhost), no path"));
    }
    if [&b.homepage, &b.icon_url]
        .iter()
        .any(|u| u.as_ref().is_some_and(|u| u.len() > 300))
    {
        return Err(bad("homepage and icon_url: max 300 chars"));
    }
    Ok(())
}

/// Creates an app (optionally owned by an account) and returns its id and one-time secret.
pub async fn register_app(s: &AppState, b: NewApp, owner: Option<&str>) -> R<Json<Value>> {
    check_app(&b)?;
    let name = b.name.trim();
    let origins = b
        .allowed_origins
        .as_ref()
        .map(|o| serde_json::to_string(o).unwrap_or_default());
    let rec = AppRecord {
        id: new_id("app"),
        name: name.to_string(),
        homepage: b.homepage,
        icon_url: b.icon_url,
        allowed_origins: b.allowed_origins,
        gen: 0,
    };
    let (id, secret) = (rec.id.clone(), seal_app(&s.cfg.master_key, &rec));
    sqlx::query("INSERT INTO apps (id,name,homepage,icon_url,secret_hash,allowed_origins,created_at,owner_user_id) VALUES (?,?,?,?,?,?,?,?)")
        .bind(&id).bind(&rec.name).bind(&rec.homepage).bind(&rec.icon_url).bind(hash_token(&secret)).bind(&origins).bind(now()).bind(owner)
        .execute(&s.db).await?;
    Ok(Json(json!({"app_id": id, "app_secret": secret})))
}

#[derive(Deserialize)]
pub struct NewConnect {
    vendor: String,
    suggested_policy: Option<PolicyIn>,
    /// browser clients only: identifies the app instead of the app secret
    app_id: Option<String>,
    /// sponsored connect: a coupon batch code of this app and vendor (the user needs no key of their own)
    coupon_code: Option<String>,
}

pub async fn create_connect(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    J(b): J<NewConnect>,
) -> R<Json<Value>> {
    // Server apps authenticate with the app secret. Browser apps send their app id, and the
    // browser's Origin must be one the app registered. That binding is browser-enforced only (a
    // non-browser client can forge Origin), so the phone shows the origin and every call still
    // follows the policy the user approves. The grant goes to whoever holds `connect_secret`.
    let (app_id, connect_secret, origin) = if headers.contains_key("authorization") {
        (auth_app(&s, &headers).await?, None, None)
    } else {
        let app_id = b.app_id.as_deref().ok_or_else(|| {
            api(
                401,
                "unauthorized",
                "send the app secret, or app_id from an allowed browser origin",
            )
        })?;
        let origin = headers
            .get("origin")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let allowed: Option<String> = sqlx::query_scalar(
            "SELECT allowed_origins FROM apps WHERE id=? AND deleted_at IS NULL",
        )
        .bind(app_id)
        .fetch_optional(&s.db)
        .await?
        .flatten();
        let allowed: Vec<String> = allowed
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();
        if !allowed.iter().any(|o| o == origin) {
            return Err(api(
                403,
                "origin_not_allowed",
                "this origin is not allowed for this app",
            ));
        }
        rate_limit(&s, peer, &headers, "connect", 20, 600, "connect codes")?;
        (
            app_id.to_string(),
            Some(new_token("keyala_cs_")),
            Some(origin.to_string()),
        )
    };
    vendor_ok(&b.vendor)?;
    // the code is matched exactly as sent (case-sensitive). A code of another app or vendor is "not found":
    // nothing is revealed about batches that are not this app's
    let coupon_batch: Option<String> = match b.coupon_code.as_deref() {
        None => None,
        Some(code) => {
            let row: Option<(String, String, String, Option<String>)> = if crate::coupons::code_ok(
                code,
            ) {
                sqlx::query_as(
                        "SELECT b.id, b.app_id, b.vendor, b.revoked_at FROM coupon_batches b JOIN apps a ON a.id=b.app_id WHERE b.code=? AND a.deleted_at IS NULL",
                    )
                    .bind(code)
                    .fetch_optional(&s.db)
                    .await?
            } else {
                None
            };
            let (id, bapp, bvendor, revoked) = row.ok_or_else(crate::coupons::e_not_found)?;
            if bapp != app_id || bvendor != b.vendor {
                return Err(crate::coupons::e_not_found());
            }
            if revoked.is_some() {
                return Err(api(
                    410,
                    "coupon_revoked",
                    "this coupon was revoked by its issuer",
                ));
            }
            Some(id)
        }
    };
    let p = b
        .suggested_policy
        .unwrap_or(PolicyIn {
            mode: "ask".into(),
            ..Default::default()
        })
        .parse()?;
    if coupon_batch.is_some() && p.mode != "ask" {
        return Err(sponsored_ask_only());
    }
    let sp = policy_json(&p.mode, p.count, p.micro, p.alert, &p.expires_at).to_string();
    let expires = after(600);
    for _ in 0..5 {
        let code = format!(
            "KY-{}",
            (0..4)
                .map(|_| CODE_ALPHABET
                    [rand::Rng::gen_range(&mut rand::thread_rng(), 0..CODE_ALPHABET.len())]
                    as char)
                .collect::<String>()
        );
        let r = sqlx::query("INSERT OR IGNORE INTO connect_codes (code,app_id,vendor,suggested_policy,created_at,expires_at,secret_hash,origin,coupon_batch_id) VALUES (?,?,?,?,?,?,?,?,?)")
            .bind(&code).bind(&app_id).bind(&b.vendor).bind(&sp).bind(now()).bind(&expires).bind(connect_secret.as_deref().map(hash_token)).bind(&origin).bind(&coupon_batch)
            .execute(&s.db).await?;
        if r.rows_affected() == 1 {
            let url = format!("{}/connect/{code}", s.cfg.public_url);
            let mut v = json!({"connect_code": code, "connect_url": url, "connect_qr_url": format!("{url}/qr.svg"), "expires_at": expires});
            if let Some(cs) = connect_secret {
                v["connect_secret"] = cs.into();
            }
            return Ok(Json(v));
        }
    }
    Err(api(500, "internal", "could not allocate a connect code"))
}

pub async fn poll_connect(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> R<Json<Value>> {
    let row: Option<(String, Option<String>, String, Option<String>)> = if let Some(cs) =
        bearer(&headers, "keyala_cs_")
    {
        sqlx::query_as("SELECT status, grant_id, expires_at, seen FROM connect_codes WHERE code=? AND secret_hash=?").bind(&code).bind(hash_token(cs)).fetch_optional(&s.db).await?
    } else {
        let app_id = auth_app(&s, &headers).await?;
        sqlx::query_as("SELECT status, grant_id, expires_at, seen FROM connect_codes WHERE code=? AND app_id=?").bind(&code).bind(&app_id).fetch_optional(&s.db).await?
    };
    let (status, grant_id, exp, seen) = row.ok_or_else(|| not_found("connect code"))?;
    if status == "pending" {
        if exp.as_str() <= now().as_str() {
            return Ok(Json(json!({"status": "expired"})));
        }
        return Ok(Json(json!({"status": "pending", "seen": seen})));
    }
    if let (true, Some(gid)) = (status == "approved", grant_id) {
        // The token is generated on first poll after approval and shown exactly once
        // (only its hash is stored). Lost response => run a new connect flow.
        let tok = new_token("keyala_grant_");
        let r = sqlx::query("UPDATE grants SET token_hash=? WHERE id=? AND token_hash IS NULL")
            .bind(hash_token(&tok))
            .bind(&gid)
            .execute(&s.db)
            .await?;
        if r.rows_affected() == 1 {
            return Ok(Json(json!({"status": "approved", "grant_token": tok})));
        }
    }
    Ok(Json(json!({"status": status})))
}

/// Sponsored calls are approved one at a time, always: no standing policy may be set on them.
pub fn sponsored_ask_only() -> ApiError {
    crate::coupons::e_policy()
}

pub fn connect_code_ok(code: &str) -> bool {
    code.len() == 7
        && code.starts_with("KY-")
        && code[3..].bytes().all(|c| CODE_ALPHABET.contains(&c))
}

/// What a phone without the app sees after scanning; on a computer, the QR to scan.
pub async fn connect_page(
    State(s): State<AppState>,
    Path(code): Path<String>,
) -> Result<Html<String>, ApiError> {
    if !connect_code_ok(&code) {
        return Err(not_found("connect code"));
    }
    // Reached in a browser, so the app did not catch the link (usually: not installed). Tell the developer's pairing UI.
    sqlx::query(
        "UPDATE connect_codes SET seen='web' WHERE code=? AND seen IS NULL AND status='pending'",
    )
    .bind(&code)
    .execute(&s.db)
    .await?;
    Ok(Html(format!(
        "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'><title>Keyala</title>\
         <body style='font-family:system-ui;text-align:center;margin:3rem 1rem;line-height:1.5'><h1>Connect with Keyala</h1>\
         <p><a href='keyala://connect/{code}' style='font-size:1.3rem'>Open in the Keyala app</a></p>\
         <p>No app yet? <a href='{GET_APP_URL}'>Get Keyala</a>, sign in, then come back and tap the link above again (the code is valid for 10 minutes).</p>\
         <p style='color:#666'>Or enter <b>{code}</b> in the app.</p>\
         <p style='color:#666;margin-top:2rem'>On a computer? Scan with your iPhone camera:</p><img src='/connect/{code}/qr.svg' width=200 height=200 alt='QR code for {code}'>{}", crate::enroll::TRUST_LINK
    )))
}

/// Where "Get Keyala" points until there is an App Store listing.
pub const GET_APP_URL: &str = "https://keyala.io/";

// ---------- user: auth, keys, devices ----------

#[derive(Deserialize)]
pub struct DevLogin {
    email: String,
}

pub async fn dev_login(State(s): State<AppState>, J(b): J<DevLogin>) -> R<Json<Value>> {
    if !s.cfg.dev_login {
        return Err(not_found("endpoint"));
    }
    let email = b.email.trim().to_lowercase();
    // `.invalid` is reserved for the operator password account and Apple placeholders: never reachable by typing an email
    if !email.contains('@') || email.len() > 200 || email.ends_with(".invalid") {
        return Err(bad("a valid email is required"));
    }
    sqlx::query("INSERT OR IGNORE INTO users (id,email,created_at) VALUES (?,?,?)")
        .bind(new_id("usr"))
        .bind(&email)
        .bind(now())
        .execute(&s.db)
        .await?;
    let uid: String = sqlx::query_scalar("SELECT id FROM users WHERE email=?")
        .bind(&email)
        .fetch_one(&s.db)
        .await?;
    new_session(&s, &uid, &email, "dev").await
}

#[derive(Deserialize)]
pub struct PasswordLogin {
    username: String,
    password: String,
}

/// Two kinds of password sign-in, both answered with the same 401 so nothing says which part was wrong:
/// 1. the operator's single account (KEYALA_PASSWORD_LOGIN, identity `{username}@password.invalid`, never
///    linked to Apple or a real email): an app-scoped session, exactly as before;
/// 2. a user's web password (set from the app with `PUT /v1/me/password`), username = account email:
///    a web-scoped session (12 h, read-only plus grant revoke and developer apps).
// ponytail: operator verifier is unsalted SHA-256, fine only because that password is random and high-entropy.
pub async fn password_login(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    J(b): J<PasswordLogin>,
) -> R<Json<Value>> {
    rate_limit(&s, peer, &headers, "password", 10, 600, "sign-in attempts")?;
    let name = b.username.trim().to_lowercase();
    if let Some((user, hash)) = &s.cfg.password_login {
        // compare both always, so timing reveals neither which part was wrong nor how much matched
        let user_ok = ct_eq(
            Sha256::digest(name.as_bytes()).as_slice(),
            Sha256::digest(user.as_bytes()).as_slice(),
        );
        let pass_ok = ct_eq(Sha256::digest(b.password.as_bytes()).as_slice(), hash);
        if user_ok & pass_ok {
            tracing::info!("password sign-in verified");
            let email = format!("{user}@password.invalid");
            sqlx::query("INSERT OR IGNORE INTO users (id,email,created_at) VALUES (?,?,?)")
                .bind(new_id("usr"))
                .bind(&email)
                .bind(now())
                .execute(&s.db)
                .await?;
            let uid: String = sqlx::query_scalar("SELECT id FROM users WHERE email=?")
                .bind(&email)
                .fetch_one(&s.db)
                .await?;
            return new_session(&s, &uid, &email, "password").await;
        }
    }
    // always one argon2 verify, against a dummy hash for unknown accounts, so timing stays flat
    let row: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, email, password_hash FROM users WHERE lower(email)=? AND password_hash IS NOT NULL",
    )
    .bind(&name)
    .fetch_optional(&s.db)
    .await?;
    let stored = row.as_ref().and_then(|r| r.2.clone());
    if crate::console::verify_password(b.password, stored).await {
        if let Some((uid, email, _)) = row {
            tracing::info!("web password sign-in verified");
            return session_for(&s, &uid, &email, "password", true).await;
        }
    }
    tracing::warn!("password sign-in rejected");
    Err(api(
        401,
        "invalid_credentials",
        "wrong username or password",
    ))
}

/// `auth_provider` (apple | google | password | dev) is set by the login route that verified the identity, so
/// clients need not guess from the email. Authorization never reads it.
pub(crate) async fn new_session(
    s: &AppState,
    uid: &str,
    email: &str,
    provider: &str,
) -> R<Json<Value>> {
    session_for(s, uid, email, provider, false).await
}

/// `web` sessions expire after 12 hours and are refused by every `ACT` handler (see `auth_level`).
pub(crate) async fn session_for(
    s: &AppState,
    uid: &str,
    email: &str,
    provider: &str,
    web: bool,
) -> R<Json<Value>> {
    let tok = new_token("keyala_sess_");
    let exp = web.then(|| after(WEB_TTL_SECS));
    sqlx::query(
        "INSERT INTO sessions (token_hash,user_id,created_at,scope,expires_at) VALUES (?,?,?,?,?)",
    )
    .bind(hash_token(&tok))
    .bind(uid)
    .bind(now())
    .bind(if web { "web" } else { "app" })
    .bind(&exp)
    .execute(&s.db)
    .await?;
    let mut v = json!({"session_token": tok, "user": {"id": uid, "email": email, "auth_provider": provider}});
    if let Some(e) = exp {
        v["expires_at"] = e.into();
        v["scope"] = "web".into();
    }
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct AppleLogin {
    id_token: String,
}

/// Identity is Apple's `sub`, never the email (emails can be relays, changed, or collide).
pub async fn apple_login(State(s): State<AppState>, J(b): J<AppleLogin>) -> R<Json<Value>> {
    // one line per attempt (outcome only: no token, sub or IP) so hosted sign-in failures are diagnosable
    let c = crate::apple::verify(&s, &b.id_token)
        .await
        .inspect_err(|e| tracing::warn!("apple sign-in rejected: {} ({})", e.1, e.2))?;
    tracing::info!("apple sign-in verified");
    // the account address is kept as before; only a VERIFIED address ever becomes a notification address
    let verified = c.email.clone().filter(|_| c.email_verified());
    idp_login(&s, "apple", &c.sub, c.email, verified).await
}

/// Which sign-in methods this server offers, so clients can hide buttons that would 404/fail.
/// Passkeys need an https public URL on the RP ID's own host (a browser/OS refuses anything else).
pub async fn auth_methods(State(s): State<AppState>) -> Json<Value> {
    let host = s
        .cfg
        .public_url
        .strip_prefix("https://")
        .and_then(|r| r.split(['/', ':']).next());
    let passkeys = host == Some(s.cfg.passkey_rp_id.as_str());
    Json(json!({
        "google": !s.cfg.google_client_ids.is_empty(),
        "passkeys": passkeys,
        "passkey_rp_id": passkeys.then(|| s.cfg.passkey_rp_id.clone()),
    }))
}

/// Google is its own identity (`google_sub`): a Google sign-in creates a new account and is never
/// linked to an Apple or other account, not even when the email matches.
pub async fn google_login(State(s): State<AppState>, J(b): J<AppleLogin>) -> R<Json<Value>> {
    if s.cfg.google_client_ids.is_empty() {
        return Err(not_found("Google sign-in"));
    }
    let c = crate::apple::verify_google(&s, &b.id_token)
        .await
        .inspect_err(|e| tracing::warn!("google sign-in rejected: {} ({})", e.1, e.2))?;
    tracing::info!("google sign-in verified");
    let email = c.email.clone().filter(|_| c.email_verified());
    idp_login(&s, "google", &c.sub, email.clone(), email).await
}

/// Find or create the account for a verified provider subject. `provider` is "apple" | "google"
/// (column `{provider}_sub`, placeholder `{sub}@{provider}.invalid`).
async fn idp_login(
    s: &AppState,
    provider: &'static str,
    sub: &str,
    email: Option<String>,
    verified: Option<String>,
) -> R<Json<Value>> {
    // the address of a valid, verified identity token: the only source of coupon notification mail. Existing
    // accounts have none until their next sign-in refreshes it.
    let verified = verified
        .map(|e| e.trim().to_lowercase())
        .filter(|e| e.contains('@') && e.len() <= 200);
    let by_sub = format!("SELECT id, email FROM users WHERE {provider}_sub=?");
    let existing: Option<(String, String)> = sqlx::query_as(&by_sub)
        .bind(sub)
        .fetch_optional(&s.db)
        .await?;
    if let Some((uid, email)) = existing {
        sqlx::query("UPDATE users SET notify_email=? WHERE id=?")
            .bind(&verified)
            .bind(&uid)
            .execute(&s.db)
            .await?;
        return new_session(s, &uid, &email, provider).await;
    }
    // never attach to an existing account by email: if it is taken, use a placeholder.
    let mut email = email
        .map(|e| e.trim().to_lowercase())
        .filter(|e| e.contains('@') && e.len() <= 200);
    if let Some(e) = &email {
        if sqlx::query_scalar::<_, i64>("SELECT 1 FROM users WHERE email=?")
            .bind(e)
            .fetch_optional(&s.db)
            .await?
            .is_some()
        {
            email = None;
        }
    }
    let email = email.unwrap_or_else(|| format!("{sub}@{provider}.invalid"));
    let uid = new_id("usr");
    sqlx::query(&format!(
        "INSERT OR IGNORE INTO users (id,email,created_at,{provider}_sub) VALUES (?,?,?,?)"
    ))
    .bind(&uid)
    .bind(&email)
    .bind(now())
    .bind(sub)
    .execute(&s.db)
    .await?;
    // a concurrent first login may have won the insert: always read back by sub
    let (uid, email): (String, String) = sqlx::query_as(&by_sub).bind(sub).fetch_one(&s.db).await?;
    sqlx::query("UPDATE users SET notify_email=? WHERE id=?")
        .bind(&verified)
        .bind(&uid)
        .execute(&s.db)
        .await?;
    new_session(s, &uid, &email, provider).await
}

pub async fn list_keys(State(s): State<AppState>, h: HeaderMap) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT vendor,last4,created_at FROM api_keys WHERE user_id=? ORDER BY vendor",
    )
    .bind(u)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(v, l, c)| json!({"vendor": v, "last4": l, "created_at": c}))
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct PutKey {
    api_key: String,
}

pub async fn put_key(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(vendor): Path<String>,
    J(b): J<PutKey>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, ACT).await?;
    let (key, checked) = check_key(&s, &vendor, &b.api_key).await?;
    let mut c = s.db.acquire().await?;
    let mut k = store_key(&s, &mut c, &u, &vendor, &key).await?;
    k["checked"] = checked.into();
    Ok(Json(k))
}

pub fn last4(key: &str) -> String {
    key.chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// Shape check, then the vendor's free model list (no inference, no cost) to catch a wrong or
/// revoked key before an app tries it. Admin keys are refused: they can manage the whole org.
/// Only a 401 rejects. A 403 can be a restricted key without the models-list permission, and the
/// vendor may be down or rate-limit us: then the key is kept and `checked` is false.
pub async fn check_key(s: &AppState, vendor: &str, raw: &str) -> R<(String, bool)> {
    vendor_ok(vendor)?;
    let key = raw.trim();
    if key.len() < 8 || key.len() > 512 || !key.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(bad("api_key looks invalid"));
    }
    let shape = match vendor {
        "openai" => {
            key.starts_with("sk-")
                && !key.starts_with("sk-ant-")
                && !key.starts_with("sk-or-")
                && !key.starts_with("sk-admin-")
        }
        "openrouter" => key.starts_with("sk-or-"),
        _ => key.starts_with("sk-ant-") && !key.starts_with("sk-ant-admin"),
    };
    if !shape {
        let msg = match vendor {
            "openai" => "This is not an OpenAI API key. It starts with sk-proj- or sk- (admin keys are not accepted).",
            "openrouter" => "This is not an OpenRouter API key. It starts with sk-or-.",
            _ => "This is not an Anthropic API key. It starts with sk-ant-api (admin keys are not accepted).",
        };
        return Err(api(400, "invalid_key", msg));
    }
    let status = if s.cfg.vendor_mock {
        if key.contains("KEYALA_MOCK_INVALID") {
            401
        } else if key.contains("KEYALA_MOCK_RESTRICTED") {
            403
        } else {
            200
        }
    } else {
        let req = if vendor == "openai" {
            s.http
                .get("https://api.openai.com/v1/models")
                .bearer_auth(key)
        } else if vendor == "openrouter" {
            s.http
                .get("https://openrouter.ai/api/v1/key")
                .bearer_auth(key)
        } else {
            s.http
                .get("https://api.anthropic.com/v1/models")
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
        };
        match req.timeout(Duration::from_secs(10)).send().await {
            Ok(r) => r.status().as_u16(),
            Err(_) => 0,
        }
    };
    if status == 401 {
        return Err(api(
            400,
            "invalid_key",
            "the provider rejected this key (wrong, revoked or disabled)",
        ));
    }
    Ok((key.to_string(), status == 200))
}

pub async fn store_key(
    s: &AppState,
    c: &mut SqliteConnection,
    user: &str,
    vendor: &str,
    key: &str,
) -> R<Value> {
    let l4 = last4(key);
    let (ct, nonce) = encrypt(&s.cfg.master_key, &key_aad(user, vendor), key);
    let created = now();
    sqlx::query("INSERT INTO api_keys (user_id,vendor,ciphertext,nonce,last4,created_at) VALUES (?,?,?,?,?,?) ON CONFLICT(user_id,vendor) DO UPDATE SET ciphertext=excluded.ciphertext, nonce=excluded.nonce, last4=excluded.last4, created_at=excluded.created_at")
        .bind(user).bind(vendor).bind(ct).bind(nonce).bind(&l4).bind(&created).execute(c).await?;
    Ok(json!({"vendor": vendor, "last4": l4, "created_at": created}))
}

pub async fn delete_key(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(vendor): Path<String>,
) -> R<StatusCode> {
    let u = auth_level(&s, &h, ACT).await?;
    sqlx::query("DELETE FROM api_keys WHERE user_id=? AND vendor=?")
        .bind(u)
        .bind(vendor)
        .execute(&s.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct Device {
    apns_token: String,
}

pub async fn add_device(State(s): State<AppState>, h: HeaderMap, J(b): J<Device>) -> R<StatusCode> {
    let u = auth_session(&s, &h).await?;
    if b.apns_token.is_empty() || b.apns_token.len() > 400 {
        return Err(bad("apns_token required"));
    }
    // real APNs delivery is M4; for now the token is only stored.
    sqlx::query("INSERT OR IGNORE INTO devices (user_id,apns_token,created_at) VALUES (?,?,?)")
        .bind(u)
        .bind(b.apns_token)
        .bind(now())
        .execute(&s.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- user: connect ----------

#[derive(sqlx::FromRow)]
struct ConnectRow {
    app_id: String,
    vendor: String,
    suggested_policy: String,
    expires_at: String,
    origin: Option<String>,
    coupon_batch_id: Option<String>,
    app_name: String,
    app_homepage: Option<String>,
    app_icon_url: Option<String>,
}

async fn pending_connect(s: &AppState, code: &str) -> R<ConnectRow> {
    let c: ConnectRow = sqlx::query_as("SELECT c.app_id, c.vendor, c.suggested_policy, c.expires_at, c.origin, c.coupon_batch_id, a.name AS app_name, a.homepage AS app_homepage, a.icon_url AS app_icon_url FROM connect_codes c JOIN apps a ON a.id=c.app_id WHERE c.code=? AND c.status='pending'")
        .bind(code).fetch_optional(&s.db).await?.ok_or_else(|| not_found("connect code"))?;
    if c.expires_at.as_str() <= now().as_str() {
        return Err(api(410, "expired", "connect code expired"));
    }
    Ok(c)
}

// ---------- account preference: route OpenAI/Anthropic apps through OpenRouter ----------

#[derive(Deserialize)]
pub struct PrefsIn {
    openrouter_for_direct_apis: bool,
}

async fn prefs_json(s: &AppState, user: &str) -> R<Json<Value>> {
    let on: i64 = sqlx::query_scalar("SELECT openrouter_for_direct FROM users WHERE id=?")
        .bind(user)
        .fetch_one(&s.db)
        .await?;
    Ok(Json(json!({"openrouter_for_direct_apis": on != 0})))
}

pub async fn get_prefs(State(s): State<AppState>, h: HeaderMap) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    prefs_json(&s, &u).await
}

/// Only NEW connects follow the setting: existing grants keep the billing vendor they were approved with.
pub async fn patch_prefs(
    State(s): State<AppState>,
    h: HeaderMap,
    J(b): J<PrefsIn>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, ACT).await?;
    sqlx::query("UPDATE users SET openrouter_for_direct=? WHERE id=?")
        .bind(b.openrouter_for_direct_apis as i64)
        .bind(&u)
        .execute(&s.db)
        .await?;
    prefs_json(&s, &u).await
}

/// Who would be billed for a NEW grant of `vendor` for this user: `openrouter` when the setting is on and the
/// app speaks the OpenAI or Anthropic protocol, otherwise the vendor itself.
async fn billing_for(s: &AppState, user: &str, vendor: &str) -> R<String> {
    let on: i64 = sqlx::query_scalar("SELECT openrouter_for_direct FROM users WHERE id=?")
        .bind(user)
        .fetch_one(&s.db)
        .await?;
    Ok(if on != 0 && matches!(vendor, "openai" | "anthropic") {
        "openrouter".into()
    } else {
        vendor.into()
    })
}

pub async fn preview_connect(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(code): Path<String>,
) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    let c = pending_connect(&s, &code).await?;
    let sponsored = c.coupon_batch_id.is_some();
    let billing = if sponsored {
        c.vendor.clone()
    } else {
        billing_for(&s, &u, &c.vendor).await?
    };
    let coupon = match &c.coupon_batch_id {
        Some(b) => crate::coupons::preview_for(&s, &u, b).await?,
        None => Value::Null,
    };
    let sp: Value = serde_json::from_str(&c.suggested_policy).unwrap_or(Value::Null);
    let has_prior_grant: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM grants WHERE user_id=? AND app_id=?)")
            .bind(&u)
            .bind(&c.app_id)
            .fetch_one(&s.db)
            .await?;
    sqlx::query("UPDATE connect_codes SET seen='app' WHERE code=?")
        .bind(&code)
        .execute(&s.db)
        .await?;
    Ok(Json(
        json!({"app": app_json(&c.app_id, &c.app_name, &c.app_homepage, &c.app_icon_url), "vendor": c.vendor, "billing_vendor": billing, "suggested_policy": sp, "expires_at": c.expires_at, "origin": c.origin, "requires_ask": sponsored || !has_prior_grant, "coupon": coupon}),
    ))
}

#[derive(Deserialize)]
pub struct ApproveConnect {
    policy: Option<PolicyIn>,
    /// the route the phone showed in the preview; if the setting changed since, approval is refused (409 route_changed)
    billing_vendor: Option<String>,
}

pub async fn approve_connect(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(code): Path<String>,
    J(b): J<ApproveConnect>,
) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    let c = pending_connect(&s, &code).await?;
    let p = match b.policy {
        Some(p) => p.parse()?,
        None => serde_json::from_str::<PolicyIn>(&c.suggested_policy)
            .map_err(|_| bad("bad suggested policy"))?
            .parse()?,
    };
    let sponsored = c.coupon_batch_id.clone();
    if sponsored.is_some() && p.mode != "ask" {
        return Err(sponsored_ask_only());
    }
    // a coupon is its own funding: never the user's key, never OpenRouter routing
    let billing = if sponsored.is_some() {
        c.vendor.clone()
    } else {
        billing_for(&s, &u, &c.vendor).await?
    };
    if b.billing_vendor.as_deref().is_some_and(|x| x != billing) {
        return Err(api(
            409,
            "route_changed",
            format!("billing for this app is now {billing}; review the connection again"),
        ));
    }
    let has_key: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM api_keys WHERE user_id=? AND vendor=?")
            .bind(&u)
            .bind(&billing)
            .fetch_optional(&s.db)
            .await?;
    if has_key.is_none() && sponsored.is_none() {
        return Err(api(
            409,
            "no_key",
            format!("add your {billing} API key before approving"),
        ));
    }
    let gid = new_id("grt");
    let mut tx = s.db.begin().await?;
    // First statement is the write (single-use guard), serializing concurrent approvals
    // before checking history. If the policy is too broad, dropping this transaction rolls
    // the status change back so the user can retry with ask.
    let r = sqlx::query("UPDATE connect_codes SET status='approved', grant_id=? WHERE code=? AND status='pending' AND expires_at>?").bind(&gid).bind(&code).bind(now()).execute(&mut *tx).await?;
    if r.rows_affected() != 1 {
        return Err(api(
            409,
            "already_used",
            "connect code already used or expired",
        ));
    }
    let has_prior_grant: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM grants WHERE user_id=? AND app_id=?)")
            .bind(&u)
            .bind(&c.app_id)
            .fetch_one(&mut *tx)
            .await?;
    if !has_prior_grant && p.mode != "ask" {
        return Err(api(
            400,
            "policy_not_allowed_yet",
            "approve calls one at a time first; widen the policy later from Grants",
        ));
    }
    // claims the batch (or finds the coupon this account already holds) in the same transaction as the approval
    let coupon_id = match &sponsored {
        Some(batch) => {
            let id = crate::coupons::claim_in_tx(&mut tx, &u, batch).await?;
            // same checks as a call: this account's coupon, this app and vendor, open and with some balance left
            crate::coupons::check_use(&mut tx, &id, &u, &c.app_id, &c.vendor, None, 1).await?;
            Some(id)
        }
        None => None,
    };
    // billing vendor is frozen here: later changes of the setting never switch this grant
    let frozen = (billing != c.vendor).then_some(billing.as_str());
    sqlx::query("INSERT INTO grants (id,user_id,app_id,vendor,mode,created_at,country,billing_vendor,coupon_id) VALUES (?,?,?,?,'ask',?,?,?,?)").bind(&gid).bind(&u).bind(&c.app_id).bind(&c.vendor).bind(now()).bind(country(&s, &h)).bind(frozen).bind(&coupon_id).execute(&mut *tx).await?;
    set_policy(&mut tx, &gid, &p).await?;
    tx.commit().await?;
    if coupon_id.is_some() {
        crate::coupons::kick(&s);
    }
    let g = grant_for_user(&s, &u, &gid).await?;
    emit(&s, &u, "grant.updated", &g.json());
    Ok(Json(g.json()))
}

pub async fn deny_connect(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(code): Path<String>,
) -> R<StatusCode> {
    auth_session(&s, &h).await?;
    let r =
        sqlx::query("UPDATE connect_codes SET status='denied' WHERE code=? AND status='pending'")
            .bind(&code)
            .execute(&s.db)
            .await?;
    if r.rows_affected() != 1 {
        return Err(not_found("connect code"));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Country of the approving phone, from the operator's trusted edge header (KEYALA_COUNTRY_HEADER).
pub(crate) fn country(s: &AppState, h: &HeaderMap) -> Option<String> {
    let c = h.get(s.cfg.country_header.as_deref()?)?.to_str().ok()?;
    (c.len() == 2 && c.bytes().all(|b| b.is_ascii_alphanumeric()) && c != "XX")
        .then(|| c.to_ascii_uppercase())
}

// ---------- developer: app info, secret rotation, report ----------

pub async fn get_app(State(s): State<AppState>, h: HeaderMap) -> R<Json<Value>> {
    let id = auth_app(&s, &h).await?;
    let (name, homepage, icon_url, origins, created_at): (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
    ) = sqlx::query_as(
        "SELECT name, homepage, icon_url, allowed_origins, created_at FROM apps WHERE id=?",
    )
    .bind(&id)
    .fetch_one(&s.db)
    .await?;
    let origins: Option<Value> = origins.and_then(|o| serde_json::from_str(&o).ok());
    Ok(Json(
        json!({"app_id": id, "name": name, "homepage": homepage, "icon_url": icon_url, "allowed_origins": origins, "created_at": created_at}),
    ))
}

pub async fn rotate_secret(State(s): State<AppState>, h: HeaderMap) -> R<Json<Value>> {
    let id = auth_app(&s, &h).await?;
    rotate_app_secret(&s, &id).await
}

pub async fn rotate_app_secret(s: &AppState, id: &str) -> R<Json<Value>> {
    let (name, homepage, icon_url, origins, gen): (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
    ) = sqlx::query_as(
        "SELECT name, homepage, icon_url, allowed_origins, secret_gen FROM apps WHERE id=?",
    )
    .bind(id)
    .fetch_one(&s.db)
    .await?;
    let rec = AppRecord {
        id: id.to_string(),
        name,
        homepage,
        icon_url,
        allowed_origins: origins.and_then(|o| serde_json::from_str(&o).ok()),
        gen: gen + 1,
    };
    let secret = seal_app(&s.cfg.master_key, &rec);
    sqlx::query("UPDATE apps SET secret_hash=?, secret_gen=? WHERE id=?")
        .bind(hash_token(&secret))
        .bind(rec.gen)
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(Json(json!({"app_secret": secret})))
}

pub async fn app_report(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> R<Json<Value>> {
    let id = auth_app(&s, &h).await?;
    report_json(&s, &id, &q).await
}

pub async fn report_json(s: &AppState, id: &str, q: &HashMap<String, String>) -> R<Json<Value>> {
    let days: i64 = q
        .get("days")
        .map(|d| d.parse().map_err(|_| bad("days must be 1-365")))
        .transpose()?
        .unwrap_or(30);
    if !(1..=365).contains(&days) {
        return Err(bad("days must be 1-365"));
    }
    let today = chrono::Utc::now().date_naive();
    let from = today - chrono::Duration::days(days - 1);
    let since = format!("{from}T00:00:00");
    // token columns are written at completion (the response itself is encrypted and later purged)
    const TOK: &str = "COALESCE(r.input_tokens, 0)";
    const OUT: &str = "COALESCE(r.output_tokens, 0)";
    let base =
        "FROM requests r JOIN grants g ON g.id=r.grant_id WHERE r.app_id=? AND r.created_at>=?";
    let n = |st: &str| format!("SUM(r.status='{st}')");
    type Totals = (
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
    );
    let t: Totals = sqlx::query_as(&format!(
        "SELECT COUNT(*), COALESCE({},0), COALESCE({},0), COALESCE({},0), COALESCE({},0), COALESCE({},0), COALESCE({},0), \
         COALESCE(SUM(CASE WHEN r.status='completed' THEN r.actual_micro END),0), \
         COALESCE(SUM(CASE WHEN r.decided_at IS NOT NULL AND r.status NOT IN ('denied','expired') THEN r.estimated_micro END),0), \
         COALESCE(SUM(r.status='completed' AND r.actual_micro IS NULL),0), COALESCE(SUM({TOK}),0), COALESCE(SUM({OUT}),0), COUNT(DISTINCT r.user_id) {base}",
        n("completed"), n("denied"), n("expired"), n("failed"), n("uncertain"), n("pending")))
        .bind(id).bind(&since).fetch_one(&s.db).await?;
    let new_users: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT user_id) FROM grants WHERE app_id=? AND created_at>=? AND user_id NOT IN (SELECT user_id FROM grants WHERE app_id=? AND created_at<?)")
        .bind(id).bind(&since).bind(id).bind(&since).fetch_one(&s.db).await?;
    let cost = "COALESCE(SUM(CASE WHEN r.status='completed' THEN r.actual_micro END),0)";
    let by_day: Vec<(String, i64, i64, i64)> = sqlx::query_as(&format!("SELECT substr(r.created_at,1,10) d, COUNT(*), SUM(r.status='completed'), {cost} {base} GROUP BY d"))
        .bind(id).bind(&since).fetch_all(&s.db).await?;
    let by_day: HashMap<String, (i64, i64, i64)> = by_day
        .into_iter()
        .map(|(d, a, b, c)| (d, (a, b, c)))
        .collect();
    let daily: Vec<Value> = (0..days)
        .map(|i| {
            let d = (from + chrono::Duration::days(i)).to_string();
            let (req, done, c) = by_day.get(&d).copied().unwrap_or_default();
            json!({"date": d, "requests": req, "completed": done, "cost_usd": usd(c)})
        })
        .collect();
    let models: Vec<(String, String, i64, i64, i64, i64, i64)> = sqlx::query_as(&format!(
        "SELECT r.vendor, r.model, COUNT(*), SUM(r.status='completed'), {cost}, COALESCE(SUM({TOK}),0), COALESCE(SUM({OUT}),0) {base} GROUP BY r.vendor, r.model ORDER BY 5 DESC, 3 DESC"))
        .bind(id).bind(&since).fetch_all(&s.db).await?;
    let countries: Vec<(Option<String>, i64, i64, i64)> = sqlx::query_as(&format!(
        "SELECT g.country, COUNT(DISTINCT r.user_id), COUNT(*), {cost} {base} GROUP BY g.country ORDER BY 3 DESC, 4 DESC"))
        .bind(id).bind(&since).fetch_all(&s.db).await?;
    // when calls happen: requests per UTC weekday (0 = Sunday, SQLite %w) x hour; the console shifts it to the viewer's zone
    let cells: Vec<(i64, i64, i64)> = sqlx::query_as(&format!(
        "SELECT CAST(strftime('%w', r.created_at) AS INTEGER), CAST(substr(r.created_at,12,2) AS INTEGER), COUNT(*) {base} GROUP BY 1, 2"))
        .bind(id).bind(&since).fetch_all(&s.db).await?;
    let mut hours = [[0i64; 24]; 7];
    for (w, h, n) in cells {
        if let (Some(row), true) = (hours.get_mut(w as usize), (0..24).contains(&h)) {
            row[h as usize] = n;
        }
    }
    Ok(Json(json!({
        "app_id": id, "days": days, "from": from.to_string(), "to": today.to_string(),
        "totals": {"requests": t.0, "completed": t.1, "denied": t.2, "expired": t.3, "failed": t.4, "uncertain": t.5, "pending": t.6,
                   "cost_usd": usd(t.7), "estimated_usd": usd(t.8), "unmetered": t.9, "input_tokens": t.10, "output_tokens": t.11,
                   "users": t.12, "new_users": new_users},
        "daily": daily,
        "models": models.into_iter().map(|(v, m, r, c, u, i, o)| json!({"vendor": v, "model": m, "requests": r, "completed": c, "cost_usd": usd(u), "input_tokens": i, "output_tokens": o})).collect::<Vec<_>>(),
        "hours_utc": hours,
        "countries": countries.into_iter().map(|(c, u, r, m)| json!({"country": c, "users": u, "requests": r, "cost_usd": usd(m)})).collect::<Vec<_>>(),
    })))
}

// ---------- user: grants ----------

async fn grant_for_user(s: &AppState, user: &str, id: &str) -> R<GrantRow> {
    sqlx::query_as(&format!(
        "{GRANT_SELECT} WHERE g.id=? AND g.user_id=? AND g.revoked_at IS NULL"
    ))
    .bind(id)
    .bind(user)
    .fetch_optional(&s.db)
    .await?
    .ok_or_else(|| not_found("grant"))
}

pub async fn list_grants(State(s): State<AppState>, h: HeaderMap) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    let rows: Vec<GrantRow> = sqlx::query_as(&format!(
        "{GRANT_SELECT} WHERE g.user_id=? AND g.revoked_at IS NULL ORDER BY g.created_at DESC"
    ))
    .bind(u)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows.iter().map(GrantRow::json).collect()))
}

#[derive(Deserialize)]
pub struct PatchGrant {
    policy: PolicyIn,
}

pub async fn patch_grant(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    J(b): J<PatchGrant>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, ACT).await?;
    let g = grant_for_user(&s, &u, &id).await?;
    let p = b.policy.parse()?;
    if g.coupon_id.is_some() && p.mode != "ask" {
        return Err(sponsored_ask_only());
    }
    set_policy(&mut *s.db.acquire().await?, &id, &p).await?;
    let g = grant_for_user(&s, &u, &id).await?;
    emit(&s, &u, "grant.updated", &g.json());
    Ok(Json(g.json()))
}

pub async fn revoke_grant(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> R<StatusCode> {
    let u = auth_level(&s, &h, READ).await?;
    let mut tx = s.db.begin().await?;
    let r = sqlx::query(
        "UPDATE grants SET revoked_at=? WHERE id=? AND user_id=? AND revoked_at IS NULL",
    )
    .bind(now())
    .bind(&id)
    .bind(&u)
    .execute(&mut *tx)
    .await?;
    if r.rows_affected() != 1 {
        return Err(not_found("grant"));
    }
    sqlx::query(
        "UPDATE requests SET status='denied', decided_at=? WHERE grant_id=? AND status='pending'",
    )
    .bind(now())
    .bind(&id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- user: requests & events ----------

pub async fn list_requests(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    let status = q.get("status").cloned().unwrap_or_default();
    let limit = q
        .get("limit")
        .and_then(|l| l.parse::<i64>().ok())
        .unwrap_or(50)
        .clamp(1, 200);
    let before_id = q.get("before_id");
    let rows: Vec<RequestRow> = if let Some(id) = before_id {
        let created_at: String =
            sqlx::query_scalar("SELECT created_at FROM requests WHERE id=? AND user_id=?")
                .bind(id)
                .bind(&u)
                .fetch_optional(&s.db)
                .await?
                .ok_or_else(|| not_found("request cursor"))?;
        sqlx::query_as(&format!("{REQUEST_SELECT} WHERE r.user_id=? AND (?='' OR r.status=?) AND (r.created_at<? OR (r.created_at=? AND r.id<?)) ORDER BY r.created_at DESC, r.id DESC LIMIT ?"))
            .bind(&u).bind(&status).bind(&status).bind(&created_at).bind(&created_at).bind(id).bind(limit).fetch_all(&s.db).await?
    } else {
        sqlx::query_as(&format!("{REQUEST_SELECT} WHERE r.user_id=? AND (?='' OR r.status=?) ORDER BY r.created_at DESC, r.id DESC LIMIT ?"))
            .bind(&u).bind(&status).bind(&status).bind(limit).fetch_all(&s.db).await?
    };
    Ok(Json(rows.iter().map(|r| r.json(false)).collect()))
}

pub async fn get_request(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    let r = request_row(&s, &id)
        .await?
        .filter(|r| r.user_id == u)
        .ok_or_else(|| not_found("request"))?;
    Ok(Json(r.json(true)))
}

pub async fn events(
    State(s): State<AppState>,
    h: HeaderMap,
) -> R<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>> {
    let u = auth_level(&s, &h, READ).await?;
    let stream = BroadcastStream::new(s.events.subscribe()).filter_map(move |m| match m {
        Ok((user, name, data)) if user == u => Some(Ok(Event::default().event(name).data(data))),
        _ => None, // other users' events, or lagged (client re-syncs via GET)
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
