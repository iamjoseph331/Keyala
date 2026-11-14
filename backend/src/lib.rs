//! MasterKey server: lets users lend an API key to apps, approving calls from their phone.
mod apns;
mod apple;
pub mod pricing;
mod proxy;
mod routes;

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
    Aes256Gcm, Key, Nonce,
};
use axum::{
    body::Bytes,
    extract::{FromRequest, Request},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch, post, put},
    Json, Router,
};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqliteConnectOptions, SqliteConnection, SqlitePool};
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::broadcast;

// ---------- config & state ----------

pub struct Config {
    pub database_url: String,
    pub master_key: [u8; 32],
    pub dev_login: bool,
    pub vendor_mock: bool,
    pub public_url: String,
    /// operator asserts pricing.rs was checked against vendor pricing (MK_PRICES_VERIFIED=1)
    pub prices_verified: bool,
    pub apple_bundle_id: String,
    /// Apple signing keys; overridable (MK_APPLE_JWKS_URL) so tests can serve their own
    pub apple_jwks_url: String,
    pub apns: Option<apns::ApnsCfg>,
}

impl Config {
    pub fn from_env() -> Result<Config, String> {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let mk = env("MK_MASTER_KEY").ok_or("MK_MASTER_KEY is required: generate one with `openssl rand -base64 32`")?;
        let master_key: [u8; 32] = STANDARD
            .decode(mk.trim())
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or("MK_MASTER_KEY must be base64 of exactly 32 bytes (`openssl rand -base64 32`)")?;
        Ok(Config {
            database_url: env("MK_DATABASE_URL").unwrap_or_else(|| "sqlite://masterkey.db?mode=rwc".into()),
            master_key,
            dev_login: env("MK_DEV_LOGIN").as_deref() == Some("1"),
            vendor_mock: env("MK_VENDOR_MOCK").as_deref() == Some("1"),
            prices_verified: env("MK_PRICES_VERIFIED").as_deref() == Some("1"),
            apple_bundle_id: env("MK_APPLE_BUNDLE_ID").unwrap_or_else(|| "app.masterkey.ios".into()),
            apple_jwks_url: env("MK_APPLE_JWKS_URL").unwrap_or_else(|| "https://appleid.apple.com/auth/keys".into()),
            apns: apns::ApnsCfg::from_env(&env),
            public_url: env("MK_PUBLIC_URL").unwrap_or_else(|| "http://localhost:8080".into()).trim_end_matches('/').to_string(),
        })
    }
}

pub struct Inner {
    pub db: SqlitePool,
    pub cfg: Config,
    /// (user_id, event name, json) — each SSE stream filters by user.
    pub events: broadcast::Sender<(String, String, String)>,
    pub http: reqwest::Client,
    pub app_limits: Mutex<HashMap<String, Vec<std::time::Instant>>>,
    pub jwks: apple::JwksCache,
    pub apns_jwt: apns::JwtCache,
}
pub type AppState = Arc<Inner>;

pub use proxy::send_request;

pub async fn build_app(cfg: Config) -> Result<Router, Box<dyn std::error::Error>> {
    Ok(router(build_state(cfg).await?))
}

pub async fn build_state(cfg: Config) -> Result<AppState, Box<dyn std::error::Error>> {
    let opts = SqliteConnectOptions::from_str(&cfg.database_url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(10));
    let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(8).connect_with(opts).await?;
    sqlx::migrate!("./migrations").run(&db).await?;
    // a crash mid-send means we cannot know whether the vendor got it: never retry.
    sqlx::query("UPDATE requests SET status='uncertain' WHERE status='sending'").execute(&db).await?;
    let state: AppState = Arc::new(Inner {
        db,
        cfg,
        events: broadcast::channel(256).0,
        http: reqwest::Client::builder().timeout(Duration::from_secs(120)).redirect(reqwest::redirect::Policy::none()).build()?,
        app_limits: Mutex::new(HashMap::new()),
        jwks: Default::default(),
        apns_jwt: Default::default(),
    });
    if state.cfg.apns.is_none() {
        tracing::info!("APNs not configured (MK_APNS_*): push notifications are disabled");
    }
    // approved but never claimed = never sent, so sending is safe.
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM requests WHERE status='approved'").fetch_all(&state.db).await?;
    for id in ids {
        tokio::spawn(send_request(state.clone(), id));
    }
    tokio::spawn(proxy::sweeper(state.clone()));
    Ok(state)
}

pub fn router(state: AppState) -> Router {
    use routes::*;
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/connect/{code}", get(connect_page))
        .route("/v1/apps", post(create_app))
        .route("/v1/connect", post(create_connect))
        .route("/v1/connect/{code}", get(poll_connect))
        .route("/v1/openai/chat/completions", post(proxy::openai))
        .route("/v1/anthropic/messages", post(proxy::anthropic))
        .route("/v1/anthropic/v1/messages", post(proxy::anthropic)) // official SDK: base_url + /v1/messages
        .route("/v1/requests/{id}", get(proxy::poll_request))
        .route("/v1/auth/dev", post(dev_login))
        .route("/v1/auth/apple", post(apple_login))
        .route("/v1/me/keys", get(list_keys))
        .route("/v1/me/keys/{vendor}", put(put_key).delete(delete_key))
        .route("/v1/me/devices", post(add_device))
        .route("/v1/me/connect/{code}", get(preview_connect))
        .route("/v1/me/connect/{code}/approve", post(approve_connect))
        .route("/v1/me/connect/{code}/deny", post(deny_connect))
        .route("/v1/me/grants", get(list_grants))
        .route("/v1/me/grants/{id}", patch(patch_grant).delete(revoke_grant))
        .route("/v1/me/requests", get(list_requests))
        .route("/v1/me/requests/{id}", get(get_request))
        .route("/v1/me/requests/{id}/approve", post(proxy::approve_request))
        .route("/v1/me/requests/{id}/deny", post(proxy::deny_request))
        .route("/v1/me/events", get(events))
        .with_state(state)
}

// ---------- errors ----------

#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub &'static str, pub String);
pub type R<T> = Result<T, ApiError>;

pub fn api(status: u16, ty: &'static str, msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::from_u16(status).unwrap(), ty, msg.into())
}
pub fn bad(msg: impl Into<String>) -> ApiError {
    api(400, "invalid_request", msg)
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": {"type": self.1, "message": self.2}}))).into_response()
    }
}
impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!("db error: {e}");
        api(500, "internal", "internal error")
    }
}

/// JSON body extractor that fails with our error shape; empty body = `{}`.
pub struct J<T>(pub T);
impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for J<T> {
    type Rejection = ApiError;
    async fn from_request(req: Request, st: &S) -> Result<Self, ApiError> {
        let b = Bytes::from_request(req, st).await.map_err(|_| bad("unreadable body"))?;
        let b: &[u8] = if b.is_empty() { b"{}" } else { &b };
        // generic message on purpose: serde errors can echo values (e.g. api keys).
        serde_json::from_slice(b).map(J).map_err(|_| bad("invalid JSON body (check field names and types)"))
    }
}

// ---------- ids, tokens, time, crypto ----------

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
pub fn after(secs: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::seconds(secs)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
pub fn rand_b64(n: usize) -> String {
    let mut b = vec![0u8; n];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut b);
    URL_SAFE_NO_PAD.encode(b)
}
pub fn new_token(prefix: &str) -> String {
    format!("{prefix}{}", rand_b64(32))
}
pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", rand_b64(12))
}
pub fn hash_token(t: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(t.as_bytes()))
}
pub fn usd(micro: i64) -> f64 {
    micro as f64 / 1_000_000.0
}

pub fn encrypt(master: &[u8; 32], aad: &str, plain: &str) -> (Vec<u8>, Vec<u8>) {
    let c = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(master));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ct = c.encrypt(&nonce, Payload { msg: plain.as_bytes(), aad: aad.as_bytes() }).expect("aes-gcm encrypt");
    (ct, nonce.to_vec())
}
pub fn decrypt(master: &[u8; 32], aad: &str, ct: &[u8], nonce: &[u8]) -> Option<String> {
    if nonce.len() != 12 {
        return None;
    }
    let c = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(master));
    let p = c.decrypt(Nonce::from_slice(nonce), Payload { msg: ct, aad: aad.as_bytes() }).ok()?;
    String::from_utf8(p).ok()
}
/// AAD binds a ciphertext to its (user, vendor) row.
pub fn key_aad(user: &str, vendor: &str) -> String {
    format!("{user}|{vendor}")
}

// ---------- auth ----------

fn bearer<'a>(h: &'a HeaderMap, prefix: &str) -> Option<&'a str> {
    h.get("authorization")?.to_str().ok()?.strip_prefix("Bearer ").map(str::trim).filter(|t| t.starts_with(prefix))
}
fn unauthorized() -> ApiError {
    api(401, "unauthorized", "missing or invalid bearer token")
}
pub async fn auth_session(s: &AppState, h: &HeaderMap) -> R<String> {
    let t = bearer(h, "mk_sess_").ok_or_else(unauthorized)?;
    sqlx::query_scalar("SELECT user_id FROM sessions WHERE token_hash=?").bind(hash_token(t)).fetch_optional(&s.db).await?.ok_or_else(unauthorized)
}
pub async fn auth_app(s: &AppState, h: &HeaderMap) -> R<String> {
    let t = bearer(h, "mk_app_").ok_or_else(unauthorized)?;
    sqlx::query_scalar("SELECT id FROM apps WHERE secret_hash=?").bind(hash_token(t)).fetch_optional(&s.db).await?.ok_or_else(unauthorized)
}
/// `x_api_key`: Anthropic proxy routes also accept the grant token in `x-api-key` (official SDK).
pub async fn auth_grant(s: &AppState, h: &HeaderMap, x_api_key: bool) -> R<GrantRow> {
    let xk = || h.get("x-api-key").and_then(|v| v.to_str().ok()).map(str::trim).filter(|t| t.starts_with("mk_grant_"));
    let t = bearer(h, "mk_grant_").or_else(|| if x_api_key { xk() } else { None }).ok_or_else(unauthorized)?;
    sqlx::query_as(&format!("{GRANT_SELECT} WHERE g.token_hash=? AND g.revoked_at IS NULL")).bind(hash_token(t)).fetch_optional(&s.db).await?.ok_or_else(unauthorized)
}

// ---------- policy ----------

pub struct Policy {
    pub mode: String,
    pub count: Option<i64>,
    pub micro: Option<i64>,
    pub alert: Option<i64>,
    pub expires_at: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct PolicyIn {
    mode: String,
    remaining_count: Option<i64>,
    remaining_usd: Option<f64>,
    alert_usd_per_day: Option<f64>,
    expires_at: Option<String>,
}

fn to_micro(usd: f64) -> R<i64> {
    if usd.is_finite() && (0.0..=10_000.0).contains(&usd) {
        Ok((usd * 1e6).round() as i64)
    } else {
        Err(bad("usd amounts must be between 0 and 10000"))
    }
}

impl PolicyIn {
    pub fn parse(self) -> R<Policy> {
        let mut p = Policy { mode: self.mode, count: None, micro: None, alert: None, expires_at: None };
        match p.mode.as_str() {
            "ask" | "trust" => {}
            "count" => p.count = Some(self.remaining_count.filter(|c| (0..=1_000_000).contains(c)).ok_or_else(|| bad("count policy needs remaining_count between 0 and 1000000"))?),
            "budget" => p.micro = Some(to_micro(self.remaining_usd.ok_or_else(|| bad("budget policy needs remaining_usd"))?)?),
            _ => return Err(bad("policy.mode must be ask, count, budget or trust")),
        }
        p.alert = self.alert_usd_per_day.map(to_micro).transpose()?;
        p.expires_at = match self.expires_at {
            None => None,
            Some(e) => Some(
                chrono::DateTime::parse_from_rfc3339(&e)
                    .map_err(|_| bad("expires_at must be RFC3339"))?
                    .with_timezone(&chrono::Utc)
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            ),
        };
        Ok(p)
    }
}

pub fn policy_json(mode: &str, count: Option<i64>, micro: Option<i64>, alert: Option<i64>, exp: &Option<String>) -> Value {
    let mut m = Map::new();
    m.insert("mode".into(), mode.into());
    if mode == "count" {
        m.insert("remaining_count".into(), count.unwrap_or(0).into());
    }
    if mode == "budget" {
        m.insert("remaining_usd".into(), usd(micro.unwrap_or(0)).into());
    }
    if let Some(a) = alert {
        m.insert("alert_usd_per_day".into(), usd(a).into());
    }
    m.insert("expires_at".into(), exp.clone().into());
    Value::Object(m)
}

pub async fn set_policy(c: &mut SqliteConnection, grant_id: &str, p: &Policy) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE grants SET policy_gen=policy_gen+1, mode=?, remaining_count=?, remaining_micro=?, alert_micro=?, expires_at=? WHERE id=?")
        .bind(&p.mode).bind(p.count).bind(p.micro).bind(p.alert).bind(&p.expires_at).bind(grant_id)
        .execute(c).await?;
    Ok(())
}

// ---------- row types & JSON ----------

pub const GRANT_SELECT: &str = "SELECT g.id, g.user_id, g.app_id, g.vendor, g.mode, g.remaining_count, g.remaining_micro, g.alert_micro, g.expires_at, g.created_at, g.last_used_at, g.spent_micro, g.request_count, a.name AS app_name, a.homepage AS app_homepage, a.icon_url AS app_icon_url FROM grants g JOIN apps a ON a.id=g.app_id";
pub const REQUEST_SELECT: &str = "SELECT r.id, r.grant_id, r.user_id, r.app_id, r.vendor, r.model, r.endpoint, r.status, r.estimated_micro, r.actual_micro, r.reserved_micro, r.reserved_count, r.policy_gen, r.payload, r.response, r.http_status, r.created_at, r.decided_at, r.completed_at, a.name AS app_name, a.homepage AS app_homepage, a.icon_url AS app_icon_url FROM requests r JOIN apps a ON a.id=r.app_id";

pub fn app_json(id: &str, name: &str, homepage: &Option<String>, icon: &Option<String>) -> Value {
    json!({"id": id, "name": name, "homepage": homepage, "icon_url": icon})
}

#[derive(sqlx::FromRow)]
pub struct GrantRow {
    pub id: String,
    pub user_id: String,
    pub app_id: String,
    pub vendor: String,
    mode: String,
    remaining_count: Option<i64>,
    remaining_micro: Option<i64>,
    alert_micro: Option<i64>,
    expires_at: Option<String>,
    created_at: String,
    last_used_at: Option<String>,
    spent_micro: i64,
    request_count: i64,
    app_name: String,
    app_homepage: Option<String>,
    app_icon_url: Option<String>,
}
impl GrantRow {
    pub fn app_name_str(&self) -> &str {
        &self.app_name
    }
    pub fn json(&self) -> Value {
        json!({"id": self.id, "app": app_json(&self.app_id, &self.app_name, &self.app_homepage, &self.app_icon_url),
               "vendor": self.vendor,
               "policy": policy_json(&self.mode, self.remaining_count, self.remaining_micro, self.alert_micro, &self.expires_at),
               "created_at": self.created_at, "last_used_at": self.last_used_at,
               "spent_usd": usd(self.spent_micro), "request_count": self.request_count})
    }
}

#[derive(sqlx::FromRow)]
pub struct RequestRow {
    pub id: String,
    pub grant_id: String,
    pub user_id: String,
    pub app_id: String,
    pub vendor: String,
    pub model: String,
    pub endpoint: String,
    pub status: String,
    pub estimated_micro: i64,
    pub actual_micro: Option<i64>,
    pub reserved_micro: i64,
    pub reserved_count: i64,
    pub policy_gen: i64,
    pub payload: String,
    pub response: Option<String>,
    pub http_status: Option<i64>,
    created_at: String,
    decided_at: Option<String>,
    completed_at: Option<String>,
    app_name: String,
    app_homepage: Option<String>,
    app_icon_url: Option<String>,
}
impl RequestRow {
    pub fn app_name(&self) -> &str {
        &self.app_name
    }
    pub fn json(&self, detail: bool) -> Value {
        let mut v = json!({"id": self.id, "grant_id": self.grant_id,
            "app": app_json(&self.app_id, &self.app_name, &self.app_homepage, &self.app_icon_url),
            "vendor": self.vendor, "model": self.model, "endpoint": self.endpoint, "status": self.status,
            "estimated_usd": usd(self.estimated_micro), "actual_usd": self.actual_micro.map(usd),
            "created_at": self.created_at, "decided_at": self.decided_at, "completed_at": self.completed_at});
        if detail {
            fn parse(s: &str) -> Value {
                serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.into()))
            }
            v["payload"] = parse(&self.payload);
            v["response"] = self.response.as_deref().map(parse).unwrap_or(Value::Null);
        }
        v
    }
}

pub async fn request_row(s: &AppState, id: &str) -> Result<Option<RequestRow>, sqlx::Error> {
    sqlx::query_as(&format!("{REQUEST_SELECT} WHERE r.id=?")).bind(id).fetch_optional(&s.db).await
}

// ---------- events ----------

pub fn emit(s: &AppState, user: &str, event: &str, data: &Value) {
    let _ = s.events.send((user.to_string(), event.to_string(), data.to_string()));
}
pub async fn emit_request(s: &AppState, event: &str, id: &str) {
    if let Ok(Some(r)) = request_row(s, id).await {
        emit(s, &r.user_id, event, &r.json(false));
    }
}
pub async fn emit_grant(s: &AppState, id: &str) {
    if let Ok(Some(g)) = sqlx::query_as::<_, GrantRow>(&format!("{GRANT_SELECT} WHERE g.id=?")).bind(id).fetch_optional(&s.db).await {
        emit(s, &g.user_id, "grant.updated", &g.json());
    }
}
