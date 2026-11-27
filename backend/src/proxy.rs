//! The metered proxy: policy check + reservation, hold-for-approval, at-most-once upstream send.
use crate::{pricing, *};
use axum::{
    extract::{Path, State},
    http::{header, HeaderValue},
};
use std::time::Instant;

// Upstream URLs are constants: nothing in a request can choose the host.
const OPENAI_URL: &str = "https://api.openai.com/v1/chat/completions";
const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const OPENROUTER_MESSAGES_URL: &str = "https://openrouter.ai/api/v1/messages";
const PENDING_TTL_SECS: i64 = 600;

pub async fn openai(State(s): State<AppState>, h: HeaderMap, body: Bytes) -> R<Response> {
    proxy(s, h, "openai", body).await
}
pub async fn anthropic(State(s): State<AppState>, h: HeaderMap, body: Bytes) -> R<Response> {
    proxy(s, h, "anthropic", body).await
}
pub async fn openrouter(State(s): State<AppState>, h: HeaderMap, body: Bytes) -> R<Response> {
    proxy(s, h, "openrouter", body).await
}

enum Decision {
    Auto {
        micro: i64,
        count: i64,
        over_alert: bool,
    },
    Ask,
}

/// Runs inside the caller's transaction; the caller has already taken the write lock.
async fn decide(c: &mut SqliteConnection, gid: &str, est: i64) -> R<(Decision, i64)> {
    let (mode, exp, alert, revoked, gen, coupon): (
        String,
        Option<String>,
        Option<i64>,
        Option<String>,
        i64,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT mode, expires_at, alert_micro, revoked_at, policy_gen, coupon_id FROM grants WHERE id=?",
    )
    .bind(gid)
    .fetch_one(&mut *c)
    .await?;
    if revoked.is_some() {
        return Err(api(401, "unauthorized", "grant revoked"));
    }
    if coupon.is_some() {
        return Ok((Decision::Ask, gen)); // sponsored: every call waits for the phone, whatever the stored policy says
    }
    let n = now();
    if exp.is_some_and(|e| e.as_str() <= n.as_str()) {
        return Ok((Decision::Ask, gen)); // expired policy behaves like `ask`
    }
    let won = |r: sqlx::sqlite::SqliteQueryResult| r.rows_affected() == 1;
    Ok((match mode.as_str() {
        "count" if won(sqlx::query("UPDATE grants SET remaining_count=remaining_count-1 WHERE id=? AND remaining_count>0").bind(gid).execute(&mut *c).await?) => {
            Decision::Auto { micro: 0, count: 1, over_alert: false }
        }
        "budget" if won(sqlx::query("UPDATE grants SET remaining_micro=remaining_micro-? WHERE id=? AND remaining_micro>=?").bind(est).bind(gid).bind(est).execute(&mut *c).await?) => {
            Decision::Auto { micro: est, count: 0, over_alert: false }
        }
        "trust" => {
            let today = format!("{}T00:00:00.000Z", &n[..10]);
            let spent: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(COALESCE(actual_micro, estimated_micro)),0) FROM requests WHERE grant_id=? AND created_at>=? AND status IN ('approved','sending','completed','uncertain')")
                .bind(gid).bind(today).fetch_one(&mut *c).await?;
            Decision::Auto { micro: 0, count: 0, over_alert: alert.is_some_and(|a| spent + est > a) }
        }
        _ => Decision::Ask,
    }, gen))
}

async fn proxy(s: AppState, h: HeaderMap, vendor: &'static str, body: Bytes) -> R<Response> {
    if crate::ops::paused(&s) {
        return Err(api(
            503,
            "upstream_paused",
            "the operator paused provider calls; try again later",
        ));
    }
    let g = auth_grant(&s, &h, vendor == "anthropic").await?;
    if g.vendor != vendor {
        return Err(api(
            403,
            "vendor_mismatch",
            format!("this grant is for {}, not {vendor}", g.vendor),
        ));
    }
    let mut payload: Value =
        serde_json::from_slice(&body).map_err(|_| bad("body must be valid JSON"))?;
    let chk = pricing::validate(
        vendor,
        &mut payload,
        s.cfg.vendor_mock || s.cfg.prices_verified,
    )?;
    // Routed grant (frozen at approval): same protocol and model as the app asked for, but billed to the user's
    // OpenRouter key under the exact mapped slug and OpenRouter's price. Unknown model or option = refuse, never fall back.
    let routed = g.billing_vendor != vendor;
    let mut estimate_micro = chk.estimate_micro;
    let mut upstream_model: Option<&'static str> = None;
    if routed {
        pricing::check_routable(vendor, &payload)?;
        let slug = pricing::or_slug(vendor, chk.model.name).ok_or_else(|| {
            api(400, "model_not_routable", format!("'{}' has no verified OpenRouter mapping; this app is routed through OpenRouter", chk.model.name))
        })?;
        let om = pricing::lookup("openrouter", slug)
            .filter(|m| m.verified || s.cfg.vendor_mock || s.cfg.prices_verified)
            .ok_or_else(|| {
                api(
                    400,
                    "model_not_priced",
                    format!("OpenRouter price for '{slug}' is not verified on this server"),
                )
            })?;
        estimate_micro = chk.reprice(om);
        upstream_model = Some(slug);
    }
    let wait = h
        .get("x-keyala-wait")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60)
        .min(120);
    let endpoint = match vendor {
        "openai" => "/v1/openai/chat/completions",
        "openrouter" => "/v1/openrouter/chat/completions",
        _ => "/v1/anthropic/messages",
    };

    let idem = match h.get("idempotency-key") {
        None => None,
        Some(v) => Some(
            v.to_str()
                .ok()
                .filter(|k| {
                    (1..=128).contains(&k.len()) && k.bytes().all(|b| (0x21..0x7f).contains(&b))
                })
                .ok_or_else(|| bad("Idempotency-Key must be 1-128 visible ASCII characters"))?
                .to_string(),
        ),
    };
    let body_hash = hash_token(&String::from_utf8_lossy(&body));

    let mut id = new_id("req");
    let mut created = true;
    let dek = user_dek(&s, &g.user_id).await?; // before the write tx: it may write the users row
    let mut tx = s.db.begin().await?;
    // write first: takes the SQLite write lock so reservation can't race another request.
    sqlx::query("UPDATE grants SET last_used_at=? WHERE id=?")
        .bind(now())
        .bind(&g.id)
        .execute(&mut *tx)
        .await?;
    // a repeat of an earlier POST (same key, same body) gets that request's outcome: nothing new is created or sent.
    let dup: Option<(String, Option<String>)> = match &idem {
        Some(k) => {
            sqlx::query_as("SELECT id, body_hash FROM requests WHERE grant_id=? AND idem_key=?")
                .bind(&g.id)
                .bind(k)
                .fetch_optional(&mut *tx)
                .await?
        }
        None => None,
    };
    let mut status = "";
    let mut over_alert = false;
    if let Some((did, dhash)) = dup {
        if dhash.as_deref() != Some(body_hash.as_str()) {
            return Err(api(
                409,
                "idempotency_conflict",
                "this Idempotency-Key was already used with a different request body",
            ));
        }
        (id, created) = (did, false);
    } else {
        if let Some(cid) = &g.coupon_id {
            crate::coupons::check_use(
                &mut tx,
                cid,
                &g.user_id,
                &g.app_id,
                &g.vendor,
                Some(chk.model.name),
                estimate_micro,
            )
            .await?;
        }
        let (d, gen) = decide(&mut tx, &g.id, estimate_micro).await?;
        let (st, micro, count) = match d {
            Decision::Auto { micro, count, .. } => ("approved", micro, count),
            Decision::Ask => ("pending", 0, 0),
        };
        status = st;
        over_alert = matches!(
            d,
            Decision::Auto {
                over_alert: true,
                ..
            }
        );
        sqlx::query("INSERT INTO requests (id,grant_id,user_id,app_id,vendor,model,endpoint,status,estimated_micro,reserved_micro,reserved_count,payload,created_at,decided_at,expires_at,policy_gen,idem_key,body_hash,billing_vendor,upstream_model,coupon_id) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&id).bind(&g.id).bind(&g.user_id).bind(&g.app_id).bind(vendor).bind(chk.model.name).bind(endpoint).bind(status)
            .bind(estimate_micro).bind(micro).bind(count).bind(seal_text(&dek, &id, "payload", &payload.to_string())).bind(now())
            .bind((status == "approved").then(now)).bind(after(PENDING_TTL_SECS)).bind(gen).bind(&idem).bind(&body_hash)
            .bind(&g.billing_vendor).bind(upstream_model).bind(&g.coupon_id)
            .execute(&mut *tx).await?;
    }
    tx.commit().await?;
    if over_alert {
        emit_grant(&s, &g.id).await;
        crate::apns::spend_alert(&s, &g.id).await;
    }

    if created && status == "approved" {
        // spawned so a dropped client connection can't abandon a request mid-send.
        let (s2, id2) = (s.clone(), id.clone());
        let _ = tokio::spawn(async move { send_request(s2, id2).await }).await;
    } else {
        if created {
            emit_request(&s, "request.created", &id).await;
            crate::apns::request_created(&s, &id).await;
        }
        // ponytail: polls the DB every 100ms; swap for a per-request Notify if hold load matters.
        let deadline = Instant::now() + Duration::from_secs(wait);
        while Instant::now() < deadline
            && matches!(
                current_status(&s, &id).await?.as_str(),
                "pending" | "approved" | "sending"
            )
        {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    respond(&s, &id).await
}

async fn current_status(s: &AppState, id: &str) -> R<String> {
    Ok(sqlx::query_scalar("SELECT status FROM requests WHERE id=?")
        .bind(id)
        .fetch_one(&s.db)
        .await?)
}

/// Turns the stored request state into the HTTP answer for the developer.
async fn respond(s: &AppState, id: &str) -> R<Response> {
    let r = request_row(s, id)
        .await?
        .ok_or_else(|| api(500, "internal", "request vanished"))?;
    let mut resp = match r.status.as_str() {
        "completed" | "failed" => {
            let code = if r.status == "completed" { 200 } else { r.http_status.unwrap_or(502) as u16 };
            let mut b = (StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_GATEWAY), r.response.unwrap_or_default()).into_response();
            b.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
            b
        }
        "denied" => api(403, "keyala_denied", "the user denied this request").into_response(),
        "expired" => api(410, "keyala_expired", "the request expired before the user decided").into_response(),
        "uncertain" => api(504, "keyala_uncertain", "the vendor call may or may not have completed; it was not retried. Check GET /v1/requests/{id}").into_response(),
        st => (StatusCode::ACCEPTED, Json(json!({"request_id": r.id, "status": st}))).into_response(),
    };
    resp.headers_mut()
        .insert("x-keyala-request-id", HeaderValue::from_str(id).unwrap());
    Ok(resp)
}

pub async fn poll_request(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> R<Json<Value>> {
    let g = auth_grant(&s, &h, false).await?;
    let r = request_row(&s, &id)
        .await?
        .filter(|r| r.grant_id == g.id)
        .ok_or_else(|| api(404, "not_found", "request not found"))?;
    let mut v = json!({"request_id": r.id, "status": r.status});
    if let Some(resp) = &r.response {
        v["response"] = serde_json::from_str(resp).unwrap_or(Value::String(resp.clone()));
    }
    Ok(Json(v))
}

// ---------- user decisions ----------

#[derive(Deserialize)]
pub struct ApproveReq {
    policy: Option<PolicyIn>,
}

pub async fn approve_request(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    J(b): J<ApproveReq>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, ACT).await?;
    let policy = b.policy.map(PolicyIn::parse).transpose()?;
    let sponsored: Option<String> =
        sqlx::query_scalar("SELECT coupon_id FROM requests WHERE id=? AND user_id=?")
            .bind(&id)
            .bind(&u)
            .fetch_optional(&s.db)
            .await?
            .flatten();
    // a sponsored call is approved once; approving it can never widen what later calls may do
    if sponsored.is_some() && policy.as_ref().is_some_and(|p| p.mode != "ask") {
        return Err(crate::routes::sponsored_ask_only());
    }
    let mut tx = s.db.begin().await?;
    // conditional UPDATE first: it's both the single-decision guard and the write lock.
    let r = sqlx::query("UPDATE requests SET status='approved', decided_at=? WHERE id=? AND user_id=? AND status='pending' AND expires_at>?")
        .bind(now()).bind(&id).bind(&u).bind(now()).execute(&mut *tx).await?;
    if r.rows_affected() != 1 {
        drop(tx);
        return Err(
            match request_row(&s, &id).await?.filter(|r| r.user_id == u) {
                None => api(404, "not_found", "request not found"),
                Some(r) => api(
                    409,
                    "not_pending",
                    format!("request is already {}", r.status),
                ),
            },
        );
    }
    if sponsored.is_some() {
        // reserve this call's estimate from the coupon now; expired, revoked, mismatched or short = not approved
        crate::coupons::hold(&mut tx, &id).await?;
    }
    if let Some(mut p) = policy {
        // Choice: "next N calls" = count policy with N-1 remaining after this one, since this
        // approval consumes one call; "up to $X" = budget X minus this request's reservation.
        // A policy that cannot cover this very request is rejected (tx rolls back, stays pending).
        let (gid, est): (String, i64) =
            sqlx::query_as("SELECT grant_id, estimated_micro FROM requests WHERE id=?")
                .bind(&id)
                .fetch_one(&mut *tx)
                .await?;
        let (mut rm, mut rc) = (0, 0);
        if let Some(c) = p.count {
            if c < 1 {
                return Err(api(
                    400,
                    "insufficient_allowance",
                    "count policy must allow at least 1 call",
                ));
            }
            rc = 1;
            p.count = Some(c - 1);
        }
        if let Some(m) = p.micro {
            if m < est {
                return Err(api(
                    400,
                    "insufficient_allowance",
                    "budget is smaller than this request's estimated cost",
                ));
            }
            rm = est;
            p.micro = Some(m - rm);
        }
        set_policy(&mut tx, &gid, &p).await?;
        sqlx::query("UPDATE requests SET reserved_micro=?, reserved_count=?, policy_gen=(SELECT policy_gen FROM grants WHERE id=?) WHERE id=?").bind(rm).bind(rc).bind(&gid).bind(&id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    emit_request(&s, "request.updated", &id).await;
    if let Some(r) = request_row(&s, &id).await? {
        emit_grant(&s, &r.grant_id).await;
    }
    let (s2, id2) = (s.clone(), id.clone());
    tokio::spawn(send_request(s2, id2));
    let r = request_row(&s, &id)
        .await?
        .ok_or_else(|| api(500, "internal", "request vanished"))?;
    Ok(Json(r.json(true)))
}

pub async fn deny_request(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, ACT).await?;
    let r = sqlx::query("UPDATE requests SET status='denied', decided_at=? WHERE id=? AND user_id=? AND status='pending'").bind(now()).bind(&id).bind(&u).execute(&s.db).await?;
    if r.rows_affected() != 1 {
        return Err(
            match request_row(&s, &id).await?.filter(|r| r.user_id == u) {
                None => api(404, "not_found", "request not found"),
                Some(r) => api(
                    409,
                    "not_pending",
                    format!("request is already {}", r.status),
                ),
            },
        );
    }
    emit_request(&s, "request.updated", &id).await;
    let r = request_row(&s, &id)
        .await?
        .ok_or_else(|| api(500, "internal", "request vanished"))?;
    Ok(Json(r.json(true)))
}

/// Prompts and responses are kept this long after a request is decided, then removed (metadata stays).
pub const RETENTION_DAYS: i64 = 30;

/// Removes content of finished requests older than RETENTION_DAYS. Undecided, in-flight and
/// `uncertain` requests keep theirs (someone may still need to check them).
pub async fn purge_expired_content(s: &AppState) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query("UPDATE requests SET payload='', response=NULL, purged=1 WHERE purged=0 AND status IN ('completed','failed','denied','expired') AND COALESCE(completed_at, decided_at, created_at) < ?")
        .bind(after(-RETENTION_DAYS * 86_400))
        .execute(&s.db)
        .await?
        .rows_affected())
}

/// Expires stale pending requests (10 minutes) and applies content retention.
pub async fn sweeper(s: AppState) {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let _ = purge_expired_content(&s).await;
        let _ = crate::coupons::sweep_coupons(&s).await;
        // key enrollments never outlive their 10 minutes, and neither does an uploaded key
        let _ = sqlx::query("UPDATE enrollments SET status='expired', ciphertext=NULL, nonce=NULL WHERE expires_at<=? AND status IN ('pending','claimed','submitted')").bind(now()).execute(&s.db).await;
        // flows stay readable for a day after their 10 minutes, then go
        let _ = sqlx::query(
            "UPDATE oauth_flows SET verifier=NULL WHERE expires_at<=? AND verifier IS NOT NULL",
        )
        .bind(now())
        .execute(&s.db)
        .await;
        let _ = sqlx::query("DELETE FROM oauth_flows WHERE expires_at<=?")
            .bind(after(-86_400))
            .execute(&s.db)
            .await;
        if !crate::ops::paused(&s) {
            // approved while paused (or lost at a restart): never claimed, so sending is safe
            let held: Vec<String> = sqlx::query_scalar(
                "SELECT id FROM requests WHERE status='approved' AND decided_at<=?",
            )
            .bind(after(-5))
            .fetch_all(&s.db)
            .await
            .unwrap_or_default();
            for id in held {
                tokio::spawn(send_request(s.clone(), id));
            }
        }
        let ids: Vec<String> =
            sqlx::query_scalar("SELECT id FROM requests WHERE status='pending' AND expires_at<=?")
                .bind(now())
                .fetch_all(&s.db)
                .await
                .unwrap_or_default();
        for id in ids {
            let r = sqlx::query("UPDATE requests SET status='expired', decided_at=? WHERE id=? AND status='pending'").bind(now()).bind(&id).execute(&s.db).await;
            if r.is_ok_and(|r| r.rows_affected() == 1) {
                emit_request(&s, "request.updated", &id).await;
            }
        }
    }
}

// ---------- sending ----------

enum Up {
    Done(u16, String),
    NotSent(String),
    Unknown(String),
}

/// At-most-once: only the caller that flips approved -> sending talks to the vendor.
pub async fn send_request(s: AppState, id: String) {
    match try_send(&s, &id).await {
        Ok(()) => {}
        Err(e) => tracing::error!("send_request {id}: {e}"),
    }
}

async fn try_send(s: &AppState, id: &str) -> Result<(), sqlx::Error> {
    if crate::ops::paused(s) {
        return Ok(()); // stays `approved`; the sweeper sends it once unpaused
    }
    // a sponsored call is only sent while its coupon is still this user's, open, unexpired and unrevoked, and
    // still bound to this app and vendor: checked in the very statement that claims the send
    let won = sqlx::query("UPDATE requests SET status='sending' WHERE id=?1 AND status='approved' AND EXISTS (SELECT 1 FROM grants WHERE id=requests.grant_id AND revoked_at IS NULL) \
        AND (requests.coupon_id IS NULL OR EXISTS (SELECT 1 FROM coupons c JOIN coupon_batches b ON b.id=c.batch_id WHERE c.id=requests.coupon_id AND c.user_id=requests.user_id AND c.closed_at IS NULL AND c.expires_at>?2 \
        AND b.revoked_at IS NULL AND b.stopped_at IS NULL AND b.app_id=requests.app_id AND b.vendor=requests.vendor))").bind(id).bind(now()).execute(&s.db).await?.rows_affected() == 1;
    if !won {
        // still approved here means the claim lost to a revoked grant: deny and give the reservation back.
        let mut tx = s.db.begin().await?;
        let r = sqlx::query(
            "UPDATE requests SET status='denied', decided_at=? WHERE id=? AND status='approved'",
        )
        .bind(now())
        .bind(id)
        .execute(&mut *tx)
        .await?;
        if r.rows_affected() == 1 {
            if let Some(r) = request_row(s, id).await? {
                refund(&mut tx, &r).await?;
            }
            tx.commit().await?;
            emit_request(s, "request.updated", id).await;
        }
        return Ok(());
    }
    let r = request_row(s, id).await?.ok_or(sqlx::Error::RowNotFound)?;
    sqlx::query("UPDATE grants SET request_count=request_count+1, last_used_at=? WHERE id=?")
        .bind(now())
        .bind(&r.grant_id)
        .execute(&s.db)
        .await?;
    emit_request(s, "request.updated", id).await;

    // the key of whoever is billed: the user's OpenRouter key for a routed request, never the direct vendor key.
    // A sponsored call uses only its batch's dedicated key; there is no fallback to the user's own keys.
    let key = if let Some(cid) = &r.coupon_id {
        #[allow(clippy::type_complexity)]
        let row: Option<(String, String, Option<Vec<u8>>, Option<Vec<u8>>)> = sqlx::query_as(
            "SELECT b.id, b.vendor, b.key_ct, b.key_nonce FROM coupons c JOIN coupon_batches b ON b.id=c.batch_id WHERE c.id=?",
        )
        .bind(cid)
        .fetch_optional(&s.db)
        .await?;
        row.and_then(|(bid, vendor, ct, n)| {
            if vendor != r.billing_vendor {
                return None;
            }
            decrypt(&s.cfg.master_key, &batch_key_aad(&bid, &vendor), &ct?, &n?)
        })
    } else {
        let key_row: Option<(Vec<u8>, Vec<u8>)> =
            sqlx::query_as("SELECT ciphertext, nonce FROM api_keys WHERE user_id=? AND vendor=?")
                .bind(&r.user_id)
                .bind(&r.billing_vendor)
                .fetch_optional(&s.db)
                .await?;
        key_row.and_then(|(ct, n)| {
            decrypt(
                &s.cfg.master_key,
                &key_aad(&r.user_id, &r.billing_vendor),
                &ct,
                &n,
            )
        })
    };
    let (status, http, body, actual) = match key {
        None => (
            "failed",
            400,
            json!({"error": {"type": "no_key", "message": "no usable API key for this vendor"}})
                .to_string(),
            None,
        ),
        Some(key) => match call_upstream(s, &r, &key).await {
            Up::Done(code, body) if (200..300).contains(&code) => {
                // price from the billed vendor's table (OpenRouter slug when routed); usage fields follow the protocol
                let model = pricing::lookup(
                    &r.billing_vendor,
                    r.upstream_model.as_deref().unwrap_or(&r.model),
                );
                let actual = model.and_then(|m| {
                    serde_json::from_str::<Value>(&body)
                        .ok()
                        .and_then(|v| pricing::actual_cost(&r.vendor, m, &v))
                });
                // no usable usage block: actual stays NULL; reservation stays charged (see finish).
                ("completed", code, body, actual)
            }
            Up::Done(code, body) => ("failed", code, body, None),
            Up::NotSent(e) => (
                "failed",
                502,
                json!({"error": {"type": "upstream_unreachable", "message": e}}).to_string(),
                None,
            ),
            Up::Unknown(e) => (
                "uncertain",
                504,
                json!({"error": {"type": "keyala_uncertain", "message": e}}).to_string(),
                None,
            ),
        },
    };
    finish(s, &r, status, http, &body, actual).await?;
    emit_request(s, "request.updated", id).await;
    emit_grant(s, &r.grant_id).await;
    Ok(())
}

pub(crate) async fn refund(tx: &mut SqliteConnection, r: &RequestRow) -> Result<(), sqlx::Error> {
    if r.reserved_micro > 0 {
        sqlx::query("UPDATE grants SET remaining_micro=remaining_micro+? WHERE id=? AND mode='budget' AND policy_gen=?").bind(r.reserved_micro).bind(&r.grant_id).bind(r.policy_gen).execute(&mut *tx).await?;
    }
    if r.reserved_count > 0 {
        sqlx::query("UPDATE grants SET remaining_count=remaining_count+? WHERE id=? AND mode='count' AND policy_gen=?").bind(r.reserved_count).bind(&r.grant_id).bind(r.policy_gen).execute(&mut *tx).await?;
    }
    if r.coupon_id.is_some() {
        // sponsored calls hold coupon balance, never the user's own budget; released at most once
        crate::coupons::release(tx, &r.id).await?;
    }
    Ok(())
}

async fn finish(
    s: &AppState,
    r: &RequestRow,
    status: &str,
    http: u16,
    body: &str,
    actual: Option<i64>,
) -> Result<(), sqlx::Error> {
    // token counts kept as columns: reports must not need the (encrypted, later purged) response
    let usage = serde_json::from_str::<Value>(body)
        .ok()
        .map(|v| v["usage"].clone());
    let tokens = |a: &str, b: &str| {
        usage
            .as_ref()
            .and_then(|u| u[a].as_i64().or_else(|| u[b].as_i64()))
    };
    let sealed_body = seal_text(&user_dek(s, &r.user_id).await?, &r.id, "response", body);
    let mut tx = s.db.begin().await?;
    let finished = sqlx::query("UPDATE requests SET status=?, http_status=?, response=?, actual_micro=?, completed_at=?, input_tokens=?, output_tokens=? WHERE id=? AND status='sending'")
        .bind(status).bind(http as i64).bind(sealed_body).bind(actual).bind(now())
        .bind(tokens("prompt_tokens", "input_tokens")).bind(tokens("completion_tokens", "output_tokens"))
        .bind(&r.id).execute(&mut *tx).await?.rows_affected() == 1;
    match status {
        "completed" => {
            // no usage: charge the estimate and keep the reservation as is.
            sqlx::query("UPDATE grants SET spent_micro=spent_micro+? WHERE id=?")
                .bind(actual.unwrap_or(r.estimated_micro))
                .bind(&r.grant_id)
                .execute(&mut *tx)
                .await?;
            if r.coupon_id.is_some() && finished {
                // real cost when the provider reported usage, otherwise the held estimate is charged
                crate::coupons::settle(&mut tx, &r.id, actual).await?;
            }
            if let (Some(a), true) = (actual, r.reserved_micro > 0) {
                // settle reservation to actual cost (refund difference; extra is deducted, floor 0);
                // only if the policy hasn't been replaced since.
                sqlx::query("UPDATE grants SET remaining_micro=MAX(0, remaining_micro+?) WHERE id=? AND mode='budget' AND policy_gen=?").bind(r.reserved_micro - a).bind(&r.grant_id).bind(r.policy_gen).execute(&mut *tx).await?;
            }
        }
        // vendor never did (billable) work: give the reservation back.
        "failed" if finished => refund(&mut tx, r).await?,
        _ => {} // uncertain: keep the reservation, never retry
    }
    tx.commit().await?;
    if r.coupon_id.is_some() {
        crate::coupons::kick(s);
    }
    Ok(())
}

/// `r.vendor` is the protocol the app spoke (and the response shape); `r.billing_vendor` decides the host and key.
async fn call_upstream(s: &AppState, r: &RequestRow, key: &str) -> Up {
    let url = match (r.billing_vendor.as_str(), r.vendor.as_str()) {
        ("openrouter", "anthropic") => OPENROUTER_MESSAGES_URL, // native Anthropic Messages on OpenRouter: no conversion
        ("openrouter", _) => OPENROUTER_URL,
        ("openai", _) => OPENAI_URL,
        _ => ANTHROPIC_URL,
    };
    // routed: only the model is rewritten (exact mapped slug); everything else is the reviewed payload
    let payload = match &r.upstream_model {
        None => r.payload.clone(),
        Some(slug) => match serde_json::from_str::<Value>(&r.payload) {
            Ok(mut v) => {
                v["model"] = slug.clone().into();
                v.to_string()
            }
            Err(e) => return Up::NotSent(e.to_string()), // nothing was sent
        },
    };
    if s.cfg.vendor_mock {
        return mock(&r.vendor, &payload, key, url);
    }
    let req = if r.billing_vendor == "openrouter" {
        let b = s
            .http
            .post(url)
            .header("authorization", format!("Bearer {key}"))
            .header("x-title", "Keyala");
        if r.vendor == "anthropic" {
            b.header("anthropic-version", "2023-06-01")
        } else {
            b
        }
    } else if r.billing_vendor == "openai" {
        s.http
            .post(url)
            .header("authorization", format!("Bearer {key}"))
    } else {
        s.http
            .post(url)
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
    };
    match req
        .header("content-type", "application/json")
        .body(payload)
        .send()
        .await
    {
        // connect errors mean the request never left; anything else may have been processed.
        Err(e) if e.is_connect() => Up::NotSent(e.without_url().to_string()),
        Err(e) => Up::Unknown(e.without_url().to_string()),
        Ok(resp) => {
            let code = resp.status().as_u16();
            match resp.text().await {
                Ok(b) => Up::Done(code, b),
                Err(e) => Up::Unknown(e.without_url().to_string()),
            }
        }
    }
}

/// Fake vendor with realistic shapes and `usage`. A message containing KEYALA_MOCK_FAIL gets a 500.
fn mock(vendor: &str, payload: &str, key: &str, url: &str) -> Up {
    if payload.contains("KEYALA_MOCK_FAIL") {
        return Up::Done(
            500,
            json!({"error": {"type": "server_error", "message": "mock upstream failure"}})
                .to_string(),
        );
    }
    let model = serde_json::from_str::<Value>(payload)
        .ok()
        .and_then(|v| v["model"].as_str().map(String::from))
        .unwrap_or_default();
    // worst case on purpose: 1 token per payload byte (the reservation ceiling must still cover it)
    let (inp, out) = (payload.len() as i64, 12);
    let text = "Hello from the Keyala mock vendor.";
    let last4: String = key
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let mut v = if pricing::openai_shape(vendor) {
        json!({"id": "chatcmpl-mock", "object": "chat.completion", "created": 0, "model": model,
               "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
               "usage": {"prompt_tokens": inp, "completion_tokens": out, "total_tokens": inp + out}})
    } else {
        json!({"id": "msg_mock", "type": "message", "role": "assistant", "model": model,
               "content": [{"type": "text", "text": text}], "stop_reason": "end_turn", "stop_sequence": null,
               "usage": {"input_tokens": inp, "output_tokens": out}})
    };
    if payload.contains("KEYALA_MOCK_NOUSAGE") {
        v.as_object_mut().unwrap().remove("usage");
    }
    v["mock_key_last4"] = last4.into(); // mock mode only: lets tests see which key was used
    v["mock_url"] = url.into(); // mock mode only: which fixed upstream URL would have been called
    Up::Done(200, v.to_string())
}
