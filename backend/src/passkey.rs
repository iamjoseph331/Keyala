//! Passkeys (WebAuthn, ES256 only): add one to a signed-in account, then sign in with it on iOS or
//! Android without Apple or Google. Attestation is not checked (the "none" policy): a passkey
//! proves possession of the private key, not the make of the device.
use crate::{
    routes::{not_found, rate_limit},
    *,
};
use axum::extract::{ConnectInfo, Path, State};
use ring::signature::{UnparsedPublicKey, ECDSA_P256_SHA256_ASN1};
use std::net::SocketAddr;

const TTL_SECS: i64 = 300;
const MAX_PER_USER: i64 = 10;
const UP_UV: u8 = 0x01 | 0x04; // user present + user verified (Face ID / PIN)
const BE: u8 = 0x08; // backup eligible
const BS: u8 = 0x10; // backed up (only valid if BE)
const AT: u8 = 0x40; // attested credential data included

fn b64(b: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(b)
}
fn unb64(s: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(s.trim_end_matches('=')).ok()
}
fn rejected() -> ApiError {
    api(
        400,
        "passkey_rejected",
        "the passkey response did not verify",
    )
}
fn failed() -> ApiError {
    api(401, "passkey_failed", "passkey not recognised")
}
fn expired() -> ApiError {
    api(
        400,
        "challenge_expired",
        "passkey challenge unknown, used or expired; ask for a new one",
    )
}

async fn new_challenge(s: &AppState, purpose: &str, user: Option<&str>) -> R<Value> {
    sqlx::query("DELETE FROM passkey_challenges WHERE expires_at<?")
        .bind(now())
        .execute(&s.db)
        .await?;
    let (id, ch, exp) = (new_id("pkc"), rand_b64(32), after(TTL_SECS));
    sqlx::query("INSERT INTO passkey_challenges (id,user_id,purpose,challenge,expires_at) VALUES (?,?,?,?,?)")
        .bind(&id)
        .bind(user)
        .bind(purpose)
        .bind(&ch)
        .bind(&exp)
        .execute(&s.db)
        .await?;
    Ok(
        json!({"challenge_id": id, "challenge": ch, "rp_id": s.cfg.passkey_rp_id, "expires_at": exp}),
    )
}

/// Single use: the row is gone whether or not the response then verifies.
async fn take(s: &AppState, id: &str, purpose: &str) -> R<(Option<String>, String)> {
    sqlx::query_as("DELETE FROM passkey_challenges WHERE id=? AND purpose=? AND expires_at>? RETURNING user_id, challenge")
        .bind(id)
        .bind(purpose)
        .bind(now())
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(expired)
}

fn client_data_ok(s: &AppState, raw: &[u8], ty: &str, challenge: &str) -> bool {
    let Ok(c) = serde_json::from_slice::<Value>(raw) else {
        return false;
    };
    c["type"] == ty
        && c["challenge"].as_str() == Some(challenge)
        && c["origin"]
            .as_str()
            .is_some_and(|o| s.cfg.passkey_origins.iter().any(|a| a == o))
        && matches!(c.get("crossOrigin"), None | Some(Value::Bool(false)))
}

/// rpIdHash and UP+UV flags checked; returns (flags, signCount, rest).
fn auth_data<'a>(s: &AppState, d: &'a [u8]) -> Option<(u8, u32, &'a [u8])> {
    if d.len() < 37 || d[..32] != Sha256::digest(s.cfg.passkey_rp_id.as_bytes())[..] {
        return None;
    }
    let flags = d[32];
    (flags & UP_UV == UP_UV && (flags & BS == 0 || flags & BE != 0)).then(|| {
        (
            flags,
            u32::from_be_bytes([d[33], d[34], d[35], d[36]]),
            &d[37..],
        )
    })
}

fn int(v: &ciborium::Value) -> Option<i128> {
    v.as_integer().map(i128::from)
}

/// COSE EC2 / ES256 / P-256 key -> uncompressed point (0x04 | x | y)
fn es256_point(k: &ciborium::Value) -> Option<Vec<u8>> {
    let m = k.as_map()?;
    let get = |i: i128| m.iter().find(|(k, _)| int(k) == Some(i)).map(|(_, v)| v);
    if int(get(1)?)? != 2 || int(get(3)?)? != -7 || int(get(-1)?)? != 1 {
        return None;
    }
    let (x, y) = (get(-2)?.as_bytes()?, get(-3)?.as_bytes()?);
    (x.len() == 32 && y.len() == 32).then(|| [&[4u8][..], x, y].concat())
}

// ---------- registration (signed in) ----------

pub async fn register_options(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    h: HeaderMap,
    body: axum::body::Bytes,
) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    rate_limit(
        &s,
        peer,
        &h,
        "passkey_register",
        20,
        600,
        "passkey registrations",
    )?;
    // a session alone is not enough: bad JSON, unknown fields or no proof are all the same 401
    let v: Value = if body.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&body).map_err(|_| crate::ops::reauth_failed())?
    };
    let known = v.as_object().is_some_and(|m| {
        m.keys()
            .all(|k| crate::ops::FRESH_FIELDS.contains(&k.as_str()))
    });
    let proof: crate::ops::FreshIdentity = known
        .then(|| serde_json::from_value(v).ok())
        .flatten()
        .ok_or_else(crate::ops::reauth_failed)?;
    crate::ops::verify_fresh_identity(&s, &u, &proof).await?;
    let (email, _) = crate::console::account(&s, &u).await?;
    let exclude: Vec<String> =
        sqlx::query_scalar("SELECT credential_id FROM passkeys WHERE user_id=?")
            .bind(&u)
            .fetch_all(&s.db)
            .await?;
    let mut v = new_challenge(&s, "register", Some(&u)).await?;
    v["user_id"] = b64(u.as_bytes()).into();
    v["user_name"] = email.into();
    v["exclude_credentials"] = exclude.into();
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct Registration {
    challenge_id: String,
    credential_id: String,
    attestation_object: String,
    client_data_json: String,
    name: Option<String>,
}

pub async fn register(
    State(s): State<AppState>,
    h: HeaderMap,
    J(b): J<Registration>,
) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    let (owner, ch) = take(&s, &b.challenge_id, "register").await?;
    if owner.as_deref() != Some(u.as_str()) {
        return Err(expired());
    }
    let cd = unb64(&b.client_data_json).ok_or_else(rejected)?;
    if !client_data_ok(&s, &cd, "webauthn.create", &ch) {
        return Err(rejected());
    }
    let att = unb64(&b.attestation_object).ok_or_else(rejected)?;
    let att: ciborium::Value = ciborium::from_reader(att.as_slice()).map_err(|_| rejected())?;
    let ad = att
        .as_map()
        .and_then(|m| m.iter().find(|(k, _)| k.as_text() == Some("authData")))
        .and_then(|(_, v)| v.as_bytes())
        .ok_or_else(rejected)?;
    let (flags, count, r) = auth_data(&s, ad).ok_or_else(rejected)?;
    // attested credential data: aaguid(16) | idLen(2) | id | COSE key
    if flags & AT == 0 || r.len() < 18 {
        return Err(rejected());
    }
    let n = u16::from_be_bytes([r[16], r[17]]) as usize;
    let id = r
        .get(18..18 + n)
        .filter(|_| (16..=1023).contains(&n))
        .ok_or_else(rejected)?;
    if unb64(&b.credential_id).as_deref() != Some(id) {
        return Err(rejected());
    }
    let key: ciborium::Value = ciborium::from_reader(&r[18 + n..]).map_err(|_| rejected())?;
    let pk = es256_point(&key).ok_or_else(|| {
        api(
            400,
            "unsupported_passkey",
            "only ES256 (P-256) passkeys are supported",
        )
    })?;
    let name = b
        .name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty() && n.chars().count() <= 60)
        .unwrap_or_else(|| "Passkey".into());
    let have: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM passkeys WHERE user_id=?")
        .bind(&u)
        .fetch_one(&s.db)
        .await?;
    if have >= MAX_PER_USER {
        return Err(api(
            409,
            "too_many_passkeys",
            "an account can have at most 10 passkeys",
        ));
    }
    let (pid, at) = (new_id("pk"), now());
    let added = sqlx::query("INSERT OR IGNORE INTO passkeys (id,user_id,credential_id,public_key,sign_count,name,created_at) VALUES (?,?,?,?,?,?,?)")
        .bind(&pid)
        .bind(&u)
        .bind(b64(id))
        .bind(&pk)
        .bind(count as i64)
        .bind(&name)
        .bind(&at)
        .execute(&s.db)
        .await?
        .rows_affected();
    if added == 0 {
        return Err(api(
            409,
            "passkey_exists",
            "this passkey is already registered",
        ));
    }
    tracing::info!("passkey registered");
    Ok(Json(
        json!({"id": pid, "name": name, "created_at": at, "last_used_at": null}),
    ))
}

pub async fn list(State(s): State<AppState>, h: HeaderMap) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id,name,created_at,last_used_at FROM passkeys WHERE user_id=? ORDER BY created_at",
    )
    .bind(&u)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, n, c, l)| json!({"id": id, "name": n, "created_at": c, "last_used_at": l}))
            .collect(),
    ))
}

pub async fn remove(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> R<StatusCode> {
    let u = auth_session(&s, &h).await?;
    let n = sqlx::query("DELETE FROM passkeys WHERE id=? AND user_id=?")
        .bind(&id)
        .bind(&u)
        .execute(&s.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(not_found("passkey"));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------- sign-in (and fresh identity for account deletion) ----------

pub async fn login_options(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    h: HeaderMap,
) -> R<Json<Value>> {
    rate_limit(
        &s,
        peer,
        &h,
        "passkey_options",
        30,
        600,
        "passkey challenges",
    )?;
    Ok(Json(new_challenge(&s, "login", None).await?))
}

#[derive(Deserialize)]
pub struct Assertion {
    challenge_id: String,
    credential_id: String,
    authenticator_data: String,
    client_data_json: String,
    signature: String,
    user_handle: Option<String>,
}

/// Verifies a `/v1/auth/passkey/options` challenge's assertion; returns the account's user id.
pub(crate) async fn verify_assertion(s: &AppState, a: &Assertion) -> R<String> {
    let (_, ch) = take(s, &a.challenge_id, "login").await?;
    let cred = unb64(&a.credential_id).ok_or_else(failed)?;
    let (pid, uid, pk, stored): (String, String, Vec<u8>, i64) = sqlx::query_as(
        "SELECT id,user_id,public_key,sign_count FROM passkeys WHERE credential_id=?",
    )
    .bind(b64(&cred))
    .fetch_optional(&s.db)
    .await?
    .ok_or_else(failed)?;
    if let Some(hdl) = &a.user_handle {
        if unb64(hdl).as_deref() != Some(uid.as_bytes()) {
            return Err(failed());
        }
    }
    let cd = unb64(&a.client_data_json).ok_or_else(failed)?;
    let ad = unb64(&a.authenticator_data).ok_or_else(failed)?;
    let sig = unb64(&a.signature).ok_or_else(failed)?;
    if !client_data_ok(s, &cd, "webauthn.get", &ch) {
        return Err(failed());
    }
    let (_, count, _) = auth_data(s, &ad).ok_or_else(failed)?;
    let msg = [&ad[..], &Sha256::digest(&cd)[..]].concat();
    UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, &pk)
        .verify(&msg, &sig)
        .map_err(|_| failed())?;
    // synced passkeys (iCloud, Google) always send 0; a counter that goes backwards means a clone
    let count = count as i64;
    if (count != 0 || stored != 0) && count <= stored {
        tracing::warn!("passkey sign-in rejected: counter did not increase");
        return Err(failed());
    }
    let ok =
        sqlx::query("UPDATE passkeys SET sign_count=?, last_used_at=? WHERE id=? AND sign_count=?")
            .bind(count)
            .bind(now())
            .bind(&pid)
            .bind(stored)
            .execute(&s.db)
            .await?
            .rows_affected();
    if ok != 1 {
        return Err(failed());
    }
    Ok(uid)
}

pub async fn login(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    h: HeaderMap,
    J(b): J<Assertion>,
) -> R<Json<Value>> {
    rate_limit(&s, peer, &h, "passkey_login", 20, 600, "passkey sign-ins")?;
    let uid = verify_assertion(&s, &b)
        .await
        .inspect_err(|e| tracing::warn!("passkey sign-in rejected: {} ({})", e.1, e.2))?;
    tracing::info!("passkey sign-in verified");
    let (email, provider) = crate::console::account(&s, &uid).await?;
    crate::routes::new_session(&s, &uid, &email, provider).await
}

/// Lets an Android app use this server's passkeys (and links), once the operator names the app.
pub async fn assetlinks(State(s): State<AppState>) -> Response {
    let Some((pkg, certs)) = &s.cfg.android_app else {
        return not_found("assetlinks").into_response();
    };
    let v = json!([{"relation": ["delegate_permission/common.handle_all_urls", "delegate_permission/common.get_login_creds"],
        "target": {"namespace": "android_app", "package_name": pkg, "sha256_cert_fingerprints": certs}}]);
    (
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        v.to_string(),
    )
        .into_response()
}
