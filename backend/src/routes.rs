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

const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // no 0/O/1/I
fn vendor_ok(v: &str) -> R<()> {
    if matches!(v, "openai" | "anthropic") { Ok(()) } else { Err(bad("vendor must be openai or anthropic")) }
}
fn not_found(what: &str) -> ApiError {
    api(404, "not_found", format!("{what} not found"))
}

// ---------- developer: apps & connect ----------

/// ponytail: in-memory per-IP sliding window, resets on restart and is per-instance; move to SQLite/edge if abused.
/// Behind a proxy (Render) the last X-Forwarded-For entry is the one the proxy added.
fn rate_limit(s: &AppState, peer: SocketAddr, headers: &HeaderMap, bucket: &str, max: usize, window: u64, what: &str) -> R<()> {
    let ip = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()).and_then(|v| v.rsplit(',').next()).map(|v| v.trim().to_string()).unwrap_or_else(|| peer.ip().to_string());
    let mut l = s.app_limits.lock().unwrap();
    let hits = l.entry(format!("{bucket}:{ip}")).or_default();
    hits.retain(|t| t.elapsed() < Duration::from_secs(window));
    if hits.len() >= max {
        return Err(api(429, "rate_limited", format!("too many {what} from this address; try again later")));
    }
    hits.push(Instant::now());
    Ok(())
}

#[derive(Deserialize)]
pub struct NewApp {
    name: String,
    homepage: Option<String>,
    icon_url: Option<String>,
    /// browser (public) client: web origins allowed to start a connect with just the app id
    allowed_origins: Option<Vec<String>>,
}

pub async fn create_app(State(s): State<AppState>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, J(b): J<NewApp>) -> R<Json<Value>> {
    rate_limit(&s, peer, &headers, "apps", 10, 3600, "apps registered")?;
    let name = b.name.trim();
    if name.is_empty() || name.len() > 100 {
        return Err(bad("name is required (max 100 chars)"));
    }
    if b.allowed_origins.as_deref().is_some_and(|o| !origins_ok(o)) {
        return Err(bad("allowed_origins: 1-10 origins like https://example.com (http only for localhost), no path"));
    }
    let origins = b.allowed_origins.map(|o| serde_json::to_string(&o).unwrap_or_default());
    let (id, secret) = (new_id("app"), new_token("mk_app_"));
    sqlx::query("INSERT INTO apps (id,name,homepage,icon_url,secret_hash,allowed_origins,created_at) VALUES (?,?,?,?,?,?,?)")
        .bind(&id).bind(name).bind(&b.homepage).bind(&b.icon_url).bind(hash_token(&secret)).bind(&origins).bind(now())
        .execute(&s.db).await?;
    Ok(Json(json!({"app_id": id, "app_secret": secret})))
}

#[derive(Deserialize)]
pub struct NewConnect {
    vendor: String,
    suggested_policy: Option<PolicyIn>,
    /// browser clients only: identifies the app instead of the app secret
    app_id: Option<String>,
}

pub async fn create_connect(State(s): State<AppState>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, J(b): J<NewConnect>) -> R<Json<Value>> {
    // Server apps authenticate with the app secret. Browser apps send their app id, and the
    // browser's Origin must be one the app registered. That binding is browser-enforced only (a
    // non-browser client can forge Origin), so the phone shows the origin and every call still
    // follows the policy the user approves. The grant goes to whoever holds `connect_secret`.
    let (app_id, connect_secret, origin) = if headers.contains_key("authorization") {
        (auth_app(&s, &headers).await?, None, None)
    } else {
        let app_id = b.app_id.as_deref().ok_or_else(|| api(401, "unauthorized", "send the app secret, or app_id from an allowed browser origin"))?;
        let origin = headers.get("origin").and_then(|v| v.to_str().ok()).unwrap_or("");
        let allowed: Option<String> = sqlx::query_scalar("SELECT allowed_origins FROM apps WHERE id=?").bind(app_id).fetch_optional(&s.db).await?.flatten();
        let allowed: Vec<String> = allowed.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
        if !allowed.iter().any(|o| o == origin) {
            return Err(api(403, "origin_not_allowed", "this origin is not allowed for this app"));
        }
        rate_limit(&s, peer, &headers, "connect", 20, 600, "connect codes")?;
        (app_id.to_string(), Some(new_token("mk_cs_")), Some(origin.to_string()))
    };
    vendor_ok(&b.vendor)?;
    let p = b.suggested_policy.unwrap_or(PolicyIn { mode: "ask".into(), ..Default::default() }).parse()?;
    let sp = policy_json(&p.mode, p.count, p.micro, p.alert, &p.expires_at).to_string();
    let expires = after(600);
    for _ in 0..5 {
        let code = format!("MK-{}", (0..4).map(|_| CODE_ALPHABET[rand::Rng::gen_range(&mut rand::thread_rng(), 0..CODE_ALPHABET.len())] as char).collect::<String>());
        let r = sqlx::query("INSERT OR IGNORE INTO connect_codes (code,app_id,vendor,suggested_policy,created_at,expires_at,secret_hash,origin) VALUES (?,?,?,?,?,?,?,?)")
            .bind(&code).bind(&app_id).bind(&b.vendor).bind(&sp).bind(now()).bind(&expires).bind(connect_secret.as_deref().map(hash_token)).bind(&origin)
            .execute(&s.db).await?;
        if r.rows_affected() == 1 {
            let mut v = json!({"connect_code": code, "connect_url": format!("{}/connect/{code}", s.cfg.public_url), "expires_at": expires});
            if let Some(cs) = connect_secret {
                v["connect_secret"] = cs.into();
            }
            return Ok(Json(v));
        }
    }
    Err(api(500, "internal", "could not allocate a connect code"))
}

pub async fn poll_connect(State(s): State<AppState>, headers: HeaderMap, Path(code): Path<String>) -> R<Json<Value>> {
    let row: Option<(String, Option<String>, String)> = if let Some(cs) = bearer(&headers, "mk_cs_") {
        sqlx::query_as("SELECT status, grant_id, expires_at FROM connect_codes WHERE code=? AND secret_hash=?").bind(&code).bind(hash_token(cs)).fetch_optional(&s.db).await?
    } else {
        let app_id = auth_app(&s, &headers).await?;
        sqlx::query_as("SELECT status, grant_id, expires_at FROM connect_codes WHERE code=? AND app_id=?").bind(&code).bind(&app_id).fetch_optional(&s.db).await?
    };
    let (status, grant_id, exp) = row.ok_or_else(|| not_found("connect code"))?;
    if status == "pending" {
        let st = if exp.as_str() <= now().as_str() { "expired" } else { "pending" };
        return Ok(Json(json!({"status": st})));
    }
    if let (true, Some(gid)) = (status == "approved", grant_id) {
        // The token is generated on first poll after approval and shown exactly once
        // (only its hash is stored). Lost response => run a new connect flow.
        let tok = new_token("mk_grant_");
        let r = sqlx::query("UPDATE grants SET token_hash=? WHERE id=? AND token_hash IS NULL").bind(hash_token(&tok)).bind(&gid).execute(&s.db).await?;
        if r.rows_affected() == 1 {
            return Ok(Json(json!({"status": "approved", "grant_token": tok})));
        }
    }
    Ok(Json(json!({"status": status})))
}

pub async fn connect_page(Path(code): Path<String>) -> Result<Html<String>, ApiError> {
    let ok = code.len() == 7 && code.starts_with("MK-") && code[3..].bytes().all(|c| CODE_ALPHABET.contains(&c));
    if !ok {
        return Err(not_found("connect code"));
    }
    Ok(Html(format!(
        "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'><title>MasterKey</title>\
         <body style='font-family:system-ui;text-align:center;margin:4rem 1rem'><h1>Connect with MasterKey</h1>\
         <p>Code <b>{code}</b></p><p><a href='masterkey://connect/{code}' style='font-size:1.3rem'>Open in the MasterKey app</a></p>"
    )))
}

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
    if !email.contains('@') || email.len() > 200 {
        return Err(bad("a valid email is required"));
    }
    sqlx::query("INSERT OR IGNORE INTO users (id,email,created_at) VALUES (?,?,?)").bind(new_id("usr")).bind(&email).bind(now()).execute(&s.db).await?;
    let uid: String = sqlx::query_scalar("SELECT id FROM users WHERE email=?").bind(&email).fetch_one(&s.db).await?;
    new_session(&s, &uid, &email).await
}

async fn new_session(s: &AppState, uid: &str, email: &str) -> R<Json<Value>> {
    let tok = new_token("mk_sess_");
    sqlx::query("INSERT INTO sessions (token_hash,user_id,created_at) VALUES (?,?,?)").bind(hash_token(&tok)).bind(uid).bind(now()).execute(&s.db).await?;
    Ok(Json(json!({"session_token": tok, "user": {"id": uid, "email": email}})))
}

#[derive(Deserialize)]
pub struct AppleLogin {
    id_token: String,
}

/// Identity is Apple's `sub`, never the email (emails can be relays, changed, or collide).
pub async fn apple_login(State(s): State<AppState>, J(b): J<AppleLogin>) -> R<Json<Value>> {
    let c = crate::apple::verify(&s, &b.id_token).await?;
    let existing: Option<(String, String)> = sqlx::query_as("SELECT id, email FROM users WHERE apple_sub=?").bind(&c.sub).fetch_optional(&s.db).await?;
    if let Some((uid, email)) = existing {
        return new_session(&s, &uid, &email).await;
    }
    // never attach to an existing account by email: if it is taken, use a placeholder.
    let mut email = c.email.map(|e| e.trim().to_lowercase()).filter(|e| e.contains('@') && e.len() <= 200);
    if let Some(e) = &email {
        if sqlx::query_scalar::<_, i64>("SELECT 1 FROM users WHERE email=?").bind(e).fetch_optional(&s.db).await?.is_some() {
            email = None;
        }
    }
    let email = email.unwrap_or_else(|| format!("{}@apple.invalid", c.sub));
    let uid = new_id("usr");
    sqlx::query("INSERT OR IGNORE INTO users (id,email,created_at,apple_sub) VALUES (?,?,?,?)").bind(&uid).bind(&email).bind(now()).bind(&c.sub).execute(&s.db).await?;
    // a concurrent first login may have won the insert: always read back by sub
    let (uid, email): (String, String) = sqlx::query_as("SELECT id, email FROM users WHERE apple_sub=?").bind(&c.sub).fetch_one(&s.db).await?;
    new_session(&s, &uid, &email).await
}

pub async fn list_keys(State(s): State<AppState>, h: HeaderMap) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    let rows: Vec<(String, String, String)> = sqlx::query_as("SELECT vendor,last4,created_at FROM api_keys WHERE user_id=? ORDER BY vendor").bind(u).fetch_all(&s.db).await?;
    Ok(Json(rows.into_iter().map(|(v, l, c)| json!({"vendor": v, "last4": l, "created_at": c})).collect()))
}

#[derive(Deserialize)]
pub struct PutKey {
    api_key: String,
}

pub async fn put_key(State(s): State<AppState>, h: HeaderMap, Path(vendor): Path<String>, J(b): J<PutKey>) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    vendor_ok(&vendor)?;
    let key = b.api_key.trim();
    if key.len() < 8 || key.len() > 512 || key.chars().any(char::is_whitespace) {
        return Err(bad("api_key looks invalid"));
    }
    let last4: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    let (ct, nonce) = encrypt(&s.cfg.master_key, &key_aad(&u, &vendor), key);
    let created = now();
    sqlx::query("INSERT INTO api_keys (user_id,vendor,ciphertext,nonce,last4,created_at) VALUES (?,?,?,?,?,?) ON CONFLICT(user_id,vendor) DO UPDATE SET ciphertext=excluded.ciphertext, nonce=excluded.nonce, last4=excluded.last4, created_at=excluded.created_at")
        .bind(&u).bind(&vendor).bind(ct).bind(nonce).bind(&last4).bind(&created).execute(&s.db).await?;
    Ok(Json(json!({"vendor": vendor, "last4": last4, "created_at": created})))
}

pub async fn delete_key(State(s): State<AppState>, h: HeaderMap, Path(vendor): Path<String>) -> R<StatusCode> {
    let u = auth_session(&s, &h).await?;
    sqlx::query("DELETE FROM api_keys WHERE user_id=? AND vendor=?").bind(u).bind(vendor).execute(&s.db).await?;
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
    sqlx::query("INSERT OR IGNORE INTO devices (user_id,apns_token,created_at) VALUES (?,?,?)").bind(u).bind(b.apns_token).bind(now()).execute(&s.db).await?;
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
    app_name: String,
    app_homepage: Option<String>,
    app_icon_url: Option<String>,
}

async fn pending_connect(s: &AppState, code: &str) -> R<ConnectRow> {
    let c: ConnectRow = sqlx::query_as("SELECT c.app_id, c.vendor, c.suggested_policy, c.expires_at, c.origin, a.name AS app_name, a.homepage AS app_homepage, a.icon_url AS app_icon_url FROM connect_codes c JOIN apps a ON a.id=c.app_id WHERE c.code=? AND c.status='pending'")
        .bind(code).fetch_optional(&s.db).await?.ok_or_else(|| not_found("connect code"))?;
    if c.expires_at.as_str() <= now().as_str() {
        return Err(api(410, "expired", "connect code expired"));
    }
    Ok(c)
}

pub async fn preview_connect(State(s): State<AppState>, h: HeaderMap, Path(code): Path<String>) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    let c = pending_connect(&s, &code).await?;
    let sp: Value = serde_json::from_str(&c.suggested_policy).unwrap_or(Value::Null);
    let has_prior_grant: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM grants WHERE user_id=? AND app_id=?)")
        .bind(&u).bind(&c.app_id).fetch_one(&s.db).await?;
    Ok(Json(json!({"app": app_json(&c.app_id, &c.app_name, &c.app_homepage, &c.app_icon_url), "vendor": c.vendor, "suggested_policy": sp, "expires_at": c.expires_at, "origin": c.origin, "requires_ask": !has_prior_grant})))
}

#[derive(Deserialize)]
pub struct ApproveConnect {
    policy: Option<PolicyIn>,
}

pub async fn approve_connect(State(s): State<AppState>, h: HeaderMap, Path(code): Path<String>, J(b): J<ApproveConnect>) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    let c = pending_connect(&s, &code).await?;
    let p = match b.policy {
        Some(p) => p.parse()?,
        None => serde_json::from_str::<PolicyIn>(&c.suggested_policy).map_err(|_| bad("bad suggested policy"))?.parse()?,
    };
    let has_key: Option<i64> = sqlx::query_scalar("SELECT 1 FROM api_keys WHERE user_id=? AND vendor=?").bind(&u).bind(&c.vendor).fetch_optional(&s.db).await?;
    if has_key.is_none() {
        return Err(api(409, "no_key", format!("add your {} API key before approving", c.vendor)));
    }
    let gid = new_id("grt");
    let mut tx = s.db.begin().await?;
    // First statement is the write (single-use guard), serializing concurrent approvals
    // before checking history. If the policy is too broad, dropping this transaction rolls
    // the status change back so the user can retry with ask.
    let r = sqlx::query("UPDATE connect_codes SET status='approved', grant_id=? WHERE code=? AND status='pending' AND expires_at>?").bind(&gid).bind(&code).bind(now()).execute(&mut *tx).await?;
    if r.rows_affected() != 1 {
        return Err(api(409, "already_used", "connect code already used or expired"));
    }
    let has_prior_grant: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM grants WHERE user_id=? AND app_id=?)")
        .bind(&u).bind(&c.app_id).fetch_one(&mut *tx).await?;
    if !has_prior_grant && p.mode != "ask" {
        return Err(api(400, "policy_not_allowed_yet", "approve calls one at a time first; widen the policy later from Grants"));
    }
    sqlx::query("INSERT INTO grants (id,user_id,app_id,vendor,mode,created_at) VALUES (?,?,?,?,'ask',?)").bind(&gid).bind(&u).bind(&c.app_id).bind(&c.vendor).bind(now()).execute(&mut *tx).await?;
    set_policy(&mut tx, &gid, &p).await?;
    tx.commit().await?;
    let g = grant_for_user(&s, &u, &gid).await?;
    emit(&s, &u, "grant.updated", &g.json());
    Ok(Json(g.json()))
}

pub async fn deny_connect(State(s): State<AppState>, h: HeaderMap, Path(code): Path<String>) -> R<StatusCode> {
    auth_session(&s, &h).await?;
    let r = sqlx::query("UPDATE connect_codes SET status='denied' WHERE code=? AND status='pending'").bind(&code).execute(&s.db).await?;
    if r.rows_affected() != 1 {
        return Err(not_found("connect code"));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------- user: grants ----------

async fn grant_for_user(s: &AppState, user: &str, id: &str) -> R<GrantRow> {
    sqlx::query_as(&format!("{GRANT_SELECT} WHERE g.id=? AND g.user_id=? AND g.revoked_at IS NULL")).bind(id).bind(user).fetch_optional(&s.db).await?.ok_or_else(|| not_found("grant"))
}

pub async fn list_grants(State(s): State<AppState>, h: HeaderMap) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    let rows: Vec<GrantRow> = sqlx::query_as(&format!("{GRANT_SELECT} WHERE g.user_id=? AND g.revoked_at IS NULL ORDER BY g.created_at DESC")).bind(u).fetch_all(&s.db).await?;
    Ok(Json(rows.iter().map(GrantRow::json).collect()))
}

#[derive(Deserialize)]
pub struct PatchGrant {
    policy: PolicyIn,
}

pub async fn patch_grant(State(s): State<AppState>, h: HeaderMap, Path(id): Path<String>, J(b): J<PatchGrant>) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    grant_for_user(&s, &u, &id).await?;
    let p = b.policy.parse()?;
    set_policy(&mut *s.db.acquire().await?, &id, &p).await?;
    let g = grant_for_user(&s, &u, &id).await?;
    emit(&s, &u, "grant.updated", &g.json());
    Ok(Json(g.json()))
}

pub async fn revoke_grant(State(s): State<AppState>, h: HeaderMap, Path(id): Path<String>) -> R<StatusCode> {
    let u = auth_session(&s, &h).await?;
    let mut tx = s.db.begin().await?;
    let r = sqlx::query("UPDATE grants SET revoked_at=? WHERE id=? AND user_id=? AND revoked_at IS NULL").bind(now()).bind(&id).bind(&u).execute(&mut *tx).await?;
    if r.rows_affected() != 1 {
        return Err(not_found("grant"));
    }
    sqlx::query("UPDATE requests SET status='denied', decided_at=? WHERE grant_id=? AND status='pending'").bind(now()).bind(&id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- user: requests & events ----------

pub async fn list_requests(State(s): State<AppState>, h: HeaderMap, Query(q): Query<HashMap<String, String>>) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    let status = q.get("status").cloned().unwrap_or_default();
    let limit = q.get("limit").and_then(|l| l.parse::<i64>().ok()).unwrap_or(50).clamp(1, 200);
    let before_id = q.get("before_id");
    let rows: Vec<RequestRow> = if let Some(id) = before_id {
        let created_at: String = sqlx::query_scalar("SELECT created_at FROM requests WHERE id=? AND user_id=?")
            .bind(id).bind(&u).fetch_optional(&s.db).await?.ok_or_else(|| not_found("request cursor"))?;
        sqlx::query_as(&format!("{REQUEST_SELECT} WHERE r.user_id=? AND (?='' OR r.status=?) AND (r.created_at<? OR (r.created_at=? AND r.id<?)) ORDER BY r.created_at DESC, r.id DESC LIMIT ?"))
            .bind(&u).bind(&status).bind(&status).bind(&created_at).bind(&created_at).bind(id).bind(limit).fetch_all(&s.db).await?
    } else {
        sqlx::query_as(&format!("{REQUEST_SELECT} WHERE r.user_id=? AND (?='' OR r.status=?) ORDER BY r.created_at DESC, r.id DESC LIMIT ?"))
            .bind(&u).bind(&status).bind(&status).bind(limit).fetch_all(&s.db).await?
    };
    Ok(Json(rows.iter().map(|r| r.json(false)).collect()))
}

pub async fn get_request(State(s): State<AppState>, h: HeaderMap, Path(id): Path<String>) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    let r = request_row(&s, &id).await?.filter(|r| r.user_id == u).ok_or_else(|| not_found("request"))?;
    Ok(Json(r.json(true)))
}

pub async fn events(State(s): State<AppState>, h: HeaderMap) -> R<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>> {
    let u = auth_session(&s, &h).await?;
    let stream = BroadcastStream::new(s.events.subscribe()).filter_map(move |m| match m {
        Ok((user, name, data)) if user == u => Some(Ok(Event::default().event(name).data(data))),
        _ => None, // other users' events, or lagged (client re-syncs via GET)
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
