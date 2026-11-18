//! One-tap OpenRouter: the server runs OAuth PKCE, so the new key goes from OpenRouter straight to
//! the server; the phone only opens the consent page and sees the result. OpenAI and Anthropic offer
//! no equivalent for personal keys.
use crate::routes::{last4, store_key};
use crate::*;
use axum::{
    extract::{Query, State},
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
    let (state, verifier, expires) = (rand_b64(16), rand_b64(32), after(TTL_SECS));
    sqlx::query("INSERT INTO oauth_states (state_hash,user_id,provider,verifier,created_at,expires_at) VALUES (?,?,'openrouter',?,?,?)")
        .bind(hash_token(&state)).bind(&u).bind(&verifier).bind(now()).bind(&expires)
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
    Ok(Json(json!({"auth_url": url.to_string(), "expires_at": expires})))
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
fn done(status: &str, detail: &str) -> Response {
    let target = format!("masterkey://oauth/openrouter?status={status}&{detail}");
    let msg = if status == "ok" { "OpenRouter is connected. Return to MasterKey." } else { "OpenRouter was not connected. Return to MasterKey and try again." };
    let page = format!(
        "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'><meta http-equiv=refresh content='0;url={target}'>\
         <title>MasterKey</title><body style='font-family:system-ui;text-align:center;margin:4rem 1rem'><h1>{msg}</h1><p><a href='{target}'>Open MasterKey</a></p>"
    );
    ([(header::CACHE_CONTROL, "no-store"), (header::REFERRER_POLICY, "no-referrer")], Html(page)).into_response()
}

pub async fn callback(State(s): State<AppState>, Query(q): Query<HashMap<String, String>>) -> Response {
    let (Some(code), Some(state)) = (q.get("code"), q.get("state")) else {
        return done("error", "reason=cancelled");
    };
    // single use: the row is gone whatever happens next
    let row: Result<Option<(String, String)>, _> = sqlx::query_as("DELETE FROM oauth_states WHERE state_hash=? AND provider='openrouter' AND expires_at>? RETURNING user_id, verifier")
        .bind(hash_token(state)).bind(now()).fetch_optional(&s.db).await;
    let Ok(Some((user, verifier))) = row else {
        return done("error", "reason=expired");
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
        return done("error", "reason=exchange_failed");
    };
    let saved = async {
        let mut c = s.db.acquire().await?;
        store_key(&s, &mut c, &user, "openrouter", &key).await
    }
    .await;
    match saved {
        Ok(k) => {
            emit(&s, &user, "key.updated", &k);
            done("ok", &format!("last4={}", last4(&key)))
        }
        Err(_) => done("error", "reason=internal"),
    }
}
