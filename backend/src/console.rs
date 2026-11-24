//! Web console backend: phone sign-in for the web, web password, account-owned developer apps, and the
//! console page itself. Web sessions are limited by `auth_level` (see lib.rs): handlers that call
//! `auth_level(.., READ)` are the allowlist; everything else answers 403 `web_session_not_allowed`.
use crate::routes::{country, not_found, rate_limit, NewApp};
use crate::*;
use argon2::{
    password_hash::{rand_core::OsRng as PwRng, PasswordHash, SaltString},
    Argon2, PasswordHasher, PasswordVerifier,
};
use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::header,
    response::Html,
};
use std::{net::SocketAddr, sync::OnceLock};

const LOGIN_TTL_SECS: i64 = 300;

// ---------- passwords ----------

fn hash_password(pw: &str) -> Option<String> {
    Argon2::default()
        .hash_password(pw.as_bytes(), &SaltString::generate(&mut PwRng))
        .ok()
        .map(|h| h.to_string())
}

/// Always runs one argon2 verify (against a throwaway hash when there is no account), so the time
/// taken does not say whether the account exists.
pub async fn verify_password(pw: String, stored: Option<String>) -> bool {
    static DUMMY: OnceLock<String> = OnceLock::new();
    tokio::task::spawn_blocking(move || {
        let known = stored.is_some();
        let hash = stored.unwrap_or_else(|| {
            DUMMY
                .get_or_init(|| hash_password("dummy").unwrap_or_default())
                .clone()
        });
        let ok = PasswordHash::new(&hash)
            .map(|p| Argon2::default().verify_password(pw.as_bytes(), &p).is_ok())
            .unwrap_or(false);
        ok && known
    })
    .await
    .unwrap_or(false)
}

#[derive(Deserialize)]
pub struct PasswordIn {
    password: String,
}

pub async fn set_password(
    State(s): State<AppState>,
    h: HeaderMap,
    J(b): J<PasswordIn>,
) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    let n = b.password.chars().count();
    if !(12..=200).contains(&n) {
        return Err(bad("password must be 12 to 200 characters"));
    }
    let pw = b.password;
    let hash = tokio::task::spawn_blocking(move || hash_password(&pw))
        .await
        .ok()
        .flatten()
        .ok_or_else(|| api(500, "internal", "internal error"))?;
    sqlx::query("UPDATE users SET password_hash=? WHERE id=?")
        .bind(hash)
        .bind(&u)
        .execute(&s.db)
        .await?;
    Ok(Json(json!({"has_password": true})))
}

pub async fn delete_password(State(s): State<AppState>, h: HeaderMap) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    sqlx::query("UPDATE users SET password_hash=NULL WHERE id=?")
        .bind(&u)
        .execute(&s.db)
        .await?;
    Ok(Json(json!({"has_password": false})))
}

pub async fn me(State(s): State<AppState>, h: HeaderMap) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    let (email, provider) = account(&s, &u).await?;
    let (pw, pk): (bool, bool) = sqlx::query_as(
        "SELECT password_hash IS NOT NULL, EXISTS(SELECT 1 FROM passkeys WHERE user_id=?1) FROM users WHERE id=?1",
    )
    .bind(&u)
    .fetch_one(&s.db)
    .await?;
    Ok(Json(
        json!({"id": u, "auth_provider": provider, "email": email, "has_password": pw, "has_passkey": pk}),
    ))
}

/// (email, auth_provider) of an account: how it was created, for picking the reauth UI.
pub(crate) async fn account(s: &AppState, uid: &str) -> R<(String, &'static str)> {
    let (email, apple, google): (String, bool, bool) = sqlx::query_as(
        "SELECT email, apple_sub IS NOT NULL, google_sub IS NOT NULL FROM users WHERE id=?",
    )
    .bind(uid)
    .fetch_one(&s.db)
    .await?;
    let p = if apple {
        "apple"
    } else if google {
        "google"
    } else if email.ends_with("@password.invalid") {
        "password"
    } else {
        "dev"
    };
    Ok((email, p))
}

// ---------- phone sign-in for the web ----------

fn login_id_ok(id: &str) -> bool {
    id.len() == 25
        && id.starts_with("wl_")
        && id[3..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[derive(sqlx::FromRow)]
struct Login {
    secret_hash: String,
    status: String,
    user_id: Option<String>,
    user_agent: Option<String>,
    country: Option<String>,
    created_at: String,
    expires_at: String,
}
impl Login {
    /// pending/approved rows past their time read as `expired` even before the janitor deletes them
    fn status(&self) -> &str {
        if matches!(self.status.as_str(), "pending" | "approved")
            && self.expires_at.as_str() <= now().as_str()
        {
            "expired"
        } else {
            &self.status
        }
    }
}
async fn login_row(s: &AppState, id: &str) -> R<Login> {
    if !login_id_ok(id) {
        return Err(not_found("web login"));
    }
    sqlx::query_as("SELECT secret_hash,status,user_id,user_agent,country,created_at,expires_at FROM web_logins WHERE id=?")
        .bind(id)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(|| not_found("web login"))
}

pub async fn web_login_start(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    h: HeaderMap,
) -> R<Json<Value>> {
    rate_limit(&s, peer, &h, "weblogin", 20, 600, "sign-in codes")?;
    let (id, secret, exp) = (
        format!("wl_{}", rand_b64(16)),
        new_token("keyala_wl_"),
        after(LOGIN_TTL_SECS),
    );
    let ua: Option<String> = h
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.chars().take(200).collect());
    sqlx::query("INSERT INTO web_logins (id,secret_hash,user_agent,country,created_at,expires_at) VALUES (?,?,?,?,?,?)")
        .bind(&id).bind(hash_token(&secret)).bind(ua).bind(country(&s, &h)).bind(now()).bind(&exp)
        .execute(&s.db).await?;
    let url = format!("{}/login/{id}", s.cfg.public_url);
    Ok(Json(
        json!({"login_id": id, "login_secret": secret, "qr_url": format!("{url}/qr.svg"), "url": url, "expires_at": exp}),
    ))
}

/// The browser polls with its login secret. When the phone approved, the first poll returns the web
/// session token (never again: the row becomes `used`).
pub async fn web_login_poll(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> R<Json<Value>> {
    let denied = || api(401, "unauthorized", "missing or invalid login secret");
    let t = bearer(&h, "keyala_wl_").ok_or_else(denied)?;
    let r = login_row(&s, &id).await.map_err(|_| denied())?;
    if !ct_eq(hash_token(t).as_bytes(), r.secret_hash.as_bytes()) {
        return Err(denied());
    }
    if r.status() == "approved" {
        let won = sqlx::query(
            "UPDATE web_logins SET status='used' WHERE id=? AND status='approved' AND expires_at>?",
        )
        .bind(&id)
        .bind(now())
        .execute(&s.db)
        .await?
        .rows_affected()
            == 1;
        if won {
            let uid = r
                .user_id
                .ok_or_else(|| api(500, "internal", "internal error"))?;
            let (email, provider) = account(&s, &uid).await?;
            let Json(mut v) = crate::routes::session_for(&s, &uid, &email, provider, true).await?;
            v["status"] = "approved".into();
            return Ok(Json(v));
        }
        return Ok(Json(json!({"status": "used"})));
    }
    Ok(Json(json!({"status": r.status()})))
}

pub async fn web_login_preview(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> R<Json<Value>> {
    auth_session(&s, &h).await?;
    let r = login_row(&s, &id).await?;
    Ok(Json(
        json!({"id": id, "status": r.status(), "created_at": r.created_at, "expires_at": r.expires_at, "user_agent": r.user_agent, "country": r.country}),
    ))
}

async fn decide(s: &AppState, h: &HeaderMap, id: &str, to: &str) -> R<Json<Value>> {
    let u = auth_session(s, h).await?;
    login_row(s, id).await?;
    let won = sqlx::query("UPDATE web_logins SET status=?, user_id=? WHERE id=? AND status='pending' AND expires_at>?")
        .bind(to).bind(&u).bind(id).bind(now())
        .execute(&s.db).await?.rows_affected() == 1;
    if !won {
        return Err(api(
            409,
            "not_pending",
            "this sign-in was already decided or has expired",
        ));
    }
    Ok(Json(json!({"id": id, "status": to})))
}
pub async fn web_login_approve(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> R<Json<Value>> {
    decide(&s, &h, &id, "approved").await
}
pub async fn web_login_deny(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> R<Json<Value>> {
    decide(&s, &h, &id, "denied").await
}

/// What a phone without the app sees after scanning the web sign-in QR.
pub async fn login_page(Path(id): Path<String>) -> R<Html<String>> {
    if !login_id_ok(&id) {
        return Err(not_found("web login"));
    }
    Ok(Html(format!(
        "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'><title>Keyala</title>\
         <body style='font-family:system-ui;text-align:center;margin:4rem 1rem'><h1>Sign in to the Keyala console</h1>\
         <p><a href='keyala://login/{id}' style='font-size:1.3rem'>Open in the Keyala app</a></p>\
         <p style='color:#666'>Approve the sign-in in the app. The code is valid for 5 minutes.</p>{}",
        crate::enroll::TRUST_LINK
    )))
}

pub async fn login_qr(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    if !login_id_ok(&id) {
        return not_found("web login").into_response();
    }
    crate::enroll::svg(&format!("{}/login/{id}", s.cfg.public_url))
}

/// Expired web sessions and finished sign-in rows do not pile up.
pub async fn janitor(s: AppState) {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let _ = sqlx::query("DELETE FROM sessions WHERE expires_at IS NOT NULL AND expires_at<=?")
            .bind(now())
            .execute(&s.db)
            .await;
        let _ = sqlx::query("DELETE FROM web_logins WHERE expires_at<=?")
            .bind(after(-3600))
            .execute(&s.db)
            .await;
    }
}

// ---------- developer apps owned by accounts ----------

async fn owned_app(s: &AppState, user: &str, id: &str) -> R<()> {
    sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM apps WHERE id=? AND owner_user_id=? AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user)
    .fetch_optional(&s.db)
    .await?
    .map(|_| ())
    .ok_or_else(|| not_found("app"))
}

type AppRowT = (
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    i64,
    i64,
);
fn app_summary(r: AppRowT) -> Value {
    let origins: Option<Value> = r.3.and_then(|o| serde_json::from_str(&o).ok());
    json!({"id": r.0, "name": r.1, "homepage": r.2, "allowed_origins": origins, "created_at": r.4, "requests_7d": r.5, "cost_usd_7d": usd(r.6)})
}
const APP_SUMMARY: &str = "SELECT a.id, a.name, a.homepage, a.allowed_origins, a.created_at, \
    (SELECT COUNT(*) FROM requests r WHERE r.app_id=a.id AND r.created_at>=?1), \
    (SELECT COALESCE(SUM(r.actual_micro),0) FROM requests r WHERE r.app_id=a.id AND r.created_at>=?1 AND r.status='completed') \
    FROM apps a WHERE a.owner_user_id=?2 AND a.deleted_at IS NULL";
fn since_7d() -> String {
    format!(
        "{}T00:00:00",
        chrono::Utc::now().date_naive() - chrono::Duration::days(6)
    )
}

pub async fn apps_list(State(s): State<AppState>, h: HeaderMap) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    let rows: Vec<AppRowT> = sqlx::query_as(&format!("{APP_SUMMARY} ORDER BY a.created_at DESC"))
        .bind(since_7d())
        .bind(&u)
        .fetch_all(&s.db)
        .await?;
    Ok(Json(rows.into_iter().map(app_summary).collect()))
}

pub async fn apps_create(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    h: HeaderMap,
    J(b): J<NewApp>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    rate_limit(&s, peer, &h, "meapps", 20, 3600, "apps created")?;
    crate::routes::register_app(&s, b, Some(&u)).await
}

#[derive(Deserialize)]
pub struct Claim {
    app_secret: String,
}

/// Attach an existing unowned app to this account by proving its secret.
pub async fn apps_claim(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    h: HeaderMap,
    J(b): J<Claim>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    rate_limit(&s, peer, &h, "claim", 10, 600, "claim attempts")?;
    let id = crate::auth_app_token(&s, b.app_secret.trim())
        .await
        .map_err(|e| {
            if e.0 == StatusCode::UNAUTHORIZED {
                api(401, "invalid_app_secret", "that app secret is not valid")
            } else {
                e
            }
        })?;
    let won = sqlx::query(
        "UPDATE apps SET owner_user_id=? WHERE id=? AND deleted_at IS NULL AND (owner_user_id IS NULL OR owner_user_id=?)",
    )
    .bind(&u)
    .bind(&id)
    .bind(&u)
    .execute(&s.db)
    .await?
    .rows_affected()
        == 1;
    if !won {
        return Err(api(
            409,
            "already_owned",
            "this app already belongs to another account",
        ));
    }
    let r: AppRowT = sqlx::query_as(&format!("{APP_SUMMARY} AND a.id=?3"))
        .bind(since_7d())
        .bind(&u)
        .bind(&id)
        .fetch_one(&s.db)
        .await?;
    Ok(Json(app_summary(r)))
}

pub async fn apps_report(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    owned_app(&s, &u, &id).await?;
    crate::routes::report_json(&s, &id, &q).await
}

/// Edit name, homepage and allowed origins (null origins = server app only). New connections use the
/// new values; existing grants keep working.
// ponytail: the sealed secret still carries the old record, which only matters if the row vanishes
// (database reset); re-creation would bring back the old values. Re-seal on edit if that ever matters.
pub async fn apps_update(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    J(b): J<NewApp>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    owned_app(&s, &u, &id).await?;
    crate::routes::check_app(&b)?;
    let origins = b
        .allowed_origins
        .as_ref()
        .map(|o| serde_json::to_string(o).unwrap_or_default());
    sqlx::query("UPDATE apps SET name=?, homepage=?, allowed_origins=? WHERE id=? AND owner_user_id=? AND deleted_at IS NULL")
        .bind(b.name.trim())
        .bind(&b.homepage)
        .bind(&origins)
        .bind(&id)
        .bind(&u)
        .execute(&s.db)
        .await?;
    let r: AppRowT = sqlx::query_as(&format!("{APP_SUMMARY} AND a.id=?3"))
        .bind(since_7d())
        .bind(&u)
        .bind(&id)
        .fetch_one(&s.db)
        .await?;
    Ok(Json(app_summary(r)))
}

/// Delete an app: its secret stops working, every user's grant for it is revoked, waiting calls are
/// denied (never sent, reservations refunded) and pending connect codes are refused. Users keep
/// their history. Calls already being sent finish.
pub async fn apps_delete(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    let mut tx = s.db.begin().await?;
    // write first: takes the write lock, so nothing is claimed for sending while we deny
    let n = sqlx::query(
        "UPDATE apps SET deleted_at=? WHERE id=? AND owner_user_id=? AND deleted_at IS NULL",
    )
    .bind(now())
    .bind(&id)
    .bind(&u)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n != 1 {
        return Err(not_found("app"));
    }
    let gids: Vec<String> = sqlx::query_scalar(
        "UPDATE grants SET revoked_at=? WHERE app_id=? AND revoked_at IS NULL RETURNING id",
    )
    .bind(now())
    .bind(&id)
    .fetch_all(&mut *tx)
    .await?;
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM requests WHERE app_id=? AND status IN ('pending','approved')",
    )
    .bind(&id)
    .fetch_all(&mut *tx)
    .await?;
    let denied = crate::ops::deny_unsent(&mut tx, ids).await?;
    sqlx::query("UPDATE connect_codes SET status='denied' WHERE app_id=? AND status='pending'")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    for r in &denied {
        emit_request(&s, "request.updated", r).await;
    }
    for g in &gids {
        emit_grant(&s, g).await;
    }
    Ok(Json(
        json!({"deleted": true, "grants_revoked": gids.len(), "requests_denied": denied.len()}),
    ))
}

pub async fn apps_rotate(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    owned_app(&s, &u, &id).await?;
    crate::routes::rotate_app_secret(&s, &id).await
}

// ---------- the page ----------

const PAGE: &str = include_str!("console.html");
const SCRIPT: &str = include_str!("console.js");

pub async fn page() -> axum::response::Response {
    use axum::response::IntoResponse;
    // script comes from this origin only; styles are inline; nothing leaves the origin
    let csp = "default-src 'self'; script-src 'self'; style-src 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";
    (
        [
            (header::CONTENT_SECURITY_POLICY, csp),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CACHE_CONTROL, "no-store"),
            (header::X_FRAME_OPTIONS, "DENY"),
        ],
        Html(PAGE),
    )
        .into_response()
}

pub async fn script() -> axum::response::Response {
    use axum::response::IntoResponse;
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        SCRIPT,
    )
        .into_response()
}
