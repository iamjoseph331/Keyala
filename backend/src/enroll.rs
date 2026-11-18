//! Add a key from a computer, where the user is already signed in to the provider's console.
//! The computer's browser starts an enrollment and shows its QR; the signed-in phone claims it;
//! the browser uploads the key; the phone confirms provider + last4; only then is it saved.
//! The browser never gets a session, can never read a key back, and the row is single use.
//! Also: QR images and the universal-link association file.
use crate::routes::{check_key, last4, not_found, rate_limit, store_key, vendor_ok};
use crate::*;
use axum::{
    extract::{ConnectInfo, Path, State},
    http::header,
    response::Html,
};
use std::net::SocketAddr;

const TTL_SECS: i64 = 600;
const AAD: &str = "enroll";

fn id_ok(id: &str) -> bool {
    id.len() == 25 && id.starts_with("en_") && id[3..].bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[derive(sqlx::FromRow)]
struct Row {
    vendor: String,
    status: String,
    user_id: Option<String>,
    last4: Option<String>,
    expires_at: String,
}
impl Row {
    fn json(&self, id: &str) -> Value {
        json!({"id": id, "vendor": self.vendor, "status": self.status, "last4": self.last4, "expires_at": self.expires_at})
    }
}

/// Expired rows read as `expired` even before the sweeper wipes them.
async fn row(s: &AppState, id: &str, extra: &'static str, bind: Option<&str>) -> R<Row> {
    if !id_ok(id) {
        return Err(not_found("enrollment"));
    }
    let sql = format!("SELECT vendor,status,user_id,last4,expires_at FROM enrollments WHERE id=? {extra}");
    let mut q = sqlx::query_as::<_, Row>(&sql).bind(id);
    if let Some(b) = bind {
        q = q.bind(b.to_string());
    }
    let mut r = q.fetch_optional(&s.db).await?.ok_or_else(|| not_found("enrollment"))?;
    if matches!(r.status.as_str(), "pending" | "claimed" | "submitted") && r.expires_at.as_str() <= now().as_str() {
        r.status = "expired".into();
    }
    Ok(r)
}

async fn notify(s: &AppState, id: &str) {
    if let Ok(r) = row(s, id, "", None).await {
        if let Some(u) = &r.user_id {
            emit(s, u, "enroll.updated", &r.json(id));
        }
    }
}

// ---------- browser (computer) side: bearer mk_es_ ----------

#[derive(Deserialize)]
pub struct NewEnroll {
    vendor: String,
}

pub async fn create(State(s): State<AppState>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, J(b): J<NewEnroll>) -> R<Json<Value>> {
    vendor_ok(&b.vendor)?;
    rate_limit(&s, peer, &headers, "enroll", 10, 600, "key enrollments")?;
    // 16 random bytes: unguessable, so nobody can claim someone else's enrollment by trying codes
    let (id, secret, expires) = (format!("en_{}", rand_b64(16)), new_token("mk_es_"), after(TTL_SECS));
    sqlx::query("INSERT INTO enrollments (id,secret_hash,vendor,created_at,expires_at) VALUES (?,?,?,?,?)")
        .bind(&id).bind(hash_token(&secret)).bind(&b.vendor).bind(now()).bind(&expires)
        .execute(&s.db).await?;
    let url = format!("{}/enroll/{id}", s.cfg.public_url);
    Ok(Json(json!({"enroll_id": id, "enroll_url": url, "qr_url": format!("{url}/qr.svg"), "enroll_secret": secret, "expires_at": expires})))
}

fn secret_of(h: &HeaderMap) -> R<String> {
    bearer(h, "mk_es_").map(hash_token).ok_or_else(|| api(401, "unauthorized", "missing or invalid bearer token"))
}

pub async fn poll(State(s): State<AppState>, h: HeaderMap, Path(id): Path<String>) -> R<Json<Value>> {
    let sh = secret_of(&h)?;
    Ok(Json(row(&s, &id, "AND secret_hash=?", Some(&sh)).await?.json(&id)))
}

#[derive(Deserialize)]
pub struct Upload {
    api_key: String,
}

/// Once, after the phone claimed it. The candidate is immutable: what the phone reviews is what it
/// saves. A wrong key = cancel on the phone and start a new enrollment.
pub async fn upload(State(s): State<AppState>, h: HeaderMap, Path(id): Path<String>, J(b): J<Upload>) -> R<Json<Value>> {
    let sh = secret_of(&h)?;
    let r = row(&s, &id, "AND secret_hash=?", Some(&sh)).await?;
    if r.status != "claimed" {
        return Err(api(409, "not_claimed", format!("enrollment is {}; it takes one key after the phone scans the QR", r.status)));
    }
    let key = check_key(&s, &r.vendor, &b.api_key).await?;
    let (ct, nonce) = encrypt(&s.cfg.master_key, &format!("{AAD}|{id}"), &key);
    let u = sqlx::query("UPDATE enrollments SET status='submitted', ciphertext=?, nonce=?, last4=? WHERE id=? AND status='claimed' AND expires_at>?")
        .bind(ct).bind(nonce).bind(last4(&key)).bind(&id).bind(now())
        .execute(&s.db).await?;
    if u.rows_affected() != 1 {
        return Err(api(409, "not_claimed", "enrollment is no longer open"));
    }
    notify(&s, &id).await;
    Ok(Json(row(&s, &id, "", None).await?.json(&id)))
}

// ---------- phone side: session ----------

/// Anyone signed in may look at an unclaimed enrollment (they hold its QR); after that, only its owner.
pub async fn preview(State(s): State<AppState>, h: HeaderMap, Path(id): Path<String>) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    let r = row(&s, &id, "AND (status='pending' OR user_id=?)", Some(&u)).await?;
    let current: Option<String> = sqlx::query_scalar("SELECT last4 FROM api_keys WHERE user_id=? AND vendor=?").bind(&u).bind(&r.vendor).fetch_optional(&s.db).await?;
    let mut v = r.json(&id);
    v["current_last4"] = current.into();
    Ok(Json(v))
}

pub async fn claim(State(s): State<AppState>, h: HeaderMap, Path(id): Path<String>) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    row(&s, &id, "", None).await?; // 404 for unknown ids
    let r = sqlx::query("UPDATE enrollments SET status='claimed', user_id=? WHERE id=? AND status='pending' AND expires_at>?").bind(&u).bind(&id).bind(now()).execute(&s.db).await?;
    if r.rows_affected() != 1 {
        return Err(api(409, "not_pending", "this enrollment was already used or has expired; start again on the computer"));
    }
    notify(&s, &id).await;
    Ok(Json(row(&s, &id, "", None).await?.json(&id)))
}

/// One transaction: claim the submitted candidate (status guard) and save it, or neither.
pub async fn confirm(State(s): State<AppState>, h: HeaderMap, Path(id): Path<String>) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    let r = row(&s, &id, "AND user_id=?", Some(&u)).await?;
    let mut tx = s.db.begin().await?;
    let (ct, nonce): (Option<Vec<u8>>, Option<Vec<u8>>) = sqlx::query_as("SELECT ciphertext, nonce FROM enrollments WHERE id=?").bind(&id).fetch_one(&mut *tx).await?;
    let f = sqlx::query("UPDATE enrollments SET status='saved', ciphertext=NULL, nonce=NULL WHERE id=? AND user_id=? AND status='submitted' AND expires_at>?").bind(&id).bind(&u).bind(now()).execute(&mut *tx).await?;
    if f.rows_affected() != 1 {
        return Err(api(409, "not_submitted", format!("enrollment is {}; paste the key on the computer first", r.status)));
    }
    let key = ct.zip(nonce).and_then(|(c, n)| decrypt(&s.cfg.master_key, &format!("{AAD}|{id}"), &c, &n)).ok_or_else(|| api(500, "internal", "could not read the uploaded key"))?;
    let k = store_key(&s, &mut tx, &u, &r.vendor, &key).await?;
    tx.commit().await?;
    notify(&s, &id).await;
    Ok(Json(k))
}

pub async fn cancel(State(s): State<AppState>, h: HeaderMap, Path(id): Path<String>) -> R<StatusCode> {
    let u = auth_session(&s, &h).await?;
    let r = sqlx::query("UPDATE enrollments SET status='cancelled', ciphertext=NULL, nonce=NULL WHERE id=? AND user_id=? AND status IN ('claimed','submitted')").bind(&id).bind(&u).execute(&s.db).await?;
    if r.rows_affected() != 1 {
        return Err(not_found("open enrollment"));
    }
    notify(&s, &id).await;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- pages, QR, universal links ----------

fn svg(url: &str) -> Response {
    let Ok(code) = qrcode::QrCode::new(url.as_bytes()) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let img = code.render::<qrcode::render::svg::Color>().min_dimensions(240, 240).quiet_zone(true).build();
    ([(header::CONTENT_TYPE, "image/svg+xml"), (header::CACHE_CONTROL, "no-store")], img).into_response()
}

pub async fn enroll_qr(State(s): State<AppState>, Path(id): Path<String>) -> Response {
    if !id_ok(&id) {
        return not_found("enrollment").into_response();
    }
    svg(&format!("{}/enroll/{id}", s.cfg.public_url))
}

pub async fn connect_qr(State(s): State<AppState>, Path(code): Path<String>) -> Response {
    if !routes::connect_code_ok(&code) {
        return not_found("connect code").into_response();
    }
    svg(&format!("{}/connect/{code}", s.cfg.public_url))
}

/// Opened on a phone without the app (universal links not set up yet): hand over to the app.
pub async fn enroll_page(Path(id): Path<String>) -> R<Html<String>> {
    if !id_ok(&id) {
        return Err(not_found("enrollment"));
    }
    Ok(Html(format!(
        "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'><title>MasterKey</title>\
         <body style='font-family:system-ui;text-align:center;margin:4rem 1rem'><h1>Add a key to MasterKey</h1>\
         <p><a href='masterkey://enroll/{id}' style='font-size:1.3rem'>Open in the MasterKey app</a></p>"
    )))
}

/// Lets iOS open https://<server>/connect/* and /enroll/* links (and camera QR scans) in the app.
pub async fn aasa(State(s): State<AppState>) -> Response {
    let app = format!("{}.{}", s.cfg.apple_team_id, s.cfg.apple_bundle_id);
    let v = json!({"applinks": {"details": [{"appIDs": [app], "components": [{"/": "/connect/*", "comment": "pairing"}, {"/": "/enroll/*", "comment": "add a key from a computer"}]}]}});
    ([(header::CONTENT_TYPE, "application/json")], v.to_string()).into_response()
}

const ADD_KEY_HTML: &str = include_str!("add_key.html");

pub async fn add_key_page() -> Response {
    let csp = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";
    (
        [(header::CONTENT_SECURITY_POLICY, csp), (header::REFERRER_POLICY, "no-referrer"), (header::CACHE_CONTROL, "no-store"), (header::X_FRAME_OPTIONS, "DENY")],
        Html(ADD_KEY_HTML),
    )
        .into_response()
}
