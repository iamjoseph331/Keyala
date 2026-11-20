//! APNs push (token-based auth, HTTP/2). No-op when not configured.
//! Privacy: pushes never contain prompt text. By default they are fully generic (nothing about the
//! app, vendor, model or spend on the lock screen); KEYALA_APNS_DETAILED=1 adds app/vendor/model/estimate.
use crate::*;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use std::time::Instant;

#[derive(Clone)]
pub struct ApnsCfg {
    pub key_pem: String,
    pub key_id: String,
    pub team_id: String,
    pub topic: String,
    pub sandbox: bool,
    pub detailed: bool,
}

impl ApnsCfg {
    pub fn from_env(env: &dyn Fn(&str) -> Option<String>) -> Option<ApnsCfg> {
        Some(ApnsCfg {
            key_pem: env("KEYALA_APNS_KEY_P8")?.replace("\\n", "\n"),
            key_id: env("KEYALA_APNS_KEY_ID")?,
            team_id: env("KEYALA_APNS_TEAM_ID")?,
            topic: env("KEYALA_APNS_TOPIC").unwrap_or_else(|| "app.sorevan.masterkey".into()),
            sandbox: env("KEYALA_APNS_SANDBOX").as_deref() == Some("1"),
            detailed: env("KEYALA_APNS_DETAILED").as_deref() == Some("1"),
        })
    }
    fn host(&self) -> &'static str {
        if self.sandbox {
            "https://api.sandbox.push.apple.com"
        } else {
            "https://api.push.apple.com"
        }
    }
}

pub type JwtCache = std::sync::Mutex<Option<(Instant, String)>>;
const JWT_TTL: Duration = Duration::from_secs(50 * 60); // Apple wants a refresh between 20 and 60 minutes

/// ES256 provider token: header {alg, kid}, claims {iss: team id, iat}.
pub fn provider_jwt(cfg: &ApnsCfg, iat: i64) -> Result<String, String> {
    let mut h = Header::new(Algorithm::ES256);
    h.kid = Some(cfg.key_id.clone());
    let key = EncodingKey::from_ec_pem(cfg.key_pem.as_bytes()).map_err(|e| e.to_string())?;
    encode(&h, &json!({"iss": cfg.team_id, "iat": iat}), &key).map_err(|e| e.to_string())
}

fn jwt(s: &AppState, cfg: &ApnsCfg) -> Option<String> {
    let mut c = s.apns_jwt.lock().unwrap();
    if let Some((t, j)) = c.as_ref() {
        if t.elapsed() < JWT_TTL {
            return Some(j.clone());
        }
    }
    match provider_jwt(cfg, chrono::Utc::now().timestamp()) {
        Ok(j) => {
            *c = Some((Instant::now(), j.clone()));
            Some(j)
        }
        Err(e) => {
            tracing::error!("APNs provider token: {e}");
            None
        }
    }
}

pub fn alert_payload(
    cfg: &ApnsCfg,
    category: &str,
    title: &str,
    body: &str,
    extra: (&str, &str),
) -> Value {
    let (title, body) = if cfg.detailed {
        (title, body)
    } else {
        ("Keyala", "Open Keyala to review a request")
    };
    json!({"aps": {"alert": {"title": title, "body": body}, "sound": "default", "category": category}, extra.0: extra.1})
}

/// Fire-and-forget to all of the user's devices.
pub fn send(s: &AppState, user_id: &str, payload: Value) {
    let Some(cfg) = s.cfg.apns.clone() else {
        return;
    };
    let (s, user_id) = (s.clone(), user_id.to_string());
    tokio::spawn(async move {
        let tokens: Vec<String> =
            sqlx::query_scalar("SELECT apns_token FROM devices WHERE user_id=?")
                .bind(&user_id)
                .fetch_all(&s.db)
                .await
                .unwrap_or_default();
        let Some(jwt) = jwt(&s, &cfg) else { return };
        for t in tokens {
            let r = s
                .http
                .post(format!("{}/3/device/{t}", cfg.host()))
                .timeout(Duration::from_secs(10))
                .header("authorization", format!("bearer {jwt}"))
                .header("apns-topic", &cfg.topic)
                .header("apns-push-type", "alert")
                .header("apns-priority", "10")
                .header("content-type", "application/json")
                .body(payload.to_string())
                .send()
                .await;
            match r {
                Ok(r) if r.status().is_success() => {}
                Ok(r) => {
                    let code = r.status().as_u16();
                    let body = r.text().await.unwrap_or_default();
                    // dead tokens: 410 Unregistered, or 400 BadDeviceToken / DeviceTokenNotForTopic
                    if code == 410
                        || (code == 400
                            && (body.contains("BadDeviceToken")
                                || body.contains("DeviceTokenNotForTopic")))
                    {
                        let _ = sqlx::query("DELETE FROM devices WHERE apns_token=?")
                            .bind(&t)
                            .execute(&s.db)
                            .await;
                    } else {
                        tracing::warn!("APNs rejected push: {code} {body}");
                    }
                }
                Err(e) => tracing::warn!("APNs error: {}", e.without_url()),
            }
        }
    });
}

pub async fn request_created(s: &AppState, id: &str) {
    if s.cfg.apns.is_none() {
        return;
    }
    if let (Ok(Some(r)), Some(cfg)) = (request_row(s, id).await, s.cfg.apns.as_ref()) {
        let title = format!("{} wants to use your {} key", r.app_name(), r.vendor);
        let body = format!("{} - est. ${:.2}", r.model, usd(r.estimated_micro));
        send(
            s,
            &r.user_id,
            alert_payload(cfg, "KEYALA_REQUEST", &title, &body, ("request_id", &r.id)),
        );
    }
}

pub async fn spend_alert(s: &AppState, grant_id: &str) {
    if let (Ok(Some(g)), Some(cfg)) = (
        sqlx::query_as::<_, GrantRow>(&format!("{GRANT_SELECT} WHERE g.id=?"))
            .bind(grant_id)
            .fetch_optional(&s.db)
            .await,
        s.cfg.apns.as_ref(),
    ) {
        let title = format!("{} passed your daily spend alert", g.app_name_str());
        send(
            s,
            &g.user_id,
            alert_payload(
                cfg,
                "KEYALA_ALERT",
                &title,
                "Daily spend alert reached",
                ("grant_id", &g.id),
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};

    fn cfg(detailed: bool) -> ApnsCfg {
        ApnsCfg {
            key_pem: include_str!("../tests/fixtures/test_ec_private.pem").into(),
            key_id: "KEY123".into(),
            team_id: "TEAM456".into(),
            topic: "app.keyala.ios".into(),
            sandbox: true,
            detailed,
        }
    }

    #[test]
    fn provider_jwt_is_es256_with_kid_team_iat() {
        let j = provider_jwt(&cfg(false), 1_700_000_000).unwrap();
        let h = decode_header(&j).unwrap();
        assert_eq!(h.alg, Algorithm::ES256);
        assert_eq!(h.kid.as_deref(), Some("KEY123"));
        let mut v = Validation::new(Algorithm::ES256);
        v.required_spec_claims.clear();
        v.validate_exp = false;
        let c = decode::<Value>(
            &j,
            &DecodingKey::from_ec_pem(include_bytes!("../tests/fixtures/test_ec_public.pem"))
                .unwrap(),
            &v,
        )
        .unwrap()
        .claims;
        assert_eq!(c["iss"], "TEAM456");
        assert_eq!(c["iat"], 1_700_000_000);
    }

    #[test]
    fn payloads_are_generic_by_default_and_never_contain_prompts() {
        let p = alert_payload(
            &cfg(false),
            "KEYALA_REQUEST",
            "Demo wants to use your openai key",
            "gpt-4o - est. $0.01",
            ("request_id", "req_1"),
        );
        assert_eq!(p["aps"]["category"], "KEYALA_REQUEST");
        assert_eq!(p["request_id"], "req_1");
        let s = p.to_string();
        assert!(!s.contains("Demo") && !s.contains("gpt-4o") && !s.contains('\u{b7}'));
        let d = alert_payload(
            &cfg(true),
            "KEYALA_REQUEST",
            "Demo wants to use your openai key",
            "gpt-4o - est. $0.01",
            ("request_id", "req_1"),
        );
        assert_eq!(
            d["aps"]["alert"]["title"],
            "Demo wants to use your openai key"
        );
        assert_eq!(d["aps"]["alert"]["body"], "gpt-4o - est. $0.01");
    }

    #[test]
    fn host_follows_sandbox_flag() {
        assert!(cfg(false).host().contains("sandbox"));
    }
}
