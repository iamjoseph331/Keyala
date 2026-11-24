//! Account controls and operations: sign out everywhere, stop-all, account deletion, pause switch,
//! status, in-process backups.
use crate::{proxy::refund, routes::rate_limit, *};
use axum::extract::{ConnectInfo, State};
use std::{net::SocketAddr, path::PathBuf};

// ---------- pause switch & status ----------

/// Directory of the SQLite file (where PAUSE_UPSTREAM and deleted-accounts.log live); None for in-memory.
pub fn db_dir(cfg: &Config) -> Option<PathBuf> {
    let rest = cfg
        .database_url
        .strip_prefix("sqlite://")
        .or_else(|| cfg.database_url.strip_prefix("sqlite:"))?;
    let path = rest
        .split('?')
        .next()
        .filter(|p| !p.is_empty() && !p.starts_with(":memory:"))?;
    Some(match std::path::Path::new(path).parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    })
}

/// True while the operator has paused new provider calls (env flag or a PAUSE_UPSTREAM file). Cheap: one stat.
pub fn paused(s: &AppState) -> bool {
    s.cfg.pause_upstream || db_dir(&s.cfg).is_some_and(|d| d.join("PAUSE_UPSTREAM").exists())
}

pub async fn status(State(s): State<AppState>) -> (StatusCode, Json<Value>) {
    let db = sqlx::query("SELECT 1").execute(&s.db).await.is_ok();
    let code = if db {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        code,
        Json(
            json!({"ok": db, "paused": paused(&s), "db": db, "version": env!("CARGO_PKG_VERSION")}),
        ),
    )
}

// ---------- sessions & stop-all ----------

#[derive(Deserialize)]
pub struct RevokeReq {
    keep_current: Option<bool>,
}

pub async fn revoke_sessions(
    State(s): State<AppState>,
    h: HeaderMap,
    J(b): J<RevokeReq>,
) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    let me = hash_token(bearer(&h, "keyala_sess_").unwrap_or_default());
    let keep = b.keep_current.unwrap_or(true);
    let r = sqlx::query("DELETE FROM sessions WHERE user_id=? AND (?=0 OR token_hash<>?)")
        .bind(&u)
        .bind(keep as i64)
        .bind(&me)
        .execute(&s.db)
        .await?;
    Ok(Json(json!({"revoked": r.rows_affected()})))
}

/// Ends only the session whose token is sent. A token that is already gone answers 401 like any
/// dead session, so clients treat 401 on logout as "already signed out" and clear local state either way.
pub async fn logout(State(s): State<AppState>, h: HeaderMap) -> R<Json<Value>> {
    auth_level(&s, &h, READ).await?; // a web session may sign itself out
    let me = hash_token(bearer(&h, "keyala_sess_").unwrap_or_default());
    sqlx::query("DELETE FROM sessions WHERE token_hash=?")
        .bind(&me)
        .execute(&s.db)
        .await?;
    Ok(Json(json!({"signed_out": true})))
}

/// Denies pending or approved-but-unsent requests and refunds their reservations. Run inside a
/// transaction that already holds the write lock (so nothing is claimed for sending meanwhile).
pub(crate) async fn deny_unsent(
    tx: &mut sqlx::SqliteConnection,
    ids: Vec<String>,
) -> Result<Vec<String>, sqlx::Error> {
    let mut denied = vec![];
    for id in ids {
        let won = sqlx::query("UPDATE requests SET status='denied', decided_at=? WHERE id=? AND status IN ('pending','approved')").bind(now()).bind(&id).execute(&mut *tx).await?.rows_affected() == 1;
        if won {
            let r: RequestRow = sqlx::query_as(&format!("{REQUEST_SELECT} WHERE r.id=?"))
                .bind(&id)
                .fetch_one(&mut *tx)
                .await?;
            refund(tx, &r).await?; // only requests that were never sent get here: no double refund
            denied.push(id);
        }
    }
    Ok(denied)
}

pub async fn stop_all(State(s): State<AppState>, h: HeaderMap) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    let mut tx = s.db.begin().await?;
    // write first: takes the write lock, so nothing is claimed for sending while we deny
    let gids: Vec<String> = sqlx::query_scalar(
        "UPDATE grants SET revoked_at=? WHERE user_id=? AND revoked_at IS NULL RETURNING id",
    )
    .bind(now())
    .bind(&u)
    .fetch_all(&mut *tx)
    .await?;
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM requests WHERE user_id=? AND status IN ('pending','approved')",
    )
    .bind(&u)
    .fetch_all(&mut *tx)
    .await?;
    let denied = deny_unsent(&mut tx, ids).await?;
    let in_flight: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM requests WHERE user_id=? AND status='sending'")
            .bind(&u)
            .fetch_one(&mut *tx)
            .await?;
    let enr = sqlx::query("UPDATE enrollments SET status='cancelled', ciphertext=NULL, nonce=NULL WHERE user_id=? AND status IN ('claimed','submitted')").bind(&u).execute(&mut *tx).await?.rows_affected();
    let flows = sqlx::query("UPDATE oauth_flows SET status='cancelled', verifier=NULL WHERE user_id=? AND status='pending'").bind(&u).execute(&mut *tx).await?.rows_affected();
    tx.commit().await?;
    for id in &denied {
        emit_request(&s, "request.updated", id).await;
    }
    for g in &gids {
        emit_grant(&s, g).await;
    }
    Ok(Json(
        json!({"grants_revoked": gids.len(), "requests_denied": denied.len(), "requests_in_flight": in_flight,
                   "enrollments_cancelled": enr, "flows_cancelled": flows}),
    ))
}

// ---------- account deletion ----------

#[derive(Deserialize)]
pub struct DeleteReq {
    confirm: String,
    id_token: Option<String>,
    authorization_code: Option<String>,
    password: Option<String>,
    email: Option<String>,
    /// any account with a passkey: an assertion for a `/v1/auth/passkey/options` challenge
    passkey: Option<crate::passkey::Assertion>,
}

fn reauth_failed() -> ApiError {
    api(
        401,
        "reauth_failed",
        "could not confirm it is you; sign in again and retry",
    )
}

fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Apple: revoke the user's tokens (needed by App Store rules). Best effort; the code and every token stay out of logs.
async fn apple_revoke(s: &AppState, code: &str) -> bool {
    let Some((kid, pem)) = &s.cfg.apple_signin_key else {
        return false;
    };
    let c = &s.cfg;
    let Ok(key) = jsonwebtoken::EncodingKey::from_ec_pem(pem.as_bytes()) else {
        return false;
    };
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
    header.kid = Some(kid.clone());
    let iat = unix_now();
    let claims = json!({"iss": c.apple_team_id, "iat": iat, "exp": iat + 300, "aud": apple::ISSUER, "sub": c.apple_bundle_id});
    let Ok(secret) = jsonwebtoken::encode(&header, &claims, &key) else {
        return false;
    };
    let post = |path: &str, form: Vec<(&str, &str)>| {
        s.http
            .post(format!("{}{path}", c.apple_auth_url))
            .timeout(Duration::from_secs(10))
            .form(&form)
            .send()
    };
    let Ok(r) = post(
        "/auth/token",
        vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", &c.apple_bundle_id),
            ("client_secret", &secret),
        ],
    )
    .await
    else {
        return false;
    };
    if !r.status().is_success() {
        return false;
    }
    let Some(refresh) = r
        .text()
        .await
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v["refresh_token"].as_str().map(String::from))
    else {
        return false;
    };
    let form = vec![
        ("token", refresh.as_str()),
        ("token_type_hint", "refresh_token"),
        ("client_id", &c.apple_bundle_id),
        ("client_secret", &secret),
    ];
    post("/auth/revoke", form)
        .await
        .is_ok_and(|r| r.status().is_success())
}

/// Appends `{word} {user_id} {time}` (opaque id, no email) to deleted-accounts.log and fsyncs it.
/// A restore replays it (docs/ops/replay-deletions.sh). No data directory (in-memory DB): nothing to restore, nothing to write.
fn tombstone(s: &AppState, word: &str, u: &str) -> std::io::Result<()> {
    use std::io::Write;
    let Some(d) = db_dir(&s.cfg) else {
        return Ok(());
    };
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(d.join("deleted-accounts.log"))?;
    f.write_all(format!("{word} {u} {}\n", now()).as_bytes())?;
    f.sync_all()
}

async fn sending_count(
    db: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    u: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM requests WHERE user_id=? AND status='sending'")
        .bind(u)
        .fetch_one(db)
        .await
}

pub async fn delete_account(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    h: HeaderMap,
    J(b): J<DeleteReq>,
) -> R<Json<Value>> {
    let u = auth_session(&s, &h).await?;
    if b.confirm != "DELETE" {
        return Err(api(
            400,
            "confirmation_required",
            "send {\"confirm\": \"DELETE\"} to delete the account",
        ));
    }
    rate_limit(&s, peer, &h, "delete", 5, 600, "account deletion attempts")?;
    let (email, apple_sub, google_sub): (String, Option<String>, Option<String>) =
        sqlx::query_as("SELECT email, apple_sub, google_sub FROM users WHERE id=?")
            .bind(&u)
            .fetch_one(&s.db)
            .await?;
    // fresh identity, same generic 401 for every failure
    let is_apple = apple_sub.is_some();
    if let Some(a) = &b.passkey {
        // the challenge is at most 5 minutes old and single use, so this is fresh too
        let who = crate::passkey::verify_assertion(&s, a)
            .await
            .map_err(|_| reauth_failed())?;
        if who != u {
            return Err(reauth_failed());
        }
    } else if let Some(sub) = apple_sub.as_ref().or(google_sub.as_ref()) {
        let tok = b.id_token.as_deref().ok_or_else(reauth_failed)?;
        let c = if is_apple {
            apple::verify(&s, tok).await
        } else {
            apple::verify_google(&s, tok).await
        }
        .map_err(|_| reauth_failed())?;
        let iat = c.iat.ok_or_else(reauth_failed)?;
        if !ct_eq(c.sub.as_bytes(), sub.as_bytes())
            || iat < unix_now() - 300
            || iat > unix_now() + 60
        {
            return Err(reauth_failed());
        }
    } else if let Some((_, hash)) = s
        .cfg
        .password_login
        .as_ref()
        .filter(|(n, _)| email == format!("{n}@password.invalid"))
    {
        let pw = b.password.as_deref().ok_or_else(reauth_failed)?;
        if !ct_eq(Sha256::digest(pw.as_bytes()).as_slice(), hash) {
            return Err(reauth_failed());
        }
    } else if !(s.cfg.dev_login
        && b.email
            .as_deref()
            .is_some_and(|e| e.trim().eq_ignore_ascii_case(&email)))
    {
        return Err(reauth_failed());
    }
    let in_flight = || {
        api(
            409,
            "request_in_flight",
            "a call is being sent to a provider; try again in about a minute",
        )
    };
    if sending_count(&s.db, &u).await? > 0 {
        return Err(in_flight());
    }
    let mut tx = s.db.begin().await?;
    sqlx::query("DELETE FROM sessions WHERE user_id=?")
        .bind(&u)
        .execute(&mut *tx)
        .await?; // write first (lock)
    if sending_count(&mut *tx, &u).await? > 0 {
        return Err(in_flight()); // tx drops: sessions stay
    }
    // FK-safe order. Every table with a user_id column is listed (a test fails if a new one appears).
    for q in [
        "DELETE FROM connect_codes WHERE grant_id IN (SELECT id FROM grants WHERE user_id=?1)",
        "DELETE FROM requests WHERE user_id=?1",
        "DELETE FROM grants WHERE user_id=?1",
        "DELETE FROM api_keys WHERE user_id=?1",
        "DELETE FROM devices WHERE user_id=?1",
        "DELETE FROM enrollments WHERE user_id=?1",
        "DELETE FROM oauth_flows WHERE user_id=?1",
        "DELETE FROM web_logins WHERE user_id=?1",
        "DELETE FROM passkeys WHERE user_id=?1",
        "DELETE FROM passkey_challenges WHERE user_id=?1",
        // apps outlive their owner: they keep working with their secret
        "UPDATE apps SET owner_user_id=NULL WHERE owner_user_id=?1",
        "DELETE FROM users WHERE id=?1",
    ] {
        sqlx::query(q).bind(&u).execute(&mut *tx).await?;
    }
    // Tombstone BEFORE commit, flushed to disk: if it cannot be written the transaction rolls back and
    // nothing is deleted (503), so a restore can never resurrect an account we told the user was gone.
    // If the commit itself then fails, an `undeleted` line cancels the tombstone.
    tombstone(&s, "deleted", &u).map_err(|e| {
        tracing::error!("deletion tombstone failed, account kept: {e}");
        api(
            503,
            "deletion_unavailable",
            "could not record the deletion safely; nothing was deleted, try again later",
        )
    })?;
    if let Err(e) = tx.commit().await {
        let _ = tombstone(&s, "undeleted", &u);
        return Err(e.into());
    }
    // after the commit, so a refused deletion (409, tombstone failure) never revokes Apple access
    let apple_revoked = match (is_apple, b.authorization_code.as_deref()) {
        (false, _) => Value::Null,
        (true, Some(code)) => apple_revoke(&s, code).await.into(),
        (true, None) => false.into(),
    };
    tracing::info!("account deleted");
    Ok(Json(
        json!({"deleted": true, "apple_revoked": apple_revoked}),
    ))
}

// ---------- in-process backups ----------

fn backups(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("masterkey-") && n.ends_with(".db"))
        })
        .collect();
    v.sort(); // names embed a UTC timestamp
    v
}

async fn backup_once(s: &AppState, dir: &std::path::Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!(
        "masterkey-{}.db",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ")
    ));
    sqlx::query("VACUUM INTO ?")
        .bind(path.to_string_lossy().to_string())
        .execute(&s.db)
        .await
        .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    let all = backups(dir);
    for old in all.iter().take(all.len().saturating_sub(s.cfg.backup_keep)) {
        let _ = std::fs::remove_file(old);
    }
    Ok(path)
}

/// Once per interval, counted from the newest existing backup so frequent restarts or deploys neither
/// pile up backups nor push out older ones. The file holds user data and encrypted keys, never the master key.
pub async fn backup_loop(s: AppState) {
    let Some(dir) = s.cfg.backup_dir.clone() else {
        return;
    };
    loop {
        let age = backups(&dir)
            .last()
            .and_then(|p| std::fs::metadata(p).ok()?.modified().ok()?.elapsed().ok());
        if let Some(wait) = age.and_then(|a| s.cfg.backup_interval.checked_sub(a)) {
            tokio::time::sleep(wait).await;
        }
        match backup_once(&s, &dir).await {
            Ok(p) => tracing::info!("backup written: {}", p.display()),
            Err(e) => tracing::error!("backup failed: {e}"),
        }
        tokio::time::sleep(s.cfg.backup_interval).await;
    }
}
