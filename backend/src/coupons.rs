//! Keyala Coupon: an app owner sponsors calls with a dedicated provider key. Recipients claim a coupon (a fixed
//! slice of the batch budget, reserved in full at claim time) and spend it through their own phone approvals.
//!
//! Money rules (integer micro-USD, one invariant per batch: total = available + Σ(remaining + held) + spent):
//! * claim moves `per_coupon` from the batch's free budget into the coupon, atomically, once per account and batch;
//! * approving a call moves its estimate from the coupon's `remaining` to `held`; success settles to the real cost
//!   (no usage block = the estimate is charged), failure/denial releases it, `uncertain` stays held;
//! * expiry/revoke returns only the unused `remaining`, once; holds of sending/uncertain calls are kept and a late
//!   completion settles against the closed coupon (leftover goes back to the batch);
//! * every movement is a ledger row with a UNIQUE key, so a retry can never apply a movement twice.
//!
//! The provider key is a dedicated batch key (AES-GCM, AAD `batch|{id}|{vendor}`). Keyala cannot make a provider
//! enforce a spend limit on it: the budget here is Keyala's own accounting. If a call ever costs more than its held
//! estimate, the overshoot is recorded as it happened and the batch stops.
use crate::routes::{not_found, rate_limit, vendor_ok};
use crate::*;
use axum::{
    extract::{ConnectInfo, Path, State},
    response::Html,
};
use std::net::SocketAddr;

const MAX_MODELS: usize = 20;
const MAX_ATTEMPTS: i64 = 5;
/// seconds to wait after the 1st, 2nd, ... failed email attempt
const BACKOFF_SECS: [i64; 4] = [60, 300, 1800, 7200];
/// Resend de-duplicates an Idempotency-Key for 24h; no attempt is made after 23h so a retry can never be a second email.
const MAIL_WINDOW_SECS: i64 = 23 * 3600;
const RESEND_URL: &str = "https://api.resend.com/emails";

/// `KC-` + 22 base64url characters (16 random bytes). Case-sensitive: codes are never normalised.
pub fn code_ok(code: &str) -> bool {
    code.len() == 25
        && code.starts_with("KC-")
        && code[3..]
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
fn new_code() -> String {
    format!("KC-{}", rand_b64(16))
}

// Error names are the native seam: the phone app switches on them. Terminal coupon errors are never 401.
pub fn e_not_found() -> ApiError {
    api(404, "coupon_not_found", "coupon not found")
}
fn e_revoked() -> ApiError {
    api(
        410,
        "coupon_revoked",
        "this coupon was revoked by its issuer",
    )
}
fn e_expired() -> ApiError {
    api(410, "coupon_expired", "this coupon has expired")
}
/// used up, too little left for this call, or the batch was stopped
fn e_exhausted() -> ApiError {
    api(
        402,
        "coupon_exhausted",
        "this coupon has no balance left for this call",
    )
}
fn e_claim_limit() -> ApiError {
    api(
        409,
        "coupon_claim_limit",
        "no coupons are left in this batch",
    )
}
pub fn e_policy() -> ApiError {
    api(
        403,
        "coupon_policy_required",
        "calls funded by a coupon are always approved one at a time on your phone",
    )
}

// ---------- rows & JSON ----------

#[derive(sqlx::FromRow)]
struct Batch {
    id: String,
    code: String,
    app_id: String,
    vendor: String,
    models: String,
    key_last4: String,
    per_coupon_micro: i64,
    total_micro: i64,
    available_micro: i64,
    spent_micro: i64,
    overshoot_micro: i64,
    claimed: i64,
    validity_days: i64,
    claim_deadline: String,
    revoked_at: Option<String>,
    stopped_at: Option<String>,
    app_name: String,
    app_homepage: Option<String>,
    app_icon_url: Option<String>,
    app_deleted: bool,
}
const BATCH_SELECT: &str = "SELECT b.id, b.code, b.app_id, b.vendor, b.models, b.key_last4, b.per_coupon_micro, b.total_micro, b.available_micro, b.spent_micro, b.overshoot_micro, (SELECT COUNT(*) FROM coupon_ledger l WHERE l.batch_id=b.id AND l.kind='claim') AS claimed, b.validity_days, b.claim_deadline, b.revoked_at, b.stopped_at, a.name AS app_name, a.homepage AS app_homepage, a.icon_url AS app_icon_url, a.deleted_at IS NOT NULL AS app_deleted FROM coupon_batches b JOIN apps a ON a.id=b.app_id";

impl Batch {
    fn models(&self) -> Vec<String> {
        serde_json::from_str(&self.models).unwrap_or_default()
    }
    fn app(&self) -> Value {
        app_json(
            &self.app_id,
            &self.app_name,
            &self.app_homepage,
            &self.app_icon_url,
        )
    }
    fn claimable_now(&self) -> bool {
        !self.app_deleted
            && self.revoked_at.is_none()
            && self.stopped_at.is_none()
            && self.claim_deadline.as_str() > now().as_str()
            && self.available_micro >= self.per_coupon_micro
    }
    fn json(&self, s: &AppState, email_enabled: bool) -> Value {
        json!({"id": self.id, "coupon_code": self.code, "claim_url": format!("{}/coupon/{}", s.cfg.public_url, self.code),
               "app": self.app(), "vendor": self.vendor, "models": self.models(),
               "per_coupon_usd": usd(self.per_coupon_micro), "total_usd": usd(self.total_micro),
               "available_usd": usd(self.available_micro), "spent_usd": usd(self.spent_micro), "claimed": self.claimed,
               "validity_days": self.validity_days,
               "claim_deadline": self.claim_deadline, "revoked_at": self.revoked_at, "stopped_at": self.stopped_at,
               "key_last4": self.key_last4, "email_enabled": email_enabled})
    }
}

async fn batch_by_code(
    db: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    code: &str,
) -> R<Batch> {
    if !code_ok(code) {
        return Err(e_not_found());
    }
    sqlx::query_as(&format!(
        "{BATCH_SELECT} WHERE b.code=? AND a.deleted_at IS NULL"
    ))
    .bind(code)
    .fetch_optional(db)
    .await?
    .ok_or_else(e_not_found)
}

async fn coupon_json(
    db: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    where_: &str,
    bind: (&str, &str),
) -> Result<Option<Value>, sqlx::Error> {
    let j: Option<String> = sqlx::query_scalar(&format!(
        "SELECT {} FROM coupons c JOIN coupon_batches b ON b.id=c.batch_id JOIN apps a ON a.id=b.app_id WHERE {where_}",
        crate::coupon_json_sql!()
    ))
    .bind(bind.0)
    .bind(bind.1)
    .fetch_optional(db)
    .await?
    .flatten();
    Ok(j.and_then(|j| serde_json::from_str(&j).ok()))
}

fn preview_json(b: &Batch, coupon: Option<Value>) -> Value {
    json!({"batch_id": b.id, "app": b.app(), "vendor": b.vendor, "models": b.models(),
           "per_coupon_usd": usd(b.per_coupon_micro), "validity_days": b.validity_days,
           "claim_deadline": b.claim_deadline, "claimable": coupon.is_none() && b.claimable_now(), "coupon": coupon})
}

/// Preview of a batch for connect screens (None when the code is unknown).
pub(crate) async fn preview_for(s: &AppState, user: &str, batch_id: &str) -> R<Value> {
    let b: Batch = sqlx::query_as(&format!("{BATCH_SELECT} WHERE b.id=?"))
        .bind(batch_id)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(e_not_found)?;
    let c = coupon_json(&s.db, "c.user_id=? AND c.batch_id=?", (user, &b.id)).await?;
    Ok(preview_json(&b, c))
}

// ---------- recipient endpoints ----------

pub async fn list_mine(State(s): State<AppState>, h: HeaderMap) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    let rows: Vec<Option<String>> = sqlx::query_scalar(&format!(
        "SELECT {} FROM coupons c JOIN coupon_batches b ON b.id=c.batch_id JOIN apps a ON a.id=b.app_id WHERE c.user_id=? ORDER BY c.claimed_at DESC",
        crate::coupon_json_sql!()
    ))
    .bind(&u)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .flatten()
            .filter_map(|j| serde_json::from_str::<Value>(&j).ok())
            .collect(),
    ))
}

pub async fn preview(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    h: HeaderMap,
    Path(code): Path<String>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    rate_limit(&s, peer, &h, "couponpreview", 60, 600, "coupon lookups")?;
    let b = batch_by_code(&s.db, &code).await?;
    let c = coupon_json(&s.db, "c.user_id=? AND c.batch_id=?", (&u, &b.id)).await?;
    Ok(Json(preview_json(&b, c)))
}

pub async fn claim(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    h: HeaderMap,
    Path(code): Path<String>,
) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    rate_limit(&s, peer, &h, "couponclaim", 30, 600, "coupon claims")?;
    let b = batch_by_code(&s.db, &code).await?;
    let mut tx = s.db.begin().await?;
    let r = claim_in_tx(&mut tx, &u, &b.id).await;
    // a refused claim (sold out) still commits nothing, but the transaction ends cleanly either way
    let id = match r {
        Ok(id) => {
            tx.commit().await?;
            id
        }
        Err(e) => {
            tx.rollback().await?;
            return Err(e);
        }
    };
    kick(&s);
    let c = coupon_json(&s.db, "c.id=? AND c.user_id=?", (&id, &u))
        .await?
        .ok_or_else(|| api(500, "internal", "coupon vanished"))?;
    Ok(Json(c))
}

/// Claims the batch for `user` or returns the coupon they already hold (same account never gets a second one,
/// not even after expiry). The first statement is a guarded write: it is the write lock and the reservation in one,
/// so two racing claims can never both take the last slice.
pub(crate) async fn claim_in_tx(
    tx: &mut SqliteConnection,
    user: &str,
    batch_id: &str,
) -> R<String> {
    let t = now();
    let won = sqlx::query(
        "UPDATE coupon_batches SET available_micro=available_micro-per_coupon_micro \
         WHERE id=?1 AND revoked_at IS NULL AND stopped_at IS NULL AND claim_deadline>?2 AND available_micro>=per_coupon_micro \
         AND EXISTS (SELECT 1 FROM apps a WHERE a.id=coupon_batches.app_id AND a.deleted_at IS NULL) \
         AND NOT EXISTS (SELECT 1 FROM coupons WHERE user_id=?3 AND batch_id=?1)",
    )
    .bind(batch_id)
    .bind(&t)
    .bind(user)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !won {
        if let Some(id) =
            sqlx::query_scalar("SELECT id FROM coupons WHERE user_id=? AND batch_id=?")
                .bind(user)
                .bind(batch_id)
                .fetch_optional(&mut *tx)
                .await?
        {
            return Ok(id);
        }
        let b: Batch = sqlx::query_as(&format!("{BATCH_SELECT} WHERE b.id=?"))
            .bind(batch_id)
            .fetch_one(&mut *tx)
            .await?;
        return Err(if b.app_deleted {
            e_not_found()
        } else if b.revoked_at.is_some() {
            e_revoked()
        } else if b.stopped_at.is_some() {
            e_exhausted()
        } else if b.claim_deadline.as_str() <= t.as_str() {
            e_expired()
        } else {
            e_claim_limit()
        });
    }
    let (per, days): (i64, i64) =
        sqlx::query_as("SELECT per_coupon_micro, validity_days FROM coupon_batches WHERE id=?")
            .bind(batch_id)
            .fetch_one(&mut *tx)
            .await?;
    let id = new_id("cpn");
    sqlx::query("INSERT INTO coupons (id,batch_id,user_id,remaining_micro,claimed_at,expires_at) VALUES (?,?,?,?,?,?)")
        .bind(&id).bind(batch_id).bind(user).bind(per).bind(&t).bind(after(days * 86_400))
        .execute(&mut *tx).await?;
    ledger(
        tx,
        batch_id,
        Some(&id),
        None,
        "claim",
        per,
        &format!("claim:{id}"),
        None,
    )
    .await?;
    after_change(tx, batch_id).await?;
    Ok(id)
}

/// Appends a movement; false when its key was already used (the movement must then not be applied again).
#[allow(clippy::too_many_arguments)]
async fn ledger(
    tx: &mut SqliteConnection,
    batch: &str,
    coupon: Option<&str>,
    request: Option<&str>,
    kind: &str,
    micro: i64,
    ukey: &str,
    model: Option<&str>,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("INSERT OR IGNORE INTO coupon_ledger (batch_id,coupon_id,request_id,kind,micro,ukey,model,created_at) VALUES (?,?,?,?,?,?,?,?)")
        .bind(batch).bind(coupon).bind(request).bind(kind).bind(micro).bind(ukey).bind(model).bind(now())
        .execute(&mut *tx).await?.rows_affected() == 1)
}

// ---------- spending: hold, settle, release ----------

/// Can `coupon_id` fund a call (or, with `model: None`, a connect) of `user` on `app`/`vendor` right now? The coupon
/// must be the user's, bound to this app and vendor (anything else is "not found": no information about other
/// people's coupons), usable, cover the model and the estimate. Nothing is reserved here.
pub(crate) async fn check_use(
    tx: &mut SqliteConnection,
    coupon_id: &str,
    user: &str,
    app: &str,
    vendor: &str,
    model: Option<&str>,
    estimate: i64,
) -> R<()> {
    #[allow(clippy::type_complexity)]
    let row: Option<(Option<String>, String, String, String, i64, String, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT c.user_id, b.app_id, b.vendor, b.models, c.remaining_micro, c.expires_at, c.closed_at, b.revoked_at, b.stopped_at FROM coupons c JOIN coupon_batches b ON b.id=c.batch_id WHERE c.id=?",
    )
    .bind(coupon_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((owner, b_app, b_vendor, models, remaining, exp, closed, revoked, stopped)) = row
    else {
        return Err(e_not_found());
    };
    if owner.as_deref() != Some(user) || b_app != app || b_vendor != vendor {
        return Err(e_not_found());
    }
    if revoked.is_some() {
        return Err(e_revoked());
    }
    if closed.is_some() || exp.as_str() <= now().as_str() {
        return Err(e_expired());
    }
    if stopped.is_some() || estimate > remaining {
        return Err(e_exhausted());
    }
    if let Some(model) = model {
        let models: Vec<String> = serde_json::from_str(&models).unwrap_or_default();
        if !models.iter().any(|m| m == model) {
            return Err(api(
                403,
                "coupon_model_not_allowed",
                format!("this coupon does not cover '{model}'"),
            ));
        }
    }
    Ok(())
}

/// Runs inside the approval transaction (the request was just flipped to `approved`). Reserves the call's estimate
/// from the coupon, once: the ledger key makes a repeat a no-op, and the guarded UPDATE re-checks the binding,
/// expiry, revoke and balance at this very moment. Failure aborts the approval (the transaction rolls back).
pub(crate) async fn hold(tx: &mut SqliteConnection, request_id: &str) -> R<()> {
    let (coupon, est, app, vendor, user, model): (Option<String>, i64, String, String, String, String) =
        sqlx::query_as(
            "SELECT coupon_id, estimated_micro, app_id, vendor, user_id, model FROM requests WHERE id=?",
        )
        .bind(request_id)
        .fetch_one(&mut *tx)
        .await?;
    let Some(coupon) = coupon else { return Ok(()) };
    let batch: String = sqlx::query_scalar("SELECT batch_id FROM coupons WHERE id=?")
        .bind(&coupon)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(e_not_found)?;
    if !ledger(
        tx,
        &batch,
        Some(&coupon),
        Some(request_id),
        "hold",
        est,
        &format!("hold:{request_id}"),
        Some(&model),
    )
    .await?
    {
        return Ok(()); // already reserved: never twice
    }
    let ok = sqlx::query(
        "UPDATE coupons SET remaining_micro=remaining_micro-?1, held_micro=held_micro+?1 \
         WHERE id=?2 AND user_id=?3 AND closed_at IS NULL AND expires_at>?4 AND remaining_micro>=?1 \
         AND EXISTS (SELECT 1 FROM coupon_batches b WHERE b.id=coupons.batch_id AND b.revoked_at IS NULL AND b.stopped_at IS NULL AND b.app_id=?5 AND b.vendor=?6)",
    )
    .bind(est).bind(&coupon).bind(&user).bind(now()).bind(&app).bind(&vendor)
    .execute(&mut *tx).await?.rows_affected() == 1;
    if ok {
        return Ok(());
    }
    // the transaction rolls back (including the ledger row): say why
    check_use(tx, &coupon, &user, &app, &vendor, None, est).await?;
    Err(e_not_found())
}

/// A held call finished with a known charge (`Some`: the provider reported usage) or without usage (`None`: the
/// held estimate is charged). True if this applied the movement (it applies at most once per call).
pub(crate) async fn settle(
    tx: &mut SqliteConnection,
    request_id: &str,
    actual: Option<i64>,
) -> Result<bool, sqlx::Error> {
    close_hold(tx, request_id, actual, false).await
}
/// A held call that never cost anything (failed, denied, revoked before sending): the hold goes back.
pub(crate) async fn release(
    tx: &mut SqliteConnection,
    request_id: &str,
) -> Result<bool, sqlx::Error> {
    close_hold(tx, request_id, Some(0), true).await
}

async fn close_hold(
    tx: &mut SqliteConnection,
    request_id: &str,
    charge: Option<i64>,
    release: bool,
) -> Result<bool, sqlx::Error> {
    let held: Option<(String, String, i64, Option<String>)> =
        sqlx::query_as("SELECT batch_id, coupon_id, micro, model FROM coupon_ledger WHERE ukey=?")
            .bind(format!("hold:{request_id}"))
            .fetch_optional(&mut *tx)
            .await?;
    let Some((batch, coupon, held, model)) = held else {
        return Ok(false);
    }; // never held: nothing to give back
    let kind = if release {
        "release"
    } else if charge.is_some() {
        "settle"
    } else {
        "estimate"
    };
    let charge = charge.unwrap_or(held).max(0);
    // one close per hold, whichever of settle / release / reconcile gets here first
    if !ledger(
        tx,
        &batch,
        Some(&coupon),
        Some(request_id),
        kind,
        charge,
        &format!("close:{request_id}"),
        model.as_deref(),
    )
    .await?
    {
        return Ok(false);
    }
    if !release {
        // one row per coupon, survives the account: lets the issuer see how many people got a working call
        ledger(
            tx,
            &batch,
            Some(&coupon),
            None,
            "first_success",
            0,
            &format!("first:{coupon}"),
            None,
        )
        .await?;
    }
    sqlx::query(
        "UPDATE coupons SET held_micro=held_micro-?1, spent_micro=spent_micro+?2 WHERE id=?3",
    )
    .bind(held)
    .bind(charge)
    .bind(&coupon)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE coupon_batches SET spent_micro=spent_micro+? WHERE id=?")
        .bind(charge)
        .bind(&batch)
        .execute(&mut *tx)
        .await?;
    let open: bool = sqlx::query_scalar("SELECT closed_at IS NULL FROM coupons WHERE id=?")
        .bind(&coupon)
        .fetch_one(&mut *tx)
        .await?;
    let leftover = held - charge;
    if leftover > 0 && open {
        sqlx::query("UPDATE coupons SET remaining_micro=remaining_micro+? WHERE id=?")
            .bind(leftover)
            .bind(&coupon)
            .execute(&mut *tx)
            .await?;
    } else if leftover > 0 {
        // already expired/revoked: its unused balance was returned earlier, so the leftover goes straight home
        sqlx::query("UPDATE coupon_batches SET available_micro=available_micro+? WHERE id=?")
            .bind(leftover)
            .bind(&batch)
            .execute(&mut *tx)
            .await?;
        ledger(
            tx,
            &batch,
            Some(&coupon),
            Some(request_id),
            "return",
            leftover,
            &format!("refund:{request_id}"),
            None,
        )
        .await?;
    } else if leftover < 0 {
        // The call cost more than the estimate we held (should not happen: estimates are ceilings). Record it as it
        // was, take it from the coupon first and then from the batch's free budget, and stop the batch.
        let over = -leftover;
        let from_coupon = if open {
            sqlx::query_scalar::<_, i64>("SELECT MIN(remaining_micro, ?) FROM coupons WHERE id=?")
                .bind(over)
                .bind(&coupon)
                .fetch_one(&mut *tx)
                .await?
        } else {
            0
        };
        sqlx::query("UPDATE coupons SET remaining_micro=remaining_micro-? WHERE id=?")
            .bind(from_coupon)
            .bind(&coupon)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE coupon_batches SET available_micro=available_micro-?1, overshoot_micro=overshoot_micro+?2, stopped_at=COALESCE(stopped_at,?3) WHERE id=?4")
            .bind(over - from_coupon).bind(over).bind(now()).bind(&batch).execute(&mut *tx).await?;
        ledger(
            tx,
            &batch,
            Some(&coupon),
            Some(request_id),
            "overshoot",
            over,
            &format!("overshoot:{request_id}"),
            None,
        )
        .await?;
        // truthful stop state: the issuer is told the batch stopped, not that its money ran out
        enqueue(tx, &batch, "stopped", 1).await?;
        tracing::error!("coupon batch stopped: a call cost more than its held estimate");
    }
    // an anonymised coupon (claimant deleted) that no longer holds anything has nothing left to remember
    sqlx::query(
        "DELETE FROM coupons WHERE id=? AND user_id IS NULL AND held_micro=0 AND remaining_micro=0",
    )
    .bind(&coupon)
    .execute(&mut *tx)
    .await?;
    after_change(tx, &batch).await?;
    Ok(true)
}

/// Accounting for a call that was left `uncertain` (or whose request row is gone with its account) and later turned
/// out to have completed (real cost, or `None`: the held estimate is charged) or not to have happened at all.
/// Only the money moves, once, by request id (the ledger holds everything needed). The request keeps its delivery
/// result: its status and body still say the outcome was uncertain; reports show the settled cost separately.
/// Internal: there is deliberately no HTTP surface for this. True if it applied.
pub async fn reconcile_hold(
    s: &AppState,
    request_id: &str,
    delivered: bool,
    actual_micro: Option<i64>,
) -> Result<bool, sqlx::Error> {
    let mut tx = s.db.begin().await?;
    // write first (a no-op touch of the batch): takes the write lock before anything is read, so the snapshot
    // this transaction decides on cannot go stale under it
    sqlx::query("UPDATE coupon_batches SET id=id WHERE id=(SELECT batch_id FROM coupon_ledger WHERE ukey=?)")
        .bind(format!("hold:{request_id}"))
        .execute(&mut *tx)
        .await?;
    // a call that is still on its way settles itself
    let live: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM requests WHERE id=? AND status IN ('approved','sending'))",
    )
    .bind(request_id)
    .fetch_one(&mut *tx)
    .await?;
    if live {
        return Ok(false);
    }
    let applied = if delivered {
        settle(&mut tx, request_id, actual_micro).await?
    } else {
        release(&mut tx, request_id).await?
    };
    if applied && delivered {
        // the grant's total reflects what this call cost, like any completed call
        let charge: i64 = sqlx::query_scalar("SELECT micro FROM coupon_ledger WHERE ukey=?")
            .bind(format!("close:{request_id}"))
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query("UPDATE grants SET spent_micro=spent_micro+? WHERE id=(SELECT grant_id FROM requests WHERE id=?)")
            .bind(charge)
            .bind(request_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    if applied {
        kick(s);
    }
    Ok(applied)
}

// ---------- closing: expiry, revoke, account deletion ----------

/// Returns a coupon's unused balance to its batch, exactly once (the `closed_at` guard and the ledger key both
/// have to agree). Held amounts stay on the coupon until their calls finish.
async fn close_coupon(tx: &mut SqliteConnection, coupon: &str) -> Result<bool, sqlx::Error> {
    if sqlx::query("UPDATE coupons SET closed_at=? WHERE id=? AND closed_at IS NULL")
        .bind(now())
        .bind(coupon)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        != 1
    {
        return Ok(false);
    }
    let (batch, rem): (String, i64) =
        sqlx::query_as("SELECT batch_id, remaining_micro FROM coupons WHERE id=?")
            .bind(coupon)
            .fetch_one(&mut *tx)
            .await?;
    if ledger(
        tx,
        &batch,
        Some(coupon),
        None,
        "return",
        rem,
        &format!("return:{coupon}"),
        None,
    )
    .await?
    {
        sqlx::query("UPDATE coupons SET remaining_micro=0 WHERE id=?")
            .bind(coupon)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE coupon_batches SET available_micro=available_micro+? WHERE id=?")
            .bind(rem)
            .bind(&batch)
            .execute(&mut *tx)
            .await?;
        after_change(tx, &batch).await?;
    }
    Ok(true)
}

/// Revokes a batch: no claims or calls any more, its key is wiped, waiting calls are denied (holds released) and
/// every coupon's unused balance returns. Calls already sending finish and settle against the closed coupon.
/// Returns the denied request ids (for events), or None if it was already revoked.
pub(crate) async fn revoke_batch(
    tx: &mut SqliteConnection,
    batch: &str,
) -> Result<Option<Vec<String>>, sqlx::Error> {
    if sqlx::query("UPDATE coupon_batches SET revoked_at=?, key_ct=NULL, key_nonce=NULL WHERE id=? AND revoked_at IS NULL")
        .bind(now()).bind(batch).execute(&mut *tx).await?.rows_affected() != 1
    {
        return Ok(None);
    }
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM requests WHERE status IN ('pending','approved') AND coupon_id IN (SELECT id FROM coupons WHERE batch_id=?)",
    )
    .bind(batch).fetch_all(&mut *tx).await?;
    let denied = crate::ops::deny_unsent(tx, ids).await?;
    let cs: Vec<String> =
        sqlx::query_scalar("SELECT id FROM coupons WHERE batch_id=? AND closed_at IS NULL")
            .bind(batch)
            .fetch_all(&mut *tx)
            .await?;
    for c in cs {
        close_coupon(tx, &c).await?;
    }
    Ok(Some(denied))
}

/// App deletion: every batch of the app is revoked in the caller's transaction.
pub(crate) async fn revoke_app_batches(
    tx: &mut SqliteConnection,
    app: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let bs: Vec<String> =
        sqlx::query_scalar("SELECT id FROM coupon_batches WHERE app_id=? AND revoked_at IS NULL")
            .bind(app)
            .fetch_all(&mut *tx)
            .await?;
    let mut denied = vec![];
    for b in bs {
        denied.extend(revoke_batch(tx, &b).await?.unwrap_or_default());
    }
    Ok(denied)
}

/// Account deletion, in the deletion transaction and before the user's rows go: waiting calls are denied (holds
/// released) and unused balances return. A call whose outcome is unknown (`uncertain`) is NOT classified as spent:
/// its hold stays reserved, on a coupon that loses its user (user_id NULL) and keeps only money, no identity; the
/// request row and its content go with the account, and `reconcile_hold` can still settle it by request id. A deleted
/// issuer's batches are revoked (key wiped, other users' waiting calls denied) and every stored mail address erased.
/// Returns request ids of other users' calls that were denied.
pub(crate) async fn detach_user(
    tx: &mut SqliteConnection,
    user: &str,
) -> Result<Vec<String>, sqlx::Error> {
    // as a recipient
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM requests WHERE user_id=? AND coupon_id IS NOT NULL AND status IN ('pending','approved')",
    )
    .bind(user).fetch_all(&mut *tx).await?;
    crate::ops::deny_unsent(tx, ids).await?;
    let cs: Vec<String> =
        sqlx::query_scalar("SELECT id FROM coupons WHERE user_id=? AND closed_at IS NULL")
            .bind(user)
            .fetch_all(&mut *tx)
            .await?;
    for c in cs {
        close_coupon(tx, &c).await?;
    }
    // what is still held belongs to nobody now; the rest of the user's coupons are deleted with the account
    sqlx::query("UPDATE coupons SET user_id=NULL WHERE user_id=? AND held_micro>0")
        .bind(user)
        .execute(&mut *tx)
        .await?;
    // as an issuer
    let bs: Vec<String> = sqlx::query_scalar("SELECT id FROM coupon_batches WHERE owner_user_id=?")
        .bind(user)
        .fetch_all(&mut *tx)
        .await?;
    let mut denied = vec![];
    for b in bs {
        denied.extend(revoke_batch(tx, &b).await?.unwrap_or_default());
        sqlx::query("UPDATE coupon_outbox SET status=CASE WHEN status IN ('pending','sending') THEN 'skipped' ELSE status END,                      last_error=CASE WHEN status IN ('pending','sending') THEN 'issuer_deleted' ELSE last_error END,                      from_addr=NULL, recipient=NULL, subject=NULL, body=NULL WHERE batch_id=?")
            .bind(&b).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE coupon_batches SET owner_user_id=NULL WHERE owner_user_id=?")
        .bind(user)
        .execute(&mut *tx)
        .await?;
    Ok(denied)
}

/// Restart-safe sweep, safe to run any time and from several places: waiting calls of expired/revoked/stopped
/// coupons are cancelled (holds released), then unused balances of expired or revoked coupons return once.
pub async fn sweep_coupons(s: &AppState) -> Result<(), sqlx::Error> {
    let t = now();
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT r.id FROM requests r JOIN coupons c ON c.id=r.coupon_id JOIN coupon_batches b ON b.id=c.batch_id \
         WHERE r.status IN ('pending','approved') AND (c.expires_at<=?1 OR c.closed_at IS NOT NULL OR b.revoked_at IS NOT NULL OR b.stopped_at IS NOT NULL)",
    )
    .bind(&t).fetch_all(&s.db).await?;
    if !ids.is_empty() {
        let mut tx = s.db.begin().await?;
        let denied = crate::ops::deny_unsent(&mut tx, ids).await?;
        tx.commit().await?;
        for id in denied {
            emit_request(s, "request.updated", &id).await;
        }
    }
    let due: Vec<String> = sqlx::query_scalar(
        "SELECT c.id FROM coupons c JOIN coupon_batches b ON b.id=c.batch_id WHERE c.closed_at IS NULL AND (c.expires_at<=?1 OR b.revoked_at IS NOT NULL)",
    )
    .bind(&t).fetch_all(&s.db).await?;
    for c in due {
        let mut tx = s.db.begin().await?;
        close_coupon(&mut tx, &c).await?;
        tx.commit().await?;
    }
    kick(s);
    Ok(())
}

pub(crate) async fn startup(s: &AppState) -> Result<(), sqlx::Error> {
    // A crash mid-send: that attempt already counted and may or may not have gone out. Sending the same message with
    // the same Idempotency-Key again is the safe side, but only inside the attempt budget and the provider's 24h
    // de-duplication window; anything older is finished instead.
    sqlx::query("UPDATE coupon_outbox SET status='pending' WHERE status='sending' AND attempts<?1 AND first_attempt_at>?2")
        .bind(MAX_ATTEMPTS).bind(after(-MAIL_WINDOW_SECS)).execute(&s.db).await?;
    expire_outbox(&s.db).await?;
    match transport(s) {
        Some(t) => tracing::info!("coupon email: {t}"),
        None => {
            tracing::info!("coupon email is off (set KEYALA_RESEND_API_KEY and KEYALA_EMAIL_FROM)")
        }
    }
    sweep_coupons(s).await
}

// ---------- notifications ----------

/// After any change to a batch's money: records the two transition events once per episode. Capacity: the free
/// budget fell below one full coupon, so no claim slot is left (claimed coupons stay valid). Exhausted: the whole
/// budget was actually spent (settled or charged at its estimate): money that is merely held or unallocated does
/// not count. Recovering clears the flag, so a later fall is a new episode with a new dedup key. (A batch stopped by
/// an overshoot has its own `stopped` event, see `close_hold`.)
async fn after_change(tx: &mut SqliteConnection, batch: &str) -> Result<(), sqlx::Error> {
    let (avail, per, total, spent, cap, cap_ep, exh, exh_ep, revoked): (i64, i64, i64, i64, i64, i64, i64, i64, Option<String>) = sqlx::query_as(
        "SELECT available_micro, per_coupon_micro, total_micro, spent_micro, cap_low, cap_episode, exh_flag, exh_episode, revoked_at FROM coupon_batches WHERE id=?",
    )
    .bind(batch).fetch_one(&mut *tx).await?;
    let stopped: bool =
        sqlx::query_scalar("SELECT stopped_at IS NOT NULL FROM coupon_batches WHERE id=?")
            .bind(batch)
            .fetch_one(&mut *tx)
            .await?;
    if revoked.is_some() {
        return Ok(());
    }
    let low = avail < per;
    let spent_out = spent >= total;
    if low != (cap != 0) {
        let ep = cap_ep + low as i64;
        sqlx::query("UPDATE coupon_batches SET cap_low=?, cap_episode=? WHERE id=?")
            .bind(low as i64)
            .bind(ep)
            .bind(batch)
            .execute(&mut *tx)
            .await?;
        if low {
            enqueue(tx, batch, "capacity", ep).await?;
        }
    }
    if spent_out != (exh != 0) {
        let ep = exh_ep + spent_out as i64;
        sqlx::query("UPDATE coupon_batches SET exh_flag=?, exh_episode=? WHERE id=?")
            .bind(spent_out as i64)
            .bind(ep)
            .bind(batch)
            .execute(&mut *tx)
            .await?;
        // a stopped batch already told the issuer (`stopped`); a second "fully spent" mail would contradict or repeat it
        if spent_out && !stopped {
            enqueue(tx, batch, "exhausted", ep).await?;
        }
    }
    Ok(())
}

/// Only the event is stored here. The recipient and text are bound (once) when the first delivery attempt starts.
async fn enqueue(
    tx: &mut SqliteConnection,
    batch: &str,
    kind: &str,
    episode: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT OR IGNORE INTO coupon_outbox (id,batch_id,kind,dedup_key,next_attempt_at,created_at) VALUES (?,?,?,?,?,?)")
        .bind(new_id("out")).bind(batch).bind(kind).bind(format!("{kind}:{batch}:{episode}")).bind(now()).bind(now())
        .execute(&mut *tx).await?;
    Ok(())
}

/// Delivery runs outside request handling so a slow or failing mail provider never blocks money movement.
pub(crate) fn kick(s: &AppState) {
    let s = s.clone();
    tokio::spawn(async move { process_outbox(&s).await });
}

fn transport(s: &AppState) -> Option<&'static str> {
    if s.cfg.email_mock {
        Some("mock")
    } else if s.cfg.resend_api_key.is_some() && s.cfg.email_from.is_some() {
        Some("resend")
    } else {
        None
    }
}

/// Test transport (KEYALA_EMAIL_MOCK=1): nothing leaves the process. Every attempt is recorded exactly as it
/// would be sent (Idempotency-Key + JSON body); `outcomes` scripts the provider's HTTP status per attempt (default 200).
#[derive(Default)]
pub struct MailMock {
    pub outcomes: std::collections::VecDeque<u16>,
    pub sent: Vec<SentMail>,
}
#[derive(Clone, Debug)]
pub struct SentMail {
    pub idempotency_key: String,
    pub body: Value,
}

fn plain_address(e: &str) -> bool {
    let Some((local, domain)) = e.split_once('@') else {
        return false;
    };
    e.len() <= 200
        && !local.is_empty()
        && domain.contains('.')
        && !domain.contains('@')
        && !domain.ends_with(".invalid")
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && e.bytes()
            .all(|b| b.is_ascii_graphic() && !b"<>,;\"()[]\\".contains(&b))
}
/// The issuer's verified address, or None. Apple/Google accounts: only the address of a verified identity token
/// (`users.notify_email`, written by sign-in; NULL for accounts that have not signed in since, so they fail closed).
/// Dev-login accounts have no verified address and only count under the mock transport. Placeholders (`.invalid`)
/// and anything that is not one plain address are refused. The client can never choose a recipient.
async fn recipient(s: &AppState, owner: Option<&str>) -> Option<String> {
    let (email, notify, federated): (String, Option<String>, bool) = sqlx::query_as(
        "SELECT email, notify_email, apple_sub IS NOT NULL OR google_sub IS NOT NULL FROM users WHERE id=?",
    )
    .bind(owner?)
    .fetch_optional(&s.db)
    .await
    .ok()??;
    let addr = if federated {
        notify
    } else if s.cfg.email_mock {
        Some(email)
    } else {
        None
    };
    addr.filter(|a| plain_address(a))
}

async fn email_enabled(s: &AppState, owner: Option<&str>) -> bool {
    transport(s).is_some() && recipient(s, owner).await.is_some()
}

/// Rows that can no longer be sent safely (attempt budget used up, or past the provider's de-duplication window)
/// are finished with their payload erased, never retried.
async fn expire_outbox(db: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE coupon_outbox SET status='failed', last_error='gave_up', from_addr=NULL, recipient=NULL, subject=NULL, body=NULL \
                 WHERE status IN ('pending','sending') AND (attempts>=?1 OR first_attempt_at<=?2)")
        .bind(MAX_ATTEMPTS).bind(after(-MAIL_WINDOW_SECS)).execute(db).await?;
    Ok(())
}

pub async fn process_outbox(s: &AppState) {
    // only rows idle in `pending` are expired here: a `sending` row belongs to a worker that is still running
    let _ = sqlx::query("UPDATE coupon_outbox SET status='failed', last_error='gave_up', from_addr=NULL, recipient=NULL, subject=NULL, body=NULL \
                         WHERE status='pending' AND (attempts>=?1 OR first_attempt_at<=?2)")
        .bind(MAX_ATTEMPTS).bind(after(-MAIL_WINDOW_SECS)).execute(&s.db).await;
    let due: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM coupon_outbox WHERE status='pending' AND next_attempt_at<=? ORDER BY created_at LIMIT 20",
    )
    .bind(now()).fetch_all(&s.db).await.unwrap_or_default();
    for id in due {
        let _ = deliver(s, &id).await;
    }
}

/// Atomically takes a row for one attempt: it must be pending, due, inside the attempt budget and inside the
/// de-duplication window, all in the one statement, so a stale worker holding an old list can neither skip a
/// backoff nor exceed the limits. Returns the attempt number it now owns.
async fn claim_outbox(db: &sqlx::SqlitePool, id: &str) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar("UPDATE coupon_outbox SET status='sending', attempts=attempts+1, first_attempt_at=COALESCE(first_attempt_at,?2) \
                        WHERE id=?1 AND status='pending' AND next_attempt_at<=?2 AND attempts<?3 AND (first_attempt_at IS NULL OR first_attempt_at>?4) RETURNING attempts")
        .bind(id).bind(now()).bind(MAX_ATTEMPTS).bind(after(-MAIL_WINDOW_SECS))
        .fetch_optional(db).await
}

enum Sent {
    Ok(&'static str),
    Retry(String),
    Permanent(String),
    Skip(&'static str),
}
/// Writes the result of attempt `attempt`. Guarded by `status='sending' AND attempts=attempt`: a result that arrives
/// after the row was skipped (issuer deleted), finished or re-claimed by a later attempt changes nothing.
/// A finished row keeps no address, sender or text.
async fn finish_outbox(
    db: &sqlx::SqlitePool,
    id: &str,
    attempt: i64,
    r: Sent,
) -> Result<(), sqlx::Error> {
    let (status, provider, err, next): (&str, Option<&str>, Option<String>, Option<String>) =
        match r {
            Sent::Ok(p) => ("sent", Some(p), None, None),
            Sent::Retry(e) if attempt >= MAX_ATTEMPTS => ("failed", None, Some(e), None),
            Sent::Retry(e) => {
                let wait = BACKOFF_SECS[(attempt as usize - 1).min(BACKOFF_SECS.len() - 1)];
                ("pending", None, Some(e), Some(after(wait)))
            }
            Sent::Permanent(e) => ("failed", None, Some(e), None),
            Sent::Skip(why) => ("skipped", None, Some(why.to_string()), None),
        };
    let keep = status == "pending";
    sqlx::query("UPDATE coupon_outbox SET status=?1, provider=COALESCE(?2,provider), last_error=?3, next_attempt_at=COALESCE(?4,next_attempt_at), \
                 sent_at=CASE WHEN ?1='sent' THEN ?5 END, \
                 from_addr=CASE WHEN ?6 THEN from_addr END, recipient=CASE WHEN ?6 THEN recipient END, subject=CASE WHEN ?6 THEN subject END, body=CASE WHEN ?6 THEN body END \
                 WHERE id=?7 AND status='sending' AND attempts=?8")
        .bind(status).bind(provider).bind(err).bind(next).bind(now()).bind(keep).bind(id).bind(attempt)
        .execute(db).await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct Out {
    kind: String,
    provider: Option<String>,
    from_addr: Option<String>,
    recipient: Option<String>,
    subject: Option<String>,
    body: Option<String>,
    code: String,
    app_name: String,
    owner: Option<String>,
    revoked: bool,
}

async fn deliver(s: &AppState, id: &str) -> Result<(), sqlx::Error> {
    let Some(attempt) = claim_outbox(&s.db, id).await? else {
        return Ok(());
    };
    let skip = |why: &'static str| finish_outbox(&s.db, id, attempt, Sent::Skip(why));
    let mut row: Out = sqlx::query_as(
        "SELECT o.kind, o.provider, o.from_addr, o.recipient, o.subject, o.body, b.code, a.name AS app_name, b.owner_user_id AS owner, b.revoked_at IS NOT NULL AS revoked \
         FROM coupon_outbox o JOIN coupon_batches b ON b.id=o.batch_id JOIN apps a ON a.id=b.app_id WHERE o.id=?",
    )
    .bind(id).fetch_one(&s.db).await?;
    let Some(provider) = transport(s) else {
        return skip("email_disabled").await;
    };
    if row.revoked {
        return skip("batch_revoked").await;
    }
    // the address must be verified NOW, on every attempt (marker removed, address changed, issuer deleted => no mail)
    let Some(current) = recipient(s, row.owner.as_deref()).await else {
        return skip("no_verified_recipient").await;
    };
    if row.recipient.is_some() {
        // a bound message stays with the transport it was bound under (mock-bound mail never reaches a real provider)
        if row.provider.as_deref() != Some(provider) {
            return skip("provider_changed").await;
        }
        if row.recipient.as_deref() != Some(current.as_str()) {
            return skip("recipient_changed").await;
        }
    } else {
        // first attempt: bind the message and the transport. From here on it never changes (a trigger enforces the text).
        let to = current;
        let from = s
            .cfg
            .email_from
            .clone()
            .unwrap_or_else(|| "Keyala <coupons@mock.invalid>".into());
        let (subject, text) = compose(&row.kind, &row.code, &row.app_name);
        let bound = sqlx::query("UPDATE coupon_outbox SET from_addr=?, recipient=?, subject=?, body=?, provider=? WHERE id=? AND status='sending' AND attempts=? AND recipient IS NULL")
            .bind(&from).bind(&to).bind(&subject).bind(&text).bind(provider).bind(id).bind(attempt)
            .execute(&s.db).await?.rows_affected() == 1;
        if !bound {
            return Ok(()); // the row was skipped or taken over meanwhile
        }
        (row.from_addr, row.recipient, row.subject, row.body) =
            (Some(from), Some(to), Some(subject), Some(text));
    }
    let (Some(from), Some(to), Some(subject), Some(text)) =
        (row.from_addr, row.recipient, row.subject, row.body)
    else {
        return skip("no_payload").await;
    };
    let body = json!({"from": from, "to": [to], "subject": subject, "text": text});
    let status = if provider == "mock" {
        let mut m = s.mail_mock.lock().unwrap_or_else(|e| e.into_inner());
        m.sent.push(SentMail {
            idempotency_key: id.to_string(),
            body,
        });
        Ok(m.outcomes.pop_front().unwrap_or(200))
    } else {
        send_resend(s, id, &body).await
    };
    let r = match status {
        Ok(200..=299) => Sent::Ok(provider),
        // rate limit, an idempotent request still in flight, provider trouble: same message again later
        Ok(c @ (409 | 429 | 500..=599)) => Sent::Retry(format!("provider answered {c}")),
        Ok(c) => Sent::Permanent(format!("provider answered {c}")),
        Err(e) => Sent::Retry(e),
    };
    finish_outbox(&s.db, id, attempt, r).await
}

/// Fixed text: the app name and batch code the issuer chose, nothing about users, prompts or keys.
fn compose(kind: &str, code: &str, app: &str) -> (String, String) {
    let app: String = app.chars().filter(|c| !c.is_control()).take(100).collect();
    let (subject, line) = match kind {
        "capacity" => ("Keyala coupons: not enough budget left for new claims",
            "has less free budget than one full coupon, so nobody new can claim it. Coupons that were already claimed stay valid."),
        "stopped" => ("Keyala coupons: batch stopped",
            "was stopped because a call unexpectedly cost more than Keyala held for it. The batch accepts no more claims or calls. Open the report for the spent and overshoot amounts."),
        _ => ("Keyala coupons: budget fully spent",
            "has spent its whole budget on completed calls."),
    };
    (subject.into(), format!("Your Keyala coupon batch {code} for the app \"{app}\" {line}\n\nOpen the Keyala console to create a new batch.\n"))
}

/// The only outbound mail call: a fixed URL, bearer key from the environment, the stored message and its
/// Idempotency-Key (the outbox id). Errors carry no URL, key, address or text.
async fn send_resend(s: &AppState, key: &str, body: &Value) -> Result<u16, String> {
    let Some(token) = &s.cfg.resend_api_key else {
        return Err("email not configured".into());
    };
    match s
        .http
        .post(RESEND_URL)
        .bearer_auth(token)
        .header("content-type", "application/json")
        .header("Idempotency-Key", key)
        .timeout(Duration::from_secs(10))
        .body(body.to_string())
        .send()
        .await
    {
        Ok(r) => Ok(r.status().as_u16()),
        Err(e) => Err(format!(
            "send failed ({})",
            if e.is_timeout() { "timeout" } else { "network" }
        )),
    }
}

// ---------- issuer endpoints ----------

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

pub async fn issuer_list(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(app): Path<String>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    owned_app(&s, &u, &app).await?;
    let rows: Vec<Batch> = sqlx::query_as(&format!(
        "{BATCH_SELECT} WHERE b.app_id=? ORDER BY b.created_at DESC"
    ))
    .bind(&app)
    .fetch_all(&s.db)
    .await?;
    let en = email_enabled(&s, Some(&u)).await;
    Ok(Json(rows.iter().map(|b| b.json(&s, en)).collect()))
}

#[derive(Deserialize)]
pub struct NewBatch {
    vendor: String,
    models: Vec<String>,
    api_key: String,
    per_coupon_usd: f64,
    total_usd: f64,
    validity_days: Option<i64>,
    claim_deadline: String,
}

fn micro(usd: f64, what: &str) -> R<i64> {
    if usd.is_finite() && (0.001..=10_000.0).contains(&usd) {
        Ok((usd * 1e6).round() as i64)
    } else {
        Err(bad(format!("{what} must be between 0.001 and 10000 USD")))
    }
}

pub async fn issuer_create(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    h: HeaderMap,
    Path(app): Path<String>,
    J(b): J<NewBatch>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    owned_app(&s, &u, &app).await?;
    rate_limit(
        &s,
        peer,
        &h,
        "couponcreate",
        20,
        3600,
        "coupon batches created",
    )?;
    vendor_ok(&b.vendor)?;
    let per = micro(b.per_coupon_usd, "per_coupon_usd")?;
    let total = micro(b.total_usd, "total_usd")?;
    if total < per {
        return Err(bad("total_usd must cover at least one coupon"));
    }
    let days = b.validity_days.unwrap_or(7);
    if !(1..=365).contains(&days) {
        return Err(bad("validity_days must be 1-365"));
    }
    let deadline = chrono::DateTime::parse_from_rfc3339(&b.claim_deadline)
        .map_err(|_| bad("claim_deadline must be RFC3339"))?
        .with_timezone(&chrono::Utc)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    if deadline.as_str() <= now().as_str() || deadline.as_str() > after(2 * 365 * 86_400).as_str() {
        return Err(bad(
            "claim_deadline must be in the future and within two years",
        ));
    }
    // allowlisted, priced models only (same table and gate as live calls); duplicates collapse
    let mut models: Vec<String> = vec![];
    for m in &b.models {
        let ok = pricing::lookup(&b.vendor, m)
            .is_some_and(|x| x.verified || s.cfg.vendor_mock || s.cfg.prices_verified);
        if !ok {
            return Err(bad(format!(
                "model '{m}' is not available for {}",
                b.vendor
            )));
        }
        if !models.contains(m) {
            models.push(m.clone());
        }
    }
    if models.is_empty() || models.len() > MAX_MODELS {
        return Err(bad(format!(
            "models: 1-{MAX_MODELS} model names from the supported list"
        )));
    }
    // same shape and liveness check as personal keys (admin keys refused, rejected keys refused)
    let (key, _checked) = crate::routes::check_key(&s, &b.vendor, &b.api_key).await?;
    let id = new_id("cpb");
    let (ct, nonce) = encrypt(&s.cfg.master_key, &batch_key_aad(&id, &b.vendor), &key);
    let row = NewRow {
        id: &id,
        app: &app,
        owner: &u,
        vendor: &b.vendor,
        models: serde_json::to_string(&models).unwrap_or_default(),
        key_ct: ct,
        key_nonce: nonce,
        key_last4: crate::routes::last4(&key),
        per,
        total,
        days,
        deadline: &deadline,
    };
    // the provider check above was awaited: the app or the account may be gone by now, so the insert itself
    // re-checks both (404 then), and a code collision is the only reason to try again
    for _ in 0..5 {
        match insert_batch(&s.db, &row, &new_code()).await? {
            Some(true) => {
                let row: Batch = sqlx::query_as(&format!("{BATCH_SELECT} WHERE b.id=?"))
                    .bind(&id)
                    .fetch_one(&s.db)
                    .await?;
                return Ok(Json(row.json(&s, email_enabled(&s, Some(&u)).await)));
            }
            Some(false) => continue,
            None => return Err(not_found("app")),
        }
    }
    Err(api(500, "internal", "could not allocate a coupon code"))
}

struct NewRow<'a> {
    id: &'a str,
    app: &'a str,
    owner: &'a str,
    vendor: &'a str,
    models: String,
    key_ct: Vec<u8>,
    key_nonce: Vec<u8>,
    key_last4: String,
    per: i64,
    total: i64,
    days: i64,
    deadline: &'a str,
}

/// One statement: the batch exists only if, at this very moment, the app is live and owned by a live account.
/// Some(true) inserted, Some(false) the code was taken, None the app or its owner is gone.
async fn insert_batch(
    db: &sqlx::SqlitePool,
    r: &NewRow<'_>,
    code: &str,
) -> Result<Option<bool>, sqlx::Error> {
    let n = sqlx::query("INSERT OR IGNORE INTO coupon_batches (id,code,app_id,owner_user_id,vendor,models,key_ct,key_nonce,key_last4,per_coupon_micro,total_micro,available_micro,validity_days,claim_deadline,created_at) \
         SELECT ?,?,a.id,u.id,?,?,?,?,?,?,?,?,?,?,? FROM apps a JOIN users u ON u.id=a.owner_user_id WHERE a.id=? AND u.id=? AND a.deleted_at IS NULL")
        .bind(r.id).bind(code).bind(r.vendor).bind(&r.models).bind(&r.key_ct).bind(&r.key_nonce).bind(&r.key_last4)
        .bind(r.per).bind(r.total).bind(r.total).bind(r.days).bind(r.deadline).bind(now())
        .bind(r.app).bind(r.owner)
        .execute(db).await?.rows_affected();
    if n == 1 {
        return Ok(Some(true));
    }
    let live: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM apps a JOIN users u ON u.id=a.owner_user_id WHERE a.id=? AND u.id=? AND a.deleted_at IS NULL)")
        .bind(r.app).bind(r.owner).fetch_one(db).await?;
    Ok(live.then_some(false))
}

async fn owned_batch(s: &AppState, user: &str, app: &str, batch: &str) -> R<Batch> {
    owned_app(s, user, app).await?;
    sqlx::query_as(&format!("{BATCH_SELECT} WHERE b.id=? AND b.app_id=?"))
        .bind(batch)
        .bind(app)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(|| not_found("coupon batch"))
}

pub async fn issuer_revoke(
    State(s): State<AppState>,
    h: HeaderMap,
    Path((app, batch)): Path<(String, String)>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    owned_batch(&s, &u, &app, &batch).await?;
    let mut tx = s.db.begin().await?;
    let denied = revoke_batch(&mut tx, &batch).await?; // write first: the write lock
    tx.commit().await?;
    for id in denied.unwrap_or_default() {
        emit_request(&s, "request.updated", &id).await;
    }
    let b = owned_batch(&s, &u, &app, &batch).await?;
    Ok(Json(b.json(&s, email_enabled(&s, Some(&u)).await)))
}

/// Aggregates only: counts and money. No prompts or responses, no user ids, no emails, no per-user rows.
/// Everything is read in ONE transaction (one snapshot), so a claim or settlement running at the same time can
/// never make the numbers contradict each other. Money and per-model figures come from the ledger and the batch, so
/// they survive the deletion of accounts and request rows; request outcome counts and tokens are those of the
/// requests still stored.
pub async fn issuer_report(
    State(s): State<AppState>,
    h: HeaderMap,
    Path((app, batch)): Path<(String, String)>,
) -> R<Json<Value>> {
    let u = auth_level(&s, &h, READ).await?;
    let email = email_enabled(&s, Some(&u)).await;
    let mut tx = s.db.begin().await?;
    let owned: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM apps WHERE id=? AND owner_user_id=? AND deleted_at IS NULL",
    )
    .bind(&app)
    .bind(&u)
    .fetch_optional(&mut *tx)
    .await?;
    if owned.is_none() {
        return Err(not_found("app"));
    }
    let b: Batch = sqlx::query_as(&format!("{BATCH_SELECT} WHERE b.id=? AND b.app_id=?"))
        .bind(&batch)
        .bind(&app)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| not_found("coupon batch"))?;
    let t = now();
    let (active, expired, revoked, exhausted, remaining, held): (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(b.revoked_at IS NULL AND c.expires_at>?1 AND c.remaining_micro>0 AND b.stopped_at IS NULL),0), \
         COALESCE(SUM(b.revoked_at IS NULL AND c.expires_at<=?1),0), \
         COALESCE(SUM(b.revoked_at IS NOT NULL),0), \
         COALESCE(SUM(b.revoked_at IS NULL AND c.expires_at>?1 AND (c.remaining_micro<=0 OR b.stopped_at IS NOT NULL)),0), \
         COALESCE(SUM(c.remaining_micro),0), COALESCE(SUM(c.held_micro),0) \
         FROM coupons c JOIN coupon_batches b ON b.id=c.batch_id WHERE c.batch_id=?2",
    )
    .bind(&t).bind(&batch).fetch_one(&mut *tx).await?;
    // ledger: counts by kind, money by kind
    let kinds: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT kind, COUNT(*), COALESCE(SUM(micro),0) FROM coupon_ledger WHERE batch_id=? GROUP BY kind",
    )
    .bind(&batch).fetch_all(&mut *tx).await?;
    let kind = |k: &str| {
        kinds
            .iter()
            .find(|x| x.0 == k)
            .map_or((0, 0), |x| (x.1, x.2))
    };
    // held money whose call is not on its way: delivery and cost are unknown (uncertain, or the account is gone)
    let uncertain_held: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(h.micro),0) FROM coupon_ledger h WHERE h.batch_id=?1 AND h.kind='hold' \
         AND NOT EXISTS (SELECT 1 FROM coupon_ledger c WHERE c.ukey='close:'||h.request_id) \
         AND NOT EXISTS (SELECT 1 FROM requests r WHERE r.id=h.request_id AND r.status IN ('approved','sending'))",
    )
    .bind(&batch).fetch_one(&mut *tx).await?;
    let models: Vec<(String, i64, i64, i64)> = sqlx::query_as(
        "SELECT COALESCE(model,'?'), COUNT(*), COALESCE(SUM(CASE WHEN kind='settle' THEN micro END),0), COALESCE(SUM(CASE WHEN kind='estimate' THEN micro END),0) \
         FROM coupon_ledger WHERE batch_id=? AND kind IN ('settle','estimate') GROUP BY model ORDER BY SUM(micro) DESC, 1",
    )
    .bind(&batch).fetch_all(&mut *tx).await?;
    let st: Vec<(String, i64, i64, i64)> = sqlx::query_as(
        "SELECT r.status, COUNT(*), COALESCE(SUM(r.input_tokens),0), COALESCE(SUM(r.output_tokens),0) FROM requests r JOIN coupons c ON c.id=r.coupon_id WHERE c.batch_id=? GROUP BY r.status",
    )
    .bind(&batch).fetch_all(&mut *tx).await?;
    let accounted: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM requests r JOIN coupons c ON c.id=r.coupon_id WHERE c.batch_id=? AND r.status='uncertain' AND EXISTS (SELECT 1 FROM coupon_ledger l WHERE l.ukey='close:'||r.id)",
    )
    .bind(&batch).fetch_one(&mut *tx).await?;
    tx.rollback().await?; // read-only: nothing to commit
    let mut requests = serde_json::Map::new();
    for k in [
        "pending",
        "approved",
        "sending",
        "completed",
        "failed",
        "denied",
        "expired",
        "uncertain",
    ] {
        requests.insert(k.into(), 0.into());
    }
    let (mut tin, mut tout) = (0, 0);
    for (status, n, i, o) in st {
        requests.insert(status, n.into());
        tin += i;
        tout += o;
    }
    let (_, actual) = kind("settle");
    let (_, estimated) = kind("estimate");
    Ok(Json(json!({
        "batch": b.json(&s, email),
        "coupons": {"claimed": kind("claim").0, "active": active, "expired": expired, "revoked": revoked, "exhausted": exhausted},
        "first_successful_users": kind("first_success").0,
        "money": {"total_usd": usd(b.total_micro), "available_usd": usd(b.available_micro), "unspent_in_coupons_usd": usd(remaining),
                  "held_usd": usd(held), "uncertain_held_usd": usd(uncertain_held), "in_flight_held_usd": usd(held - uncertain_held),
                  "spent_usd": usd(b.spent_micro), "actual_cost_usd": usd(actual), "completed_estimated_cost_usd": usd(estimated),
                  "returned_usd": usd(kind("return").1), "overshoot_usd": usd(b.overshoot_micro)},
        "requests": requests, "uncertain_requests_accounted": accounted, "input_tokens": tin, "output_tokens": tout,
        "models": models.into_iter().map(|(m, n, a, e)| json!({"model": m, "requests": n, "actual_cost_usd": usd(a), "completed_estimated_cost_usd": usd(e), "cost_usd": usd(a + e)})).collect::<Vec<_>>(),
    })))
}

// ---------- web landing ----------

/// What a phone without the app (or a computer) sees for a coupon link. Shows nothing about the batch.
pub async fn page(Path(code): Path<String>) -> R<Html<String>> {
    if !code_ok(&code) {
        return Err(e_not_found());
    }
    Ok(Html(format!(
        "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'><title>Keyala coupon</title>\
         <body style='font-family:system-ui;text-align:center;margin:3rem 1rem;line-height:1.5'><h1>You have a Keyala coupon</h1>\
         <p><a href='keyala://coupon/{code}' style='font-size:1.3rem'>Open in the Keyala app</a></p>\
         <p>No app yet? <a href='{}'>Get Keyala</a>, sign in, then come back and tap the link above again.</p>\
         <p style='color:#666'>Or enter <b>{code}</b> in the app.</p>\
         <p style='color:#666;margin-top:2rem'>On a computer? Scan with your iPhone camera:</p><img src='/coupon/{code}/qr.svg' width=200 height=200 alt='QR code for {code}'>{}",
        crate::routes::GET_APP_URL,
        crate::enroll::TRUST_LINK
    )))
}

pub async fn qr(State(s): State<AppState>, Path(code): Path<String>) -> axum::response::Response {
    use axum::response::IntoResponse;
    if !code_ok(&code) {
        return e_not_found().into_response();
    }
    crate::enroll::svg(&format!("{}/coupon/{code}", s.cfg.public_url))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    /// real schema (all migrations) in memory, foreign keys on
    async fn pool() -> SqlitePool {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::from_str("sqlite::memory:")
                    .unwrap()
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        db
    }
    async fn exec(db: &SqlitePool, sql: &str) {
        sqlx::query(sql).execute(db).await.unwrap();
    }
    fn row<'a>(id: &'a str, app: &'a str, owner: &'a str) -> NewRow<'a> {
        NewRow {
            id,
            app,
            owner,
            vendor: "openai",
            models: "[\"gpt-4o-mini\"]".into(),
            key_ct: vec![1],
            key_nonce: vec![2],
            key_last4: "9999".into(),
            per: 1,
            total: 2,
            days: 7,
            deadline: "2099-01-01T00:00:00.000Z",
        }
    }

    #[test]
    fn code_grammar_is_kc_plus_22_base64url_and_case_sensitive() {
        let c = new_code();
        assert!(code_ok(&c), "{c}");
        assert_eq!(c.len(), 25);
        assert!(!code_ok(&c[..24]) && !code_ok(&format!("{c}A")));
        assert!(!code_ok(&c.replacen("KC-", "kc-", 1)));
        assert!(!code_ok("KC-AAAAAAAAAAAAAAAAAAAAA+"));
        assert!(!code_ok("KC-AAAAAAAA"), "the old 8-character form is gone");
        assert!(code_ok("KC-abcDEF012_-abcDEF012_-"));
    }

    /// The final INSERT is the guard: after the awaited provider check the app or the issuer may be gone.
    #[tokio::test]
    async fn batch_insert_refuses_a_deleted_app_or_owner() {
        let db = pool().await;
        exec(
            &db,
            "INSERT INTO users (id,email,created_at) VALUES ('usr_o','o@x.co','t')",
        )
        .await;
        exec(&db, "INSERT INTO apps (id,name,secret_hash,created_at,owner_user_id) VALUES ('app_1','A','h','t','usr_o')").await;
        assert_eq!(
            insert_batch(
                &db,
                &row("cpb_1", "app_1", "usr_o"),
                "KC-AAAAAAAAAAAAAAAAAAAAAA"
            )
            .await
            .unwrap(),
            Some(true)
        );
        // a taken code is a retry, not a refusal
        assert_eq!(
            insert_batch(
                &db,
                &row("cpb_2", "app_1", "usr_o"),
                "KC-AAAAAAAAAAAAAAAAAAAAAA"
            )
            .await
            .unwrap(),
            Some(false)
        );
        // someone else's app is not this issuer's
        exec(
            &db,
            "INSERT INTO users (id,email,created_at) VALUES ('usr_x','x@x.co','t')",
        )
        .await;
        assert_eq!(
            insert_batch(
                &db,
                &row("cpb_3", "app_1", "usr_x"),
                "KC-BBBBBBBBBBBBBBBBBBBBBB"
            )
            .await
            .unwrap(),
            None
        );
        // the app is deleted between validation and insert
        exec(&db, "UPDATE apps SET deleted_at='t' WHERE id='app_1'").await;
        assert_eq!(
            insert_batch(
                &db,
                &row("cpb_4", "app_1", "usr_o"),
                "KC-CCCCCCCCCCCCCCCCCCCCCC"
            )
            .await
            .unwrap(),
            None
        );
        // the owner's account is deleted (apps outlive their owner with no owner)
        exec(&db, "UPDATE apps SET deleted_at=NULL WHERE id='app_1'").await;
        exec(&db, "UPDATE apps SET owner_user_id=NULL WHERE id='app_1'").await;
        exec(&db, "DELETE FROM users WHERE id='usr_o'").await;
        assert_eq!(
            insert_batch(
                &db,
                &row("cpb_5", "app_1", "usr_o"),
                "KC-DDDDDDDDDDDDDDDDDDDDDD"
            )
            .await
            .unwrap(),
            None
        );
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM coupon_batches")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(n, 1, "only the first insert happened");
    }

    async fn outbox_row(
        db: &SqlitePool,
        id: &str,
        status: &str,
        attempts: i64,
        first: Option<String>,
        next: String,
    ) {
        sqlx::query("INSERT INTO coupon_outbox (id,batch_id,kind,dedup_key,status,attempts,first_attempt_at,next_attempt_at,created_at,recipient,from_addr,subject,body) VALUES (?1,'b','capacity',?1,?2,?3,?4,?5,'t','to@x.co','from@x.co','s','text')")
            .bind(id).bind(status).bind(attempts).bind(first).bind(next).execute(db).await.unwrap();
    }
    async fn state(db: &SqlitePool, id: &str) -> (String, i64, Option<String>) {
        sqlx::query_as("SELECT status, attempts, recipient FROM coupon_outbox WHERE id=?")
            .bind(id)
            .fetch_one(db)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn outbox_claim_is_due_bounded_windowed_and_exclusive() {
        let db = pool().await;
        outbox_row(&db, "due", "pending", 0, None, after(-5)).await;
        outbox_row(&db, "later", "pending", 1, Some(now()), after(600)).await;
        outbox_row(
            &db,
            "spent",
            "pending",
            MAX_ATTEMPTS,
            Some(now()),
            after(-5),
        )
        .await;
        outbox_row(&db, "old", "pending", 1, Some(after(-25 * 3600)), after(-5)).await;
        assert_eq!(
            claim_outbox(&db, "later").await.unwrap(),
            None,
            "backoff not elapsed"
        );
        assert_eq!(
            claim_outbox(&db, "spent").await.unwrap(),
            None,
            "attempt budget used up"
        );
        assert_eq!(
            claim_outbox(&db, "old").await.unwrap(),
            None,
            "past the provider's 24h de-duplication window"
        );
        assert_eq!(claim_outbox(&db, "due").await.unwrap(), Some(1));
        assert_eq!(
            claim_outbox(&db, "due").await.unwrap(),
            None,
            "a second worker gets nothing"
        );
        let first: Option<String> =
            sqlx::query_scalar("SELECT first_attempt_at FROM coupon_outbox WHERE id='due'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert!(
            first.is_some(),
            "the 24h window starts at the first attempt"
        );
        assert_eq!(state(&db, "later").await.0, "pending");
    }

    #[tokio::test]
    async fn a_stale_result_cannot_resurrect_skip_or_overwrite_a_newer_attempt() {
        let db = pool().await;
        outbox_row(&db, "m", "pending", 0, None, after(-5)).await;
        // worker A claims attempt 1; the issuer is deleted (row skipped) while A is sending
        assert_eq!(claim_outbox(&db, "m").await.unwrap(), Some(1));
        exec(&db, "UPDATE coupon_outbox SET status='skipped', recipient=NULL, last_error='issuer_deleted' WHERE id='m'").await;
        finish_outbox(&db, "m", 1, Sent::Retry("late failure".into()))
            .await
            .unwrap();
        assert_eq!(
            state(&db, "m").await.0,
            "skipped",
            "a retry result must not put a skipped row back to pending"
        );
        finish_outbox(&db, "m", 1, Sent::Ok("mock")).await.unwrap();
        assert_eq!(state(&db, "m").await.0, "skipped");
        // attempt guard: attempt 1's late result after attempt 2 started changes nothing
        outbox_row(&db, "n", "pending", 0, None, after(-5)).await;
        assert_eq!(claim_outbox(&db, "n").await.unwrap(), Some(1));
        finish_outbox(&db, "n", 1, Sent::Retry("x".into()))
            .await
            .unwrap();
        exec(
            &db,
            "UPDATE coupon_outbox SET next_attempt_at='2000-01-01T00:00:00.000Z' WHERE id='n'",
        )
        .await;
        assert_eq!(claim_outbox(&db, "n").await.unwrap(), Some(2));
        finish_outbox(&db, "n", 1, Sent::Ok("mock")).await.unwrap();
        assert_eq!(
            state(&db, "n").await.0,
            "sending",
            "attempt 1 no longer owns the row"
        );
        // the owner of attempt 2 finishes: sent, and the address and text are erased
        finish_outbox(&db, "n", 2, Sent::Ok("mock")).await.unwrap();
        assert_eq!(state(&db, "n").await, ("sent".into(), 2, None));
        // a failure on the last attempt is final and permanent errors never retry
        outbox_row(
            &db,
            "p",
            "pending",
            MAX_ATTEMPTS - 1,
            Some(now()),
            after(-5),
        )
        .await;
        assert_eq!(claim_outbox(&db, "p").await.unwrap(), Some(MAX_ATTEMPTS));
        finish_outbox(&db, "p", MAX_ATTEMPTS, Sent::Retry("x".into()))
            .await
            .unwrap();
        assert_eq!(state(&db, "p").await.0, "failed");
        outbox_row(&db, "q", "pending", 0, None, after(-5)).await;
        assert_eq!(claim_outbox(&db, "q").await.unwrap(), Some(1));
        finish_outbox(&db, "q", 1, Sent::Permanent("422".into()))
            .await
            .unwrap();
        assert_eq!(state(&db, "q").await.0, "failed");
    }

    #[tokio::test]
    async fn outbox_payload_is_immutable_once_bound_but_can_be_erased() {
        let db = pool().await;
        outbox_row(&db, "i", "pending", 0, None, after(-5)).await;
        for col in ["recipient", "from_addr", "subject", "body"] {
            let r = sqlx::query(&format!(
                "UPDATE coupon_outbox SET {col}='changed' WHERE id='i'"
            ))
            .execute(&db)
            .await;
            assert!(r.is_err(), "{col} must be immutable once bound");
        }
        exec(&db, "UPDATE coupon_outbox SET recipient=NULL, from_addr=NULL, subject=NULL, body=NULL WHERE id='i'").await;
        assert_eq!(state(&db, "i").await.2, None);
    }
}
