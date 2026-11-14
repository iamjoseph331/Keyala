//! The metered proxy: policy check + reservation, hold-for-approval, at-most-once upstream send.
use crate::{pricing, *};
use std::time::Instant;
use axum::{
    extract::{Path, State},
    http::{header, HeaderValue},
};

// Upstream URLs are constants: nothing in a request can choose the host.
const OPENAI_URL: &str = "https://api.openai.com/v1/chat/completions";
const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const PENDING_TTL_SECS: i64 = 600;

pub async fn openai(State(s): State<AppState>, h: HeaderMap, body: Bytes) -> R<Response> {
    proxy(s, h, "openai", body).await
}
pub async fn anthropic(State(s): State<AppState>, h: HeaderMap, body: Bytes) -> R<Response> {
    proxy(s, h, "anthropic", body).await
}

enum Decision {
    Auto { micro: i64, count: i64, over_alert: bool },
    Ask,
}

/// Runs inside the caller's transaction; the caller has already taken the write lock.
async fn decide(c: &mut SqliteConnection, gid: &str, est: i64) -> R<(Decision, i64)> {
    let (mode, exp, alert, revoked, gen): (String, Option<String>, Option<i64>, Option<String>, i64) =
        sqlx::query_as("SELECT mode, expires_at, alert_micro, revoked_at, policy_gen FROM grants WHERE id=?").bind(gid).fetch_one(&mut *c).await?;
    if revoked.is_some() {
        return Err(api(401, "unauthorized", "grant revoked"));
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
    let g = auth_grant(&s, &h, vendor == "anthropic").await?;
    if g.vendor != vendor {
        return Err(api(403, "vendor_mismatch", format!("this grant is for {}, not {vendor}", g.vendor)));
    }
    let mut payload: Value = serde_json::from_slice(&body).map_err(|_| bad("body must be valid JSON"))?;
    let chk = pricing::validate(vendor, &mut payload, s.cfg.vendor_mock || s.cfg.prices_verified)?;
    let wait = h.get("x-masterkey-wait").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok()).unwrap_or(60).min(120);
    let endpoint = if vendor == "openai" { "/v1/openai/chat/completions" } else { "/v1/anthropic/messages" };

    let id = new_id("req");
    let mut tx = s.db.begin().await?;
    // write first: takes the SQLite write lock so reservation can't race another request.
    sqlx::query("UPDATE grants SET last_used_at=? WHERE id=?").bind(now()).bind(&g.id).execute(&mut *tx).await?;
    let (d, gen) = decide(&mut tx, &g.id, chk.estimate_micro).await?;
    let (status, micro, count) = match d {
        Decision::Auto { micro, count, .. } => ("approved", micro, count),
        Decision::Ask => ("pending", 0, 0),
    };
    sqlx::query("INSERT INTO requests (id,grant_id,user_id,app_id,vendor,model,endpoint,status,estimated_micro,reserved_micro,reserved_count,payload,created_at,decided_at,expires_at,policy_gen) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&id).bind(&g.id).bind(&g.user_id).bind(&g.app_id).bind(vendor).bind(chk.model.name).bind(endpoint).bind(status)
        .bind(chk.estimate_micro).bind(micro).bind(count).bind(payload.to_string()).bind(now())
        .bind((status == "approved").then(now)).bind(after(PENDING_TTL_SECS)).bind(gen)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    if matches!(d, Decision::Auto { over_alert: true, .. }) {
        emit_grant(&s, &g.id).await;
        crate::apns::spend_alert(&s, &g.id).await;
    }

    if status == "pending" {
        emit_request(&s, "request.created", &id).await;
        crate::apns::request_created(&s, &id).await;
        // ponytail: polls the DB every 100ms; swap for a per-request Notify if hold load matters.
        let deadline = Instant::now() + Duration::from_secs(wait);
        while Instant::now() < deadline && matches!(current_status(&s, &id).await?.as_str(), "pending" | "approved" | "sending") {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    } else {
        // spawned so a dropped client connection can't abandon a request mid-send.
        let (s2, id2) = (s.clone(), id.clone());
        let _ = tokio::spawn(async move { send_request(s2, id2).await }).await;
    }
    respond(&s, &id).await
}


async fn current_status(s: &AppState, id: &str) -> R<String> {
    Ok(sqlx::query_scalar("SELECT status FROM requests WHERE id=?").bind(id).fetch_one(&s.db).await?)
}

/// Turns the stored request state into the HTTP answer for the developer.
async fn respond(s: &AppState, id: &str) -> R<Response> {
    let r = request_row(s, id).await?.ok_or_else(|| api(500, "internal", "request vanished"))?;
    let mut resp = match r.status.as_str() {
        "completed" | "failed" => {
            let code = if r.status == "completed" { 200 } else { r.http_status.unwrap_or(502) as u16 };
            let mut b = (StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_GATEWAY), r.response.unwrap_or_default()).into_response();
            b.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
            b
        }
        "denied" => api(403, "masterkey_denied", "the user denied this request").into_response(),
        "expired" => api(410, "masterkey_expired", "the request expired before the user decided").into_response(),
        "uncertain" => api(504, "masterkey_uncertain", "the vendor call may or may not have completed; it was not retried. Check GET /v1/requests/{id}").into_response(),
        st => (StatusCode::ACCEPTED, Json(json!({"request_id": r.id, "status": st}))).into_response(),
    };
    resp.headers_mut().insert("x-masterkey-request-id", HeaderValue::from_str(id).unwrap());
    Ok(resp)
}

pub async fn poll_request(State(s): State<AppState>, h: HeaderMap, Path(id): Path<String>) -> R<Json<Value>> {
    let g = auth_grant(&s, &h, false).await?;
    let r = request_row(&s, &id).await?.filter(|r| r.grant_id == g.id).ok_or_else(|| api(404, "not_found", "request not found"))?;
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

pub async fn approve_request(State(s): State<AppState>, h: HeaderMap, Path(id): Path<String>, J(b): J<ApproveReq>) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    let policy = b.policy.map(PolicyIn::parse).transpose()?;
    let mut tx = s.db.begin().await?;
    // conditional UPDATE first: it's both the single-decision guard and the write lock.
    let r = sqlx::query("UPDATE requests SET status='approved', decided_at=? WHERE id=? AND user_id=? AND status='pending' AND expires_at>?")
        .bind(now()).bind(&id).bind(&u).bind(now()).execute(&mut *tx).await?;
    if r.rows_affected() != 1 {
        drop(tx);
        return Err(match request_row(&s, &id).await?.filter(|r| r.user_id == u) {
            None => api(404, "not_found", "request not found"),
            Some(r) => api(409, "not_pending", format!("request is already {}", r.status)),
        });
    }
    if let Some(mut p) = policy {
        // Choice: "next N calls" = count policy with N-1 remaining after this one, since this
        // approval consumes one call; "up to $X" = budget X minus this request's reservation.
        // A policy that cannot cover this very request is rejected (tx rolls back, stays pending).
        let (gid, est): (String, i64) = sqlx::query_as("SELECT grant_id, estimated_micro FROM requests WHERE id=?").bind(&id).fetch_one(&mut *tx).await?;
        let (mut rm, mut rc) = (0, 0);
        if let Some(c) = p.count {
            if c < 1 {
                return Err(api(400, "insufficient_allowance", "count policy must allow at least 1 call"));
            }
            rc = 1;
            p.count = Some(c - 1);
        }
        if let Some(m) = p.micro {
            if m < est {
                return Err(api(400, "insufficient_allowance", "budget is smaller than this request's estimated cost"));
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
    let r = request_row(&s, &id).await?.ok_or_else(|| api(500, "internal", "request vanished"))?;
    Ok(Json(r.json(true)))
}

pub async fn deny_request(State(s): State<AppState>, h: HeaderMap, Path(id): Path<String>) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    let r = sqlx::query("UPDATE requests SET status='denied', decided_at=? WHERE id=? AND user_id=? AND status='pending'").bind(now()).bind(&id).bind(&u).execute(&s.db).await?;
    if r.rows_affected() != 1 {
        return Err(match request_row(&s, &id).await?.filter(|r| r.user_id == u) {
            None => api(404, "not_found", "request not found"),
            Some(r) => api(409, "not_pending", format!("request is already {}", r.status)),
        });
    }
    emit_request(&s, "request.updated", &id).await;
    let r = request_row(&s, &id).await?.ok_or_else(|| api(500, "internal", "request vanished"))?;
    Ok(Json(r.json(true)))
}

/// Expires stale pending requests (10 minutes).
pub async fn sweeper(s: AppState) {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM requests WHERE status='pending' AND expires_at<=?").bind(now()).fetch_all(&s.db).await.unwrap_or_default();
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
    let won = sqlx::query("UPDATE requests SET status='sending' WHERE id=? AND status='approved' AND EXISTS (SELECT 1 FROM grants WHERE id=requests.grant_id AND revoked_at IS NULL)").bind(id).execute(&s.db).await?.rows_affected() == 1;
    if !won {
        // still approved here means the claim lost to a revoked grant: deny and give the reservation back.
        let mut tx = s.db.begin().await?;
        let r = sqlx::query("UPDATE requests SET status='denied', decided_at=? WHERE id=? AND status='approved'").bind(now()).bind(id).execute(&mut *tx).await?;
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
    sqlx::query("UPDATE grants SET request_count=request_count+1, last_used_at=? WHERE id=?").bind(now()).bind(&r.grant_id).execute(&s.db).await?;
    emit_request(s, "request.updated", id).await;

    let key_row: Option<(Vec<u8>, Vec<u8>)> = sqlx::query_as("SELECT ciphertext, nonce FROM api_keys WHERE user_id=? AND vendor=?").bind(&r.user_id).bind(&r.vendor).fetch_optional(&s.db).await?;
    let key = key_row.and_then(|(ct, n)| decrypt(&s.cfg.master_key, &key_aad(&r.user_id, &r.vendor), &ct, &n));
    let (status, http, body, actual) = match key {
        None => ("failed", 400, json!({"error": {"type": "no_key", "message": "no usable API key for this vendor"}}).to_string(), None),
        Some(key) => match call_upstream(s, &r.vendor, &key, &r.payload).await {
            Up::Done(code, body) if (200..300).contains(&code) => {
                let model = pricing::lookup(&r.vendor, &r.model);
                let actual = model.and_then(|m| serde_json::from_str::<Value>(&body).ok().and_then(|v| pricing::actual_cost(&r.vendor, m, &v)));
                // no usable usage block: actual stays NULL; reservation stays charged (see finish).
                ("completed", code, body, actual)
            }
            Up::Done(code, body) => ("failed", code, body, None),
            Up::NotSent(e) => ("failed", 502, json!({"error": {"type": "upstream_unreachable", "message": e}}).to_string(), None),
            Up::Unknown(e) => ("uncertain", 504, json!({"error": {"type": "masterkey_uncertain", "message": e}}).to_string(), None),
        },
    };
    finish(s, &r, status, http, &body, actual).await?;
    emit_request(s, "request.updated", id).await;
    emit_grant(s, &r.grant_id).await;
    Ok(())
}

async fn refund(tx: &mut SqliteConnection, r: &RequestRow) -> Result<(), sqlx::Error> {
    if r.reserved_micro > 0 {
        sqlx::query("UPDATE grants SET remaining_micro=remaining_micro+? WHERE id=? AND mode='budget' AND policy_gen=?").bind(r.reserved_micro).bind(&r.grant_id).bind(r.policy_gen).execute(&mut *tx).await?;
    }
    if r.reserved_count > 0 {
        sqlx::query("UPDATE grants SET remaining_count=remaining_count+? WHERE id=? AND mode='count' AND policy_gen=?").bind(r.reserved_count).bind(&r.grant_id).bind(r.policy_gen).execute(&mut *tx).await?;
    }
    Ok(())
}

async fn finish(s: &AppState, r: &RequestRow, status: &str, http: u16, body: &str, actual: Option<i64>) -> Result<(), sqlx::Error> {
    let mut tx = s.db.begin().await?;
    sqlx::query("UPDATE requests SET status=?, http_status=?, response=?, actual_micro=?, completed_at=? WHERE id=? AND status='sending'")
        .bind(status).bind(http as i64).bind(body).bind(actual).bind(now()).bind(&r.id).execute(&mut *tx).await?;
    match status {
        "completed" => {
            // no usage: charge the estimate and keep the reservation as is.
            sqlx::query("UPDATE grants SET spent_micro=spent_micro+? WHERE id=?").bind(actual.unwrap_or(r.estimated_micro)).bind(&r.grant_id).execute(&mut *tx).await?;
            if let (Some(a), true) = (actual, r.reserved_micro > 0) {
                // settle reservation to actual cost (refund difference; extra is deducted, floor 0);
                // only if the policy hasn't been replaced since.
                sqlx::query("UPDATE grants SET remaining_micro=MAX(0, remaining_micro+?) WHERE id=? AND mode='budget' AND policy_gen=?").bind(r.reserved_micro - a).bind(&r.grant_id).bind(r.policy_gen).execute(&mut *tx).await?;
            }
        }
        // vendor never did (billable) work: give the reservation back.
        "failed" => refund(&mut tx, r).await?,
        _ => {} // uncertain: keep the reservation, never retry
    }
    tx.commit().await
}

async fn call_upstream(s: &AppState, vendor: &str, key: &str, payload: &str) -> Up {
    if s.cfg.vendor_mock {
        return mock(vendor, payload, key);
    }
    let req = if vendor == "openai" {
        s.http.post(OPENAI_URL).header("authorization", format!("Bearer {key}"))
    } else {
        s.http.post(ANTHROPIC_URL).header("x-api-key", key).header("anthropic-version", "2023-06-01")
    };
    match req.header("content-type", "application/json").body(payload.to_string()).send().await {
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

/// Fake vendor with realistic shapes and `usage`. A message containing MK_MOCK_FAIL gets a 500.
fn mock(vendor: &str, payload: &str, key: &str) -> Up {
    if payload.contains("MK_MOCK_FAIL") {
        return Up::Done(500, json!({"error": {"type": "server_error", "message": "mock upstream failure"}}).to_string());
    }
    let model = serde_json::from_str::<Value>(payload).ok().and_then(|v| v["model"].as_str().map(String::from)).unwrap_or_default();
    let (inp, out) = (payload.len() as i64 / 4 + 1, 12);
    let text = "Hello from the MasterKey mock vendor.";
    let last4: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    let mut v = if vendor == "openai" {
        json!({"id": "chatcmpl-mock", "object": "chat.completion", "created": 0, "model": model,
               "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
               "usage": {"prompt_tokens": inp, "completion_tokens": out, "total_tokens": inp + out}})
    } else {
        json!({"id": "msg_mock", "type": "message", "role": "assistant", "model": model,
               "content": [{"type": "text", "text": text}], "stop_reason": "end_turn", "stop_sequence": null,
               "usage": {"input_tokens": inp, "output_tokens": out}})
    };
    if payload.contains("MK_MOCK_NOUSAGE") {
        v.as_object_mut().unwrap().remove("usage");
    }
    v["mock_key_last4"] = last4.into(); // mock mode only: lets tests see which key was used
    Up::Done(200, v.to_string())
}
