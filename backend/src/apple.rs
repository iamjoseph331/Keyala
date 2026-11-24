//! Sign in with Apple / Google: verify an OpenID identity token (RS256) against the provider's JWKS.
use crate::*;
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use std::time::Instant;

pub const ISSUER: &str = "https://appleid.apple.com";
pub const GOOGLE_ISSUERS: [&str; 2] = ["https://accounts.google.com", "accounts.google.com"];
const TTL: Duration = Duration::from_secs(3600);
const MIN_REFETCH: Duration = Duration::from_secs(60); // unknown kids can't make us hammer Apple

pub type JwksCache = tokio::sync::Mutex<Option<(Instant, JwkSet)>>;

#[derive(Deserialize)]
pub struct Claims {
    pub sub: String,
    pub email: Option<String>,
    pub iat: Option<i64>,
    /// Google: bool; Apple: bool or "true"
    pub email_verified: Option<Value>,
}

impl Claims {
    pub fn email_verified(&self) -> bool {
        matches!(&self.email_verified, Some(Value::Bool(true)))
            || self.email_verified.as_ref().and_then(|v| v.as_str()) == Some("true")
    }
}

/// which identity provider a token is checked against
struct Idp<'a> {
    name: &'static str,
    unreachable: &'static str,
    jwks_url: &'a str,
    cache: &'a JwksCache,
    issuers: &'a [&'a str],
    audiences: &'a [String],
}

fn invalid(name: &str) -> ApiError {
    api(
        401,
        "invalid_token",
        format!("invalid {name} identity token"),
    )
}

async fn fetch(s: &AppState, p: &Idp<'_>) -> R<JwkSet> {
    let r = s
        .http
        .get(p.jwks_url)
        .timeout(Duration::from_secs(10))
        .send()
        .await;
    match r {
        Ok(r) if r.status().is_success() => {
            let b = r.text().await.unwrap_or_default();
            serde_json::from_str::<JwkSet>(&b)
                .map_err(|_| api(502, p.unreachable, format!("bad JWKS from {}", p.name)))
        }
        _ => Err(api(
            502,
            p.unreachable,
            format!("could not fetch {} signing keys", p.name),
        )),
    }
}

pub async fn verify(s: &AppState, id_token: &str) -> R<Claims> {
    let aud = [s.cfg.apple_bundle_id.clone()];
    let p = Idp {
        name: "Apple",
        unreachable: "apple_unreachable",
        jwks_url: &s.cfg.apple_jwks_url,
        cache: &s.jwks,
        issuers: &[ISSUER],
        audiences: &aud,
    };
    verify_with(s, &p, id_token).await
}

/// Google ID token; `aud` must be one of the operator's OAuth client IDs (iOS, Android, web).
pub async fn verify_google(s: &AppState, id_token: &str) -> R<Claims> {
    if s.cfg.google_client_ids.is_empty() {
        return Err(invalid("Google"));
    }
    let p = Idp {
        name: "Google",
        unreachable: "google_unreachable",
        jwks_url: &s.cfg.google_jwks_url,
        cache: &s.google_jwks,
        issuers: &GOOGLE_ISSUERS,
        audiences: &s.cfg.google_client_ids,
    };
    verify_with(s, &p, id_token).await
}

async fn verify_with(s: &AppState, p: &Idp<'_>, id_token: &str) -> R<Claims> {
    let invalid = || invalid(p.name);
    let kid = decode_header(id_token)
        .ok()
        .and_then(|h| h.kid)
        .ok_or_else(invalid)?;
    let mut cache = p.cache.lock().await;
    let fresh = cache.as_ref().is_some_and(|(t, _)| t.elapsed() < TTL);
    if !(fresh
        && cache
            .as_ref()
            .is_some_and(|(_, set)| set.find(&kid).is_some()))
    {
        // refetch when stale or kid unknown (Apple rotates keys), but at most once a minute.
        if cache
            .as_ref()
            .is_none_or(|(t, _)| t.elapsed() > MIN_REFETCH)
        {
            *cache = Some((Instant::now(), fetch(s, p).await?));
        }
    }
    let jwk = cache
        .as_ref()
        .and_then(|(_, set)| set.find(&kid))
        .ok_or_else(invalid)?;
    let key = DecodingKey::from_jwk(jwk).map_err(|_| invalid())?;
    let mut v = Validation::new(Algorithm::RS256); // header alg must be RS256 (no `none`/HS256 tricks)
    v.set_issuer(p.issuers);
    v.set_audience(p.audiences);
    v.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    let c = decode::<Claims>(id_token, &key, &v)
        .map_err(|_| invalid())?
        .claims;
    if c.sub.is_empty() || c.sub.len() > 200 {
        return Err(invalid());
    }
    Ok(c)
}
