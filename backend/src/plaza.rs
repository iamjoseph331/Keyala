//! Plaza: an opt-in list of community apps on the examples page. An owner submits an app that has a
//! homepage, a reviewer (a user id in KEYALA_PLAZA_REVIEWERS) approves it by hand, and only then do
//! its name and homepage become public. Everything is 404 unless KEYALA_PLAZA_ENABLED=1.
//!
//! Status lives in `apps.plaza_status` (NULL, pending, approved, rejected). Every transition is one
//! conditional UPDATE, so an owner edit (console::apps_update clears the status in its own UPDATE)
//! and a reviewer decision can never both win: approval is bound to the exact name and homepage the
//! reviewer saw.
use crate::routes::{not_found, rate_limit};
use crate::*;
use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::header,
    response::Html,
};
use std::net::SocketAddr;

const MAX_PUBLIC: i64 = 200;

fn on(s: &AppState) -> R<()> {
    s.cfg
        .plaza_enabled
        .then_some(())
        .ok_or_else(|| not_found("page"))
}

pub fn is_reviewer(s: &AppState, uid: &str) -> bool {
    s.cfg.plaza_enabled && s.cfg.plaza_reviewers.iter().any(|r| r == uid)
}

/// An https URL naming a public DNS host: no credentials, no IP literals, no local names, only
/// plain ASCII (internationalised hosts go in as punycode).
pub fn homepage_ok(u: &str) -> bool {
    let Some(rest) = u.strip_prefix("https://") else {
        return false;
    };
    if u.len() > 300
        || !u.bytes().all(|b| b.is_ascii_graphic())
        || u.contains(['\\', '<', '>', '"', '\'', '`'])
    {
        return false;
    }
    let auth = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = match auth.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => h,
        Some(_) => return false,
        None => auth,
    };
    let labels: Vec<&str> = host.split('.').collect();
    let tld = labels.last().copied().unwrap_or("");
    labels.len() >= 2
        && labels.iter().all(|l| {
            (1..=63).contains(&l.len())
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        // a TLD starts with a letter: rules out 127.0.0.1, 127.1 and 0x7f.0x1 style IP spellings
        && tld.as_bytes()[0].is_ascii_alphabetic()
        && !matches!(
            tld.to_ascii_lowercase().as_str(),
            "local" | "localhost" | "internal" | "test" | "invalid" | "example" | "lan" | "home" | "corp"
        )
}

// ---------- public ----------

/// Approved apps, name and homepage only. Public and CORS-open (see the router).
pub async fn list(State(s): State<AppState>) -> R<axum::response::Response> {
    use axum::response::IntoResponse;
    on(&s)?;
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT name, homepage FROM apps WHERE plaza_status='approved' AND deleted_at IS NULL \
         AND owner_user_id IS NOT NULL AND homepage IS NOT NULL ORDER BY plaza_at DESC, id LIMIT ?",
    )
    .bind(MAX_PUBLIC)
    .fetch_all(&s.db)
    .await?;
    let v: Vec<Value> = rows
        .into_iter()
        .map(|(name, homepage)| json!({"name": name, "homepage": homepage}))
        .collect();
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(v)).into_response())
}

// ---------- the app owner ----------

pub async fn submit(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> R<Json<Value>> {
    on(&s)?;
    let u = auth_level(&s, &h, READ).await?;
    rate_limit(&s, peer, &h, "plaza", 20, 3600, "submissions")?;
    let hp: Option<Option<String>> = sqlx::query_scalar(
        "SELECT homepage FROM apps WHERE id=? AND owner_user_id=? AND deleted_at IS NULL",
    )
    .bind(&id)
    .bind(&u)
    .fetch_optional(&s.db)
    .await?;
    let hp = hp.ok_or_else(|| not_found("app"))?;
    let hp = hp
        .filter(|x| homepage_ok(x))
        .ok_or_else(|| bad("the app needs a valid https:// homepage before it can be listed"))?;
    // bound to the homepage we just checked, so an edit in between cannot slip an unchecked one in
    let n = sqlx::query(
        "UPDATE apps SET plaza_status='pending', plaza_at=? WHERE id=? AND owner_user_id=? \
         AND deleted_at IS NULL AND homepage=? AND (plaza_status IS NULL OR plaza_status='rejected')",
    )
    .bind(now())
    .bind(&id)
    .bind(&u)
    .bind(&hp)
    .execute(&s.db)
    .await?
    .rows_affected();
    if n != 1 {
        return Err(api(
            409,
            "already_listed",
            "this app is already submitted or listed, or it was just edited: reload and retry",
        ));
    }
    Ok(Json(json!({"status": "pending"})))
}

pub async fn withdraw(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> R<Json<Value>> {
    on(&s)?;
    let u = auth_level(&s, &h, READ).await?;
    let n = sqlx::query(
        "UPDATE apps SET plaza_status=NULL WHERE id=? AND owner_user_id=? AND deleted_at IS NULL",
    )
    .bind(&id)
    .bind(&u)
    .execute(&s.db)
    .await?
    .rows_affected();
    if n != 1 {
        return Err(not_found("app"));
    }
    Ok(Json(json!({"status": null})))
}

// ---------- the reviewer ----------

async fn reviewer(s: &AppState, h: &HeaderMap) -> R<()> {
    on(s)?;
    let u = auth_level(s, h, READ).await?;
    // everyone else learns nothing, not even that the route exists
    is_reviewer(s, &u)
        .then_some(())
        .ok_or_else(|| not_found("page"))
}

/// What a reviewer sees: public fields only. No owner, no secrets, no keys, no request content.
pub async fn review_list(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> R<Json<Value>> {
    reviewer(&s, &h).await?;
    let st = q.get("status").map_or("pending", |v| v.as_str());
    if !matches!(st, "pending" | "approved" | "rejected") {
        return Err(bad("status must be pending, approved or rejected"));
    }
    let rows: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT id, name, homepage, plaza_at FROM apps WHERE plaza_status=? AND deleted_at IS NULL \
         AND owner_user_id IS NOT NULL ORDER BY plaza_at DESC, id LIMIT ?",
    )
    .bind(st)
    .bind(MAX_PUBLIC)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows.into_iter().map(|(id, name, homepage, at)| {
        json!({"app_id": id, "name": name, "homepage": homepage, "status": st, "submitted_at": at})
    }).collect()))
}

#[derive(Deserialize)]
pub struct Decision {
    decision: String,
    /// what the reviewer saw; the decision only applies if the app still has exactly these
    name: String,
    homepage: String,
}

pub async fn review_decide(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    J(b): J<Decision>,
) -> R<Json<Value>> {
    reviewer(&s, &h).await?;
    let (from, to) = match b.decision.as_str() {
        "approve" => ("pending", "approved"),
        "reject" => ("pending", "rejected"),
        "withdraw" => ("approved", "rejected"),
        _ => return Err(bad("decision must be approve, reject or withdraw")),
    };
    if to == "approved" && !homepage_ok(&b.homepage) {
        return Err(bad("the homepage is not a valid https:// URL"));
    }
    let n = sqlx::query(
        "UPDATE apps SET plaza_status=?, plaza_at=? WHERE id=? AND plaza_status=? AND name=? \
         AND homepage=? AND deleted_at IS NULL AND owner_user_id IS NOT NULL",
    )
    .bind(to)
    .bind(now())
    .bind(&id)
    .bind(from)
    .bind(&b.name)
    .bind(&b.homepage)
    .execute(&s.db)
    .await?
    .rows_affected();
    if n == 1 {
        return Ok(Json(json!({"app_id": id, "status": to})));
    }
    let cur: Option<(Option<String>, String, Option<String>)> = sqlx::query_as(
        "SELECT plaza_status, name, homepage FROM apps WHERE id=? AND deleted_at IS NULL AND owner_user_id IS NOT NULL",
    )
    .bind(&id)
    .fetch_optional(&s.db)
    .await?;
    match cur {
        None => Err(not_found("app")),
        Some((_, name, hp)) if name != b.name || hp.as_deref() != Some(b.homepage.as_str()) => {
            Err(api(
                409,
                "stale_submission",
                "the app changed since you loaded it: reload and review again",
            ))
        }
        Some(_) => Err(api(409, "not_pending", format!("the app is not {from}"))),
    }
}

// ---------- the page ----------

const PAGE: &str = include_str!("plaza.html");
const SCRIPT: &str = include_str!("plaza.js");

pub async fn page(State(s): State<AppState>) -> R<axum::response::Response> {
    use axum::response::IntoResponse;
    on(&s)?;
    let csp = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; font-src 'self' data:; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";
    Ok((
        [
            (header::CONTENT_SECURITY_POLICY, csp),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Html(PAGE),
    )
        .into_response())
}

pub async fn script(State(s): State<AppState>) -> R<axum::response::Response> {
    use axum::response::IntoResponse;
    on(&s)?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        SCRIPT,
    )
        .into_response())
}
