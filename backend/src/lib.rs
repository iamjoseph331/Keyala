//! Keyala server: lets users lend an API key to apps, approving calls from their phone.
mod apns;
mod apple;
mod enroll;
mod oauth;
mod ops;
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
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, patch, post, put},
    Json, Router,
};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
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
    /// operator asserts pricing.rs was checked against vendor pricing (KEYALA_PRICES_VERIFIED=1)
    pub prices_verified: bool,
    pub apple_bundle_id: String,
    /// Apple developer team, for the universal-link association file (KEYALA_APPLE_TEAM_ID)
    pub apple_team_id: String,
    /// Apple signing keys; overridable (KEYALA_APPLE_JWKS_URL) so tests can serve their own
    pub apple_jwks_url: String,
    pub apns: Option<apns::ApnsCfg>,
    /// Browser apps declared by the operator (KEYALA_PUBLIC_APPS), upserted on every start so they
    /// survive a database reset (Render free plan). Lets a static page ship a fixed app id.
    pub public_apps: Vec<PublicApp>,
    /// Header a trusted edge in front of the server sets to the client's country (KEYALA_COUNTRY_HEADER,
    /// e.g. cf-ipcountry). Unset = no country in reports; clients could otherwise forge it.
    pub country_header: Option<String>,
    /// One operator-provisioned password account (KEYALA_PASSWORD_LOGIN=`username:sha256hex(password)`),
    /// a fallback while Sign in with Apple is unavailable. Unset = `POST /v1/auth/password` is 404.
    pub password_login: Option<(String, [u8; 32])>,
    /// Sign in with Apple key for revoking tokens on account deletion: (key id, .p8 PEM). Optional.
    pub apple_signin_key: Option<(String, String)>,
    /// Apple's auth host, overridable so tests can serve a local one
    pub apple_auth_url: String,
    /// Operator switch (KEYALA_PAUSE_UPSTREAM=1, or a `PAUSE_UPSTREAM` file beside the database): no new upstream calls.
    pub pause_upstream: bool,
    /// In-process backups: directory (KEYALA_BACKUP_DIR), how many to keep, and how often.
    pub backup_dir: Option<std::path::PathBuf>,
    pub backup_keep: usize,
    pub backup_interval: Duration,
}

#[derive(Deserialize, Clone)]
pub struct PublicApp {
    pub id: String,
    pub name: String,
    pub homepage: Option<String>,
    pub allowed_origins: Vec<String>,
}

/// A web origin as browsers send it: `https://host[:port]`, or `http://` for localhost only. No path.
pub fn origin_ok(o: &str) -> bool {
    let (https, rest) = match (o.strip_prefix("https://"), o.strip_prefix("http://")) {
        (Some(r), _) => (true, r),
        (_, Some(r)) => (false, r),
        _ => return false,
    };
    let host = match rest.rsplit_once(':') {
        Some((h, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => h,
        _ => rest,
    };
    let chars = rest
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b".-:[]".contains(&b));
    o.len() <= 200
        && !host.is_empty()
        && chars
        && (https || matches!(host, "localhost" | "127.0.0.1" | "[::1]"))
}
pub fn origins_ok(v: &[String]) -> bool {
    !v.is_empty() && v.len() <= 10 && v.iter().all(|o| origin_ok(o))
}

impl Config {
    pub fn from_env() -> Result<Config, String> {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let mk = env("KEYALA_MASTER_KEY")
            .or_else(|| env("MK_MASTER_KEY"))
            .ok_or("KEYALA_MASTER_KEY is required: generate one with `openssl rand -base64 32`")?;
        let master_key: [u8; 32] = STANDARD
            .decode(mk.trim())
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or(
                "KEYALA_MASTER_KEY must be base64 of exactly 32 bytes (`openssl rand -base64 32`)",
            )?;
        Ok(Config {
            database_url: env("KEYALA_DATABASE_URL")
                .unwrap_or_else(|| "sqlite://keyala.db?mode=rwc".into()),
            master_key,
            dev_login: env("KEYALA_DEV_LOGIN").as_deref() == Some("1"),
            vendor_mock: env("KEYALA_VENDOR_MOCK").as_deref() == Some("1"),
            prices_verified: env("KEYALA_PRICES_VERIFIED").as_deref() == Some("1"),
            apple_bundle_id: env("KEYALA_APPLE_BUNDLE_ID")
                .unwrap_or_else(|| "app.sorevan.masterkey".into()),
            apple_team_id: env("KEYALA_APPLE_TEAM_ID").unwrap_or_else(|| "83APHP8F76".into()),
            apple_jwks_url: env("KEYALA_APPLE_JWKS_URL")
                .unwrap_or_else(|| "https://appleid.apple.com/auth/keys".into()),
            apns: apns::ApnsCfg::from_env(&env),
            apple_signin_key: match (
                env("KEYALA_APPLE_SIGNIN_KEY_ID"),
                env("KEYALA_APPLE_SIGNIN_KEY_P8"),
            ) {
                (Some(k), Some(p)) => Some((k, p.replace("\\n", "\n"))),
                _ => None,
            },
            apple_auth_url: env("KEYALA_APPLE_AUTH_URL")
                .unwrap_or_else(|| "https://appleid.apple.com".into())
                .trim_end_matches('/')
                .to_string(),
            pause_upstream: env("KEYALA_PAUSE_UPSTREAM").as_deref() == Some("1"),
            backup_dir: env("KEYALA_BACKUP_DIR").map(Into::into),
            backup_keep: env("KEYALA_BACKUP_KEEP")
                .and_then(|v| v.parse().ok())
                .filter(|k| *k >= 1)
                .unwrap_or(7),
            backup_interval: Duration::from_secs(
                env("KEYALA_BACKUP_INTERVAL_SECS")
                    .and_then(|v| v.parse().ok())
                    .filter(|s| *s >= 1)
                    .unwrap_or(86_400),
            ),
            country_header: env("KEYALA_COUNTRY_HEADER").map(|h| h.to_ascii_lowercase()),
            password_login: match env("KEYALA_PASSWORD_LOGIN") {
                None => None,
                Some(v) => {
                    let err = "KEYALA_PASSWORD_LOGIN must be `username:<64 hex chars of sha256(password)>` (username 3-40 of a-z 0-9 . _ -)";
                    let (u, h) = v.trim().split_once(':').ok_or(err)?;
                    let hash: Vec<u8> = (0..h.len())
                        .step_by(2)
                        .map(|i| h.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok()))
                        .collect::<Option<_>>()
                        .ok_or(err)?;
                    let hash: [u8; 32] = hash.try_into().map_err(|_| err)?;
                    if !username_ok(u) {
                        return Err(err.into());
                    }
                    Some((u.to_string(), hash))
                }
            },
            public_apps: match env("KEYALA_PUBLIC_APPS") {
                None => vec![],
                Some(j) => {
                    let apps: Vec<PublicApp> = serde_json::from_str(&j).map_err(|_| "KEYALA_PUBLIC_APPS must be a JSON array of {id, name, homepage?, allowed_origins}")?;
                    for a in &apps {
                        let id_ok = a.id.starts_with("app_")
                            && a.id.len() <= 64
                            && a.id
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
                        if !id_ok
                            || a.name.trim().is_empty()
                            || a.name.len() > 100
                            || !origins_ok(&a.allowed_origins)
                        {
                            return Err(format!("KEYALA_PUBLIC_APPS: invalid app '{}' (id app_[A-Za-z0-9_-], name 1-100 chars, 1-10 origins like https://example.com)", a.id));
                        }
                    }
                    apps
                }
            },
            public_url: env("KEYALA_PUBLIC_URL")
                .or_else(|| env("RENDER_EXTERNAL_URL"))
                .unwrap_or_else(|| "http://localhost:8080".into())
                .trim_end_matches('/')
                .to_string(),
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

pub use proxy::{purge_expired_content, send_request, RETENTION_DAYS};

pub async fn build_app(cfg: Config) -> Result<Router, Box<dyn std::error::Error>> {
    Ok(router(build_state(cfg).await?))
}

pub async fn build_state(cfg: Config) -> Result<AppState, Box<dyn std::error::Error>> {
    let opts = SqliteConnectOptions::from_str(&cfg.database_url)?
        .create_if_missing(true)
        .foreign_keys(true)
        // deleted rows (keys, prompts) are zeroed in the file, not left in free pages
        .pragma("secure_delete", "on")
        .busy_timeout(Duration::from_secs(10));
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations").run(&db).await?;
    // a crash mid-send means we cannot know whether the vendor got it: never retry.
    sqlx::query("UPDATE requests SET status='uncertain' WHERE status='sending'")
        .execute(&db)
        .await?;
    // a crash mid-exchange: OpenRouter codes are single use, so never retry; say so instead
    sqlx::query("UPDATE oauth_flows SET status='failed', reason='interrupted', verifier=NULL WHERE status='exchanging'").execute(&db).await?;
    let state: AppState = Arc::new(Inner {
        db,
        cfg,
        events: broadcast::channel(256).0,
        http: reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .redirect(reqwest::redirect::Policy::none())
            .build()?,
        app_limits: Mutex::new(HashMap::new()),
        jwks: Default::default(),
        apns_jwt: Default::default(),
    });
    // retention first (nothing to encrypt that is due anyway), then encrypt rows written before 0013
    proxy::purge_expired_content(&state).await?;
    if encrypt_legacy_requests(&state).await? > 0 {
        // one-time: rewrite the file so the old plaintext pages are gone too
        sqlx::query("VACUUM").execute(&state.db).await?;
    }
    for a in &state.cfg.public_apps {
        // the secret is random and discarded: declared apps are browser-only
        sqlx::query("INSERT INTO apps (id,name,homepage,secret_hash,allowed_origins,created_at) VALUES (?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name, homepage=excluded.homepage, allowed_origins=excluded.allowed_origins")
            .bind(&a.id).bind(a.name.trim()).bind(&a.homepage).bind(hash_token(&new_token("keyala_app_"))).bind(serde_json::to_string(&a.allowed_origins)?).bind(now())
            .execute(&state.db).await?;
    }
    if state.cfg.apns.is_none() {
        tracing::info!("APNs not configured (KEYALA_APNS_*): push notifications are disabled");
    }
    // approved but never claimed = never sent, so sending is safe.
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM requests WHERE status='approved'")
        .fetch_all(&state.db)
        .await?;
    for id in ids {
        tokio::spawn(send_request(state.clone(), id));
    }
    tokio::spawn(proxy::sweeper(state.clone()));
    if state.cfg.backup_dir.is_some() {
        tokio::spawn(ops::backup_loop(state.clone()));
    }
    Ok(state)
}

/// CORS for developer endpoints only. They authenticate with bearer tokens, never cookies, so
/// allowing any origin exposes nothing a token holder couldn't already do; it lets static pages
/// (public clients) call Keyala directly. User endpoints (/v1/me) stay same-origin.
async fn cors(req: Request, next: Next) -> Response {
    let mut r = if req.method() == Method::OPTIONS {
        let mut r = StatusCode::NO_CONTENT.into_response();
        let h = r.headers_mut();
        h.insert(
            "access-control-allow-methods",
            HeaderValue::from_static("GET, POST, OPTIONS"),
        );
        h.insert("access-control-max-age", HeaderValue::from_static("600"));
        if let Some(v) = req.headers().get("access-control-request-headers") {
            h.insert("access-control-allow-headers", v.clone());
        }
        r
    } else {
        next.run(req).await
    };
    let h = r.headers_mut();
    h.insert("access-control-allow-origin", HeaderValue::from_static("*"));
    h.insert(
        "access-control-expose-headers",
        HeaderValue::from_static("x-keyala-request-id"),
    );
    r
}

pub fn router(state: AppState) -> Router {
    use routes::*;
    let developer = Router::new()
        .route("/v1/connect", post(create_connect))
        .route("/v1/connect/{code}", get(poll_connect))
        .route("/v1/openai/chat/completions", post(proxy::openai))
        .route("/v1/anthropic/messages", post(proxy::anthropic))
        .route("/v1/anthropic/v1/messages", post(proxy::anthropic)) // official SDK: base_url + /v1/messages
        .route("/v1/openrouter/chat/completions", post(proxy::openrouter))
        .route("/v1/requests/{id}", get(proxy::poll_request))
        .route("/v1/apps", post(create_app))
        .route("/v1/app", get(get_app))
        .route("/v1/app/secret", post(rotate_secret))
        .route("/v1/app/report", get(app_report))
        .route("/connect/{code}/qr.svg", get(enroll::connect_qr))
        .layer(middleware::from_fn(cors));
    Router::new()
        .merge(developer)
        .route("/healthz", get(|| async { "ok" }))
        .route(
            "/",
            get(|| async { axum::response::Redirect::to(routes::GET_APP_URL) }),
        )
        .route("/v1/status", get(ops::status))
        .route("/v1/me/sessions/revoke-all", post(ops::revoke_sessions))
        .route("/v1/me/stop-all", post(ops::stop_all))
        .route("/v1/me/delete", post(ops::delete_account))
        .route("/connect/{code}", get(connect_page))
        .route("/.well-known/apple-app-site-association", get(enroll::aasa))
        .route("/add-key", get(enroll::add_key_page))
        .route("/sdk/keyala.js", get(enroll::sdk_js))
        .route("/about", get(enroll::trust_page))
        .route("/privacy", get(enroll::trust_page))
        .route("/v1/me/logout", post(ops::logout))
        .route("/enroll/{id}", get(enroll::enroll_page))
        .route("/enroll/{id}/qr.svg", get(enroll::enroll_qr))
        .route("/v1/enroll", post(enroll::create))
        .route("/v1/enroll/{id}", get(enroll::poll))
        .route("/v1/enroll/{id}/key", post(enroll::upload))
        .route("/v1/me/enroll/{id}", get(enroll::preview))
        .route("/v1/me/enroll/{id}/claim", post(enroll::claim))
        .route("/v1/me/enroll/{id}/confirm", post(enroll::confirm))
        .route("/v1/me/enroll/{id}/cancel", post(enroll::cancel))
        .route("/v1/me/oauth/openrouter", post(oauth::start))
        .route("/v1/me/oauth/openrouter/flows/{id}", get(oauth::status))
        .route(
            "/v1/me/oauth/openrouter/flows/{id}/cancel",
            post(oauth::cancel),
        )
        .route("/oauth/openrouter/callback", get(oauth::callback))
        .route("/oauth/openrouter/mock", get(oauth::mock_consent))
        .route("/v1/auth/dev", post(dev_login))
        .route("/v1/auth/apple", post(apple_login))
        .route("/v1/auth/password", post(password_login))
        .route("/v1/me/preferences", get(get_prefs).patch(patch_prefs))
        .route("/v1/me/keys", get(list_keys))
        .route("/v1/me/keys/{vendor}", put(put_key).delete(delete_key))
        .route("/v1/me/devices", post(add_device))
        .route("/v1/me/connect/{code}", get(preview_connect))
        .route("/v1/me/connect/{code}/approve", post(approve_connect))
        .route("/v1/me/connect/{code}/deny", post(deny_connect))
        .route("/v1/me/grants", get(list_grants))
        .route(
            "/v1/me/grants/{id}",
            patch(patch_grant).delete(revoke_grant),
        )
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
        (
            self.0,
            Json(json!({"error": {"type": self.1, "message": self.2}})),
        )
            .into_response()
    }
}
impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::Protocol(m) = &e {
            if m.ends_with(UNREADABLE) {
                // wrong KEYALA_MASTER_KEY for this database (see RUNBOOK): say so, never mint a new key over it
                tracing::error!(
                    "{m}: is KEYALA_MASTER_KEY the one this database was written with?"
                );
                return api(503, "data_key_unreadable", "stored data cannot be decrypted with this server's key; the operator has been alerted");
            }
        }
        tracing::error!("db error: {e}");
        api(500, "internal", "internal error")
    }
}

/// JSON body extractor that fails with our error shape; empty body = `{}`.
pub struct J<T>(pub T);
impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for J<T> {
    type Rejection = ApiError;
    async fn from_request(req: Request, st: &S) -> Result<Self, ApiError> {
        let b = Bytes::from_request(req, st)
            .await
            .map_err(|_| bad("unreadable body"))?;
        let b: &[u8] = if b.is_empty() { b"{}" } else { &b };
        // generic message on purpose: serde errors can echo values (e.g. api keys).
        serde_json::from_slice(b)
            .map(J)
            .map_err(|_| bad("invalid JSON body (check field names and types)"))
    }
}

// ---------- ids, tokens, time, crypto ----------

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
pub fn after(secs: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::seconds(secs))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
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
pub fn username_ok(u: &str) -> bool {
    (3..=40).contains(&u.len())
        && u.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
}
/// Equal-length byte compare whose time does not depend on where the inputs differ.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
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
    let ct = c
        .encrypt(
            &nonce,
            Payload {
                msg: plain.as_bytes(),
                aad: aad.as_bytes(),
            },
        )
        .expect("aes-gcm encrypt");
    (ct, nonce.to_vec())
}
pub fn decrypt(master: &[u8; 32], aad: &str, ct: &[u8], nonce: &[u8]) -> Option<String> {
    if nonce.len() != 12 {
        return None;
    }
    let c = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(master));
    let p = c
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ct,
                aad: aad.as_bytes(),
            },
        )
        .ok()?;
    String::from_utf8(p).ok()
}
/// AAD binds a ciphertext to its (user, vendor) row.
pub fn key_aad(user: &str, vendor: &str) -> String {
    format!("{user}|{vendor}")
}

const UNREADABLE: &str = " could not be decrypted";
fn unreadable(what: &str) -> sqlx::Error {
    sqlx::Error::Protocol(format!("{what}{UNREADABLE}"))
}

/// The user's data key: created on first use, stored wrapped with the master key (AAD `dek|{user}`).
/// Call it outside any open write transaction (it may write the users row itself).
pub async fn user_dek(s: &AppState, user: &str) -> Result<[u8; 32], sqlx::Error> {
    let aad = format!("dek|{user}");
    for _ in 0..2 {
        let (ct, n): (Option<Vec<u8>>, Option<Vec<u8>>) =
            sqlx::query_as("SELECT dek_ct, dek_nonce FROM users WHERE id=?")
                .bind(user)
                .fetch_one(&s.db)
                .await?;
        if let (Some(ct), Some(n)) = (ct, n) {
            return decrypt(&s.cfg.master_key, &aad, &ct, &n)
                .and_then(|k| STANDARD.decode(k).ok())
                .and_then(|k| k.try_into().ok())
                .ok_or_else(|| unreadable("user data key"));
        }
        let mut k = [0u8; 32];
        rand::RngCore::fill_bytes(&mut OsRng, &mut k);
        let (ct, n) = encrypt(&s.cfg.master_key, &aad, &STANDARD.encode(k));
        // a concurrent first use may win: then the loop reads that one
        sqlx::query("UPDATE users SET dek_ct=?, dek_nonce=? WHERE id=? AND dek_ct IS NULL")
            .bind(ct)
            .bind(n)
            .bind(user)
            .execute(&s.db)
            .await?;
    }
    Err(unreadable("user data key"))
}

/// Request content at rest: `enc1:` + base64url(nonce || AES-GCM ciphertext), AAD `req|{id}|{field}`.
pub fn seal_text(dek: &[u8; 32], id: &str, field: &str, plain: &str) -> String {
    let (ct, nonce) = encrypt(dek, &format!("req|{id}|{field}"), plain);
    format!("enc1:{}", URL_SAFE_NO_PAD.encode([nonce, ct].concat()))
}
/// Plaintext (rows from before encryption) passes through unchanged.
fn open_text(dek: &[u8; 32], id: &str, field: &str, stored: &str) -> Option<String> {
    let Some(b) = stored.strip_prefix("enc1:") else {
        return Some(stored.to_string());
    };
    let b = URL_SAFE_NO_PAD.decode(b).ok().filter(|b| b.len() > 12)?;
    decrypt(dek, &format!("req|{id}|{field}"), &b[12..], &b[..12])
}
fn sealed(v: &str) -> bool {
    v.starts_with("enc1:")
}

/// Encrypts request content stored before migration 0013, in small batches. Returns rows changed.
async fn encrypt_legacy_requests(s: &AppState) -> Result<usize, sqlx::Error> {
    let mut done = 0;
    loop {
        let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
            // only rows whose user exists (requests.user_id has no foreign key); deletion removes the rest
            "SELECT r.id, r.user_id, r.payload, r.response FROM requests r JOIN users u ON u.id=r.user_id WHERE r.purged=0 AND (r.payload NOT LIKE 'enc1:%' OR r.response NOT LIKE 'enc1:%') LIMIT 100",
        )
        .fetch_all(&s.db)
        .await?;
        if rows.is_empty() {
            return Ok(done);
        }
        for (id, user, payload, response) in rows {
            let dek = user_dek(s, &user).await?;
            let p = if sealed(&payload) {
                payload
            } else {
                seal_text(&dek, &id, "payload", &payload)
            };
            let r = response.map(|r| {
                if sealed(&r) {
                    r
                } else {
                    seal_text(&dek, &id, "response", &r)
                }
            });
            sqlx::query("UPDATE requests SET payload=?, response=? WHERE id=?")
                .bind(p)
                .bind(r)
                .bind(&id)
                .execute(&s.db)
                .await?;
            done += 1;
        }
    }
}

// ---------- auth ----------

pub(crate) fn bearer<'a>(h: &'a HeaderMap, prefix: &str) -> Option<&'a str> {
    h.get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|t| t.starts_with(prefix))
}
fn unauthorized() -> ApiError {
    api(401, "unauthorized", "missing or invalid bearer token")
}
pub async fn auth_session(s: &AppState, h: &HeaderMap) -> R<String> {
    let t = bearer(h, "keyala_sess_").ok_or_else(unauthorized)?;
    sqlx::query_scalar("SELECT user_id FROM sessions WHERE token_hash=?")
        .bind(hash_token(t))
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(unauthorized)
}
/// What an app secret carries: the app record, sealed (AES-GCM) with the master key.
#[derive(Serialize, Deserialize)]
pub struct AppRecord {
    pub id: String,
    pub name: String,
    pub homepage: Option<String>,
    pub icon_url: Option<String>,
    pub allowed_origins: Option<Vec<String>>,
    pub gen: i64,
}
const APP_AAD: &str = "app-secret";
pub fn seal_app(master: &[u8; 32], a: &AppRecord) -> String {
    let (ct, nonce) = encrypt(
        master,
        APP_AAD,
        &serde_json::to_string(a).unwrap_or_default(),
    );
    format!(
        "keyala_app_{}",
        URL_SAFE_NO_PAD.encode([nonce, ct].concat())
    )
}
fn open_app(master: &[u8; 32], t: &str) -> Option<AppRecord> {
    let b = URL_SAFE_NO_PAD
        .decode(t.strip_prefix("keyala_app_")?)
        .ok()?;
    if b.len() < 13 {
        return None;
    }
    serde_json::from_str(&decrypt(master, APP_AAD, &b[12..], &b[..12])?).ok()
}
/// Known secret -> its app. Unknown but sealed by this server -> the database was reset (free
/// plan), so re-create the app from the token. A rotated secret (higher gen) supersedes older ones;
/// an older one is refused once a newer one has been seen since the reset.
pub async fn auth_app(s: &AppState, h: &HeaderMap) -> R<String> {
    let t = bearer(h, "keyala_app_").ok_or_else(unauthorized)?;
    if let Some(id) = sqlx::query_scalar("SELECT id FROM apps WHERE secret_hash=?")
        .bind(hash_token(t))
        .fetch_optional(&s.db)
        .await?
    {
        return Ok(id);
    }
    let a = open_app(&s.cfg.master_key, t).ok_or_else(unauthorized)?;
    let origins = a
        .allowed_origins
        .as_ref()
        .map(|o| serde_json::to_string(o).unwrap_or_default());
    let r = sqlx::query("INSERT INTO apps (id,name,homepage,icon_url,secret_hash,allowed_origins,secret_gen,created_at) VALUES (?,?,?,?,?,?,?,?) \
                         ON CONFLICT(id) DO UPDATE SET secret_hash=excluded.secret_hash, secret_gen=excluded.secret_gen WHERE excluded.secret_gen > apps.secret_gen")
        .bind(&a.id).bind(&a.name).bind(&a.homepage).bind(&a.icon_url).bind(hash_token(t)).bind(&origins).bind(a.gen).bind(now())
        .execute(&s.db).await?;
    if r.rows_affected() == 1 {
        Ok(a.id)
    } else {
        Err(unauthorized())
    }
}
/// `x_api_key`: Anthropic proxy routes also accept the grant token in `x-api-key` (official SDK).
pub async fn auth_grant(s: &AppState, h: &HeaderMap, x_api_key: bool) -> R<GrantRow> {
    let xk = || {
        h.get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|t| t.starts_with("keyala_grant_"))
    };
    let t = bearer(h, "keyala_grant_")
        .or_else(|| if x_api_key { xk() } else { None })
        .ok_or_else(unauthorized)?;
    sqlx::query_as(&format!(
        "{GRANT_SELECT} WHERE g.token_hash=? AND g.revoked_at IS NULL"
    ))
    .bind(hash_token(t))
    .fetch_optional(&s.db)
    .await?
    .ok_or_else(unauthorized)
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
        let mut p = Policy {
            mode: self.mode,
            count: None,
            micro: None,
            alert: None,
            expires_at: None,
        };
        match p.mode.as_str() {
            "ask" | "trust" => {}
            "count" => {
                p.count = Some(
                    self.remaining_count
                        .filter(|c| (0..=1_000_000).contains(c))
                        .ok_or_else(|| {
                            bad("count policy needs remaining_count between 0 and 1000000")
                        })?,
                )
            }
            "budget" => {
                p.micro = Some(to_micro(
                    self.remaining_usd
                        .ok_or_else(|| bad("budget policy needs remaining_usd"))?,
                )?)
            }
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

pub fn policy_json(
    mode: &str,
    count: Option<i64>,
    micro: Option<i64>,
    alert: Option<i64>,
    exp: &Option<String>,
) -> Value {
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

pub async fn set_policy(
    c: &mut SqliteConnection,
    grant_id: &str,
    p: &Policy,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE grants SET policy_gen=policy_gen+1, mode=?, remaining_count=?, remaining_micro=?, alert_micro=?, expires_at=? WHERE id=?")
        .bind(&p.mode).bind(p.count).bind(p.micro).bind(p.alert).bind(&p.expires_at).bind(grant_id)
        .execute(c).await?;
    Ok(())
}

// ---------- row types & JSON ----------

pub const GRANT_SELECT: &str = "SELECT g.id, g.user_id, g.app_id, g.vendor, COALESCE(g.billing_vendor, g.vendor) AS billing_vendor, g.mode, g.remaining_count, g.remaining_micro, g.alert_micro, g.expires_at, g.created_at, g.last_used_at, g.spent_micro, g.request_count, a.name AS app_name, a.homepage AS app_homepage, a.icon_url AS app_icon_url FROM grants g JOIN apps a ON a.id=g.app_id";
pub const REQUEST_SELECT: &str = "SELECT r.id, r.grant_id, r.user_id, r.app_id, r.vendor, COALESCE(r.billing_vendor, r.vendor) AS billing_vendor, r.upstream_model, r.model, r.endpoint, r.status, r.estimated_micro, r.actual_micro, r.reserved_micro, r.reserved_count, r.policy_gen, r.payload, r.response, r.http_status, r.purged, r.created_at, r.decided_at, r.completed_at, a.name AS app_name, a.homepage AS app_homepage, a.icon_url AS app_icon_url FROM requests r JOIN apps a ON a.id=r.app_id";

pub fn app_json(id: &str, name: &str, homepage: &Option<String>, icon: &Option<String>) -> Value {
    json!({"id": id, "name": name, "homepage": homepage, "icon_url": icon})
}

#[derive(sqlx::FromRow)]
pub struct GrantRow {
    pub id: String,
    pub user_id: String,
    pub app_id: String,
    pub vendor: String,
    /// who is billed: the same as `vendor` (direct) or `openrouter` (routed); frozen at approval
    pub billing_vendor: String,
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
               "vendor": self.vendor, "billing_vendor": self.billing_vendor,
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
    pub billing_vendor: String,
    /// exact OpenRouter slug actually sent when routed (the original model stays in `model`); None = direct
    pub upstream_model: Option<String>,
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
    /// 1 = content removed by the retention sweep
    pub purged: i64,
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
            "vendor": self.vendor, "billing_vendor": self.billing_vendor, "model": self.model, "upstream_model": self.upstream_model,
            "endpoint": self.endpoint, "status": self.status,
            "estimated_usd": usd(self.estimated_micro), "actual_usd": self.actual_micro.map(usd),
            "created_at": self.created_at, "decided_at": self.decided_at, "completed_at": self.completed_at,
            "purged": self.purged != 0});
        if detail && self.purged != 0 {
            // content removed by the retention sweep; only metadata is left
            v["payload"] = Value::Null;
            v["response"] = Value::Null;
        } else if detail {
            fn parse(s: &str) -> Value {
                serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.into()))
            }
            v["payload"] = parse(&self.payload);
            v["response"] = self.response.as_deref().map(parse).unwrap_or(Value::Null);
        }
        v
    }
}

/// One request with its content decrypted (the only way content is read back).
pub async fn request_row(s: &AppState, id: &str) -> Result<Option<RequestRow>, sqlx::Error> {
    let r: Option<RequestRow> = sqlx::query_as(&format!("{REQUEST_SELECT} WHERE r.id=?"))
        .bind(id)
        .fetch_optional(&s.db)
        .await?;
    let Some(mut r) = r else { return Ok(None) };
    if sealed(&r.payload) || r.response.as_deref().is_some_and(sealed) {
        let dek = user_dek(s, &r.user_id).await?;
        r.payload = open_text(&dek, &r.id, "payload", &r.payload)
            .ok_or_else(|| unreadable("request payload"))?;
        r.response = match r.response.take() {
            None => None,
            Some(v) => Some(
                open_text(&dek, &r.id, "response", &v)
                    .ok_or_else(|| unreadable("request response"))?,
            ),
        };
    }
    Ok(Some(r))
}

// ---------- events ----------

pub fn emit(s: &AppState, user: &str, event: &str, data: &Value) {
    let _ = s
        .events
        .send((user.to_string(), event.to_string(), data.to_string()));
}
pub async fn emit_request(s: &AppState, event: &str, id: &str) {
    if let Ok(Some(r)) = request_row(s, id).await {
        emit(s, &r.user_id, event, &r.json(false));
    }
}
pub async fn emit_grant(s: &AppState, id: &str) {
    if let Ok(Some(g)) = sqlx::query_as::<_, GrantRow>(&format!("{GRANT_SELECT} WHERE g.id=?"))
        .bind(id)
        .fetch_optional(&s.db)
        .await
    {
        emit(s, &g.user_id, "grant.updated", &g.json());
    }
}
