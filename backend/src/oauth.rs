//! One-tap OpenRouter: the server runs OAuth PKCE, so the new key goes from OpenRouter straight to
//! the server; the phone only opens the consent page and sees the result. OpenAI and Anthropic offer
//! no equivalent for personal keys.
use crate::routes::{last4, store_key};
use crate::*;
use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{Html, Redirect},
};

const TTL_SECS: i64 = 600;
const AUTH: &str = "https://openrouter.ai/auth";
const EXCHANGE: &str = "https://openrouter.ai/api/v1/auth/keys";

#[derive(Deserialize)]
pub struct Start {
    /// prefilled spending cap on OpenRouter's consent form (the user can change it there)
    limit_usd: Option<f64>,
}

pub async fn start(State(s): State<AppState>, h: HeaderMap, J(b): J<Start>) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    if b.limit_usd.is_some_and(|l| !(l.is_finite() && l > 0.0 && l <= 10_000.0)) {
        return Err(bad("limit_usd must be between 0 and 10000"));
    }
    let (flow, state, verifier, expires) = (format!("of_{}", rand_b64(16)), rand_b64(16), rand_b64(32), after(TTL_SECS));
    sqlx::query("INSERT INTO oauth_flows (id,state_hash,user_id,provider,verifier,created_at,expires_at) VALUES (?,?,?,'openrouter',?,?,?)")
        .bind(&flow).bind(hash_token(&state)).bind(&u).bind(&verifier).bind(now()).bind(&expires)
        .execute(&s.db).await?;
    let callback = format!("{}/oauth/openrouter/callback", s.cfg.public_url);
    let url = if s.cfg.vendor_mock {
        reqwest::Url::parse_with_params(&format!("{}/oauth/openrouter/mock", s.cfg.public_url), &[("state", &state)])
    } else {
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let mut p = vec![("callback_url", callback), ("code_challenge", challenge), ("code_challenge_method", "S256".into()), ("state", state), ("key_label", "MasterKey".into())];
        if let Some(l) = b.limit_usd {
            p.push(("limit", format!("{:.2}", l)));
        }
        reqwest::Url::parse_with_params(AUTH, &p)
    }
    .map_err(|_| api(500, "internal", "could not build the sign-in URL"))?;
    Ok(Json(json!({"flow_id": flow, "auth_url": url.to_string(), "expires_at": expires})))
}

/// The app's source of truth for an attempt: only its owner can read or cancel it.
async fn flow_json(s: &AppState, user: &str, id: &str) -> R<Value> {
    let r: Option<(String, Option<String>, Option<String>, String)> = sqlx::query_as("SELECT status, reason, last4, expires_at FROM oauth_flows WHERE id=? AND user_id=? AND provider='openrouter'")
        .bind(id).bind(user).fetch_optional(&s.db).await?;
    let (mut status, reason, last4, exp) = r.ok_or_else(|| crate::routes::not_found("flow"))?;
    if status == "pending" && exp.as_str() <= now().as_str() {
        status = "expired".into();
    }
    Ok(json!({"flow_id": id, "provider": "openrouter", "status": status, "reason": reason, "last4": last4, "expires_at": exp}))
}

pub async fn status(State(s): State<AppState>, h: HeaderMap, Path(id): Path<String>) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    Ok(Json(flow_json(&s, &u, &id).await?))
}

/// Pending only: once the code exchange has started the outcome is the server's to record.
pub async fn cancel(State(s): State<AppState>, h: HeaderMap, Path(id): Path<String>) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    sqlx::query("UPDATE oauth_flows SET status='cancelled', verifier=NULL WHERE id=? AND user_id=? AND status='pending'").bind(&id).bind(&u).execute(&s.db).await?;
    Ok(Json(flow_json(&s, &u, &id).await?))
}

/// Mock mode only: stands in for OpenRouter's consent page.
pub async fn mock_consent(State(s): State<AppState>, Query(q): Query<HashMap<String, String>>) -> R<Redirect> {
    if !s.cfg.vendor_mock {
        return Err(crate::routes::not_found("endpoint"));
    }
    let st = q.get("state").cloned().unwrap_or_default();
    let url = reqwest::Url::parse_with_params(&format!("{}/oauth/openrouter/callback", s.cfg.public_url), &[("code", "mock"), ("state", st.as_str())]).map_err(|_| bad("bad state"))?;
    Ok(Redirect::to(url.as_str()))
}

/// Back to the app: ASWebAuthenticationSession (callback scheme `masterkey`) closes on this redirect.
/// It only says which attempt finished; the app confirms the outcome with GET .../flows/{flow}.
fn done(status: &str, detail: &str) -> Response {
    let target = format!("masterkey://oauth/openrouter?status={status}{detail}");
    let msg = if status == "ok" { "OpenRouter is connected. Return to MasterKey." } else { "OpenRouter was not connected. Return to MasterKey and try again." };
    let page = format!(
        "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'><meta http-equiv=refresh content='0;url={target}'>\
         <title>MasterKey</title><body style='font-family:system-ui;text-align:center;margin:4rem 1rem'><h1>{msg}</h1><p><a href='{target}'>Open MasterKey</a></p>{}", crate::enroll::TRUST_LINK
    );
    ([(header::CACHE_CONTROL, "no-store"), (header::REFERRER_POLICY, "no-referrer")], Html(page)).into_response()
}

pub async fn callback(State(s): State<AppState>, Query(q): Query<HashMap<String, String>>) -> Response {
    let Some(state) = q.get("state") else {
        // OpenRouter sends no state when the user declines: nothing to correlate
        return done("error", "&reason=cancelled");
    };
    // pending -> exchanging exactly once; the verifier leaves the database with it
    let row: Result<Option<(String, String, Option<String>)>, _> = sqlx::query_as(
        "UPDATE oauth_flows SET status='exchanging' WHERE state_hash=? AND provider='openrouter' AND status='pending' AND expires_at>? \
         RETURNING id, user_id, verifier")
        .bind(hash_token(state)).bind(now()).fetch_optional(&s.db).await;
    let Ok(Some((flow, user, Some(verifier)))) = row else {
        return done("error", "&reason=expired");
    };
    let _ = sqlx::query("UPDATE oauth_flows SET verifier=NULL WHERE id=?").bind(&flow).execute(&s.db).await;
    let finish = |st: &'static str, reason: Option<&'static str>, l4: Option<String>| {
        let (s2, flow2) = (s.clone(), flow.clone());
        async move {
            let _ = sqlx::query("UPDATE oauth_flows SET status=?, reason=?, last4=? WHERE id=?").bind(st).bind(reason).bind(l4).bind(&flow2).execute(&s2.db).await;
        }
    };
    let Some(code) = q.get("code") else {
        finish("failed", Some("no_code"), None).await;
        return done("error", &format!("&reason=cancelled&flow={flow}"));
    };
    let key: Option<String> = if s.cfg.vendor_mock {
        Some(format!("sk-or-v1-mock{}", &hash_token(code)[..8]))
    } else {
        let body = json!({"code": code, "code_verifier": verifier, "code_challenge_method": "S256"}).to_string();
        let r = s.http.post(EXCHANGE).header("content-type", "application/json").body(body).timeout(Duration::from_secs(15)).send().await;
        match r {
            Ok(r) if r.status().is_success() => r.text().await.ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v["key"].as_str().map(String::from)),
            _ => None,
        }
    };
    let Some(key) = key.filter(|k| k.starts_with("sk-or-") && k.len() <= 512) else {
        tracing::warn!("openrouter oauth exchange failed");
        finish("failed", Some("exchange_failed"), None).await;
        return done("error", &format!("&reason=exchange_failed&flow={flow}"));
    };
    let saved = async {
        let mut c = s.db.acquire().await?;
        store_key(&s, &mut c, &user, "openrouter", &key).await
    }
    .await;
    match saved {
        Ok(k) => {
            finish("saved", None, Some(last4(&key))).await;
            emit(&s, &user, "key.updated", &k);
            done("ok", &format!("&flow={flow}"))
        }
        Err(_) => {
            finish("failed", Some("internal"), None).await;
            done("error", &format!("&reason=internal&flow={flow}"))
        }
    }
}
