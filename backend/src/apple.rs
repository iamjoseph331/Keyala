//! Sign in with Apple: verify the identity token (RS256) against Apple's JWKS.
use crate::*;
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use std::time::Instant;

pub const ISSUER: &str = "https://appleid.apple.com";
const TTL: Duration = Duration::from_secs(3600);
const MIN_REFETCH: Duration = Duration::from_secs(60); // unknown kids can't make us hammer Apple

pub type JwksCache = tokio::sync::Mutex<Option<(Instant, JwkSet)>>;

#[derive(Deserialize)]
pub struct Claims {
    pub sub: String,
    pub email: Option<String>,
    pub iat: Option<i64>,
}

fn invalid() -> ApiError {
    api(401, "invalid_token", "invalid Apple identity token")
}

async fn fetch(s: &AppState) -> R<JwkSet> {
    let r = s
        .http
        .get(&s.cfg.apple_jwks_url)
        .timeout(Duration::from_secs(10))
        .send()
        .await;
    match r {
        Ok(r) if r.status().is_success() => {
            let b = r.text().await.unwrap_or_default();
            serde_json::from_str::<JwkSet>(&b)
                .map_err(|_| api(502, "apple_unreachable", "bad JWKS from Apple"))
        }
        _ => Err(api(
            502,
            "apple_unreachable",
            "could not fetch Apple signing keys",
        )),
    }
}

pub async fn verify(s: &AppState, id_token: &str) -> R<Claims> {
    let kid = decode_header(id_token)
        .ok()
        .and_then(|h| h.kid)
        .ok_or_else(invalid)?;
    let mut cache = s.jwks.lock().await;
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
            *cache = Some((Instant::now(), fetch(s).await?));
        }
    }
    let jwk = cache
        .as_ref()
        .and_then(|(_, set)| set.find(&kid))
        .ok_or_else(invalid)?;
    let key = DecodingKey::from_jwk(jwk).map_err(|_| invalid())?;
    let mut v = Validation::new(Algorithm::RS256); // header alg must be RS256 (no `none`/HS256 tricks)
    v.set_issuer(&[ISSUER]);
    v.set_audience(&[s.cfg.apple_bundle_id.as_str()]);
    v.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    let c = decode::<Claims>(id_token, &key, &v)
        .map_err(|_| invalid())?
        .claims;
    if c.sub.is_empty() || c.sub.len() > 200 {
        return Err(invalid());
    }
    Ok(c)
}
