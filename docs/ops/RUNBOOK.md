# MasterKey runbook

Services: `masterkey-prod` (branch `main`), `masterkey-staging` (branch `staging`) on Render. Data is one SQLite
file (`MK_DATABASE_URL`); provider keys in it are AES-GCM encrypted with `MK_MASTER_KEY` (env var, never in the DB).

## Checks
- `curl https://HOST/healthz` -> `ok` (process is up).
- `curl https://HOST/v1/status` -> `{"ok":true,"paused":false,"db":true,"version":"..."}`. HTTP 503 / `db:false` = the database does not answer.
- Logs: sign-in outcomes, deletions ("account deleted"), backups ("backup written: path"). No tokens, keys, prompts or IPs are logged.

## Pause provider calls (kill switch)
New proxy calls answer `503 upstream_paused` before anything is stored; requests already `approved` stay approved and are
sent after unpausing (the sweeper retries every 30 s); `sending` ones finish. Users can still sign in, view and stop everything.
- Env: set `MK_PAUSE_UPSTREAM=1` in the Render dashboard (restarts the service), remove to resume.
- No restart: in the Render shell run `touch /var/data/PAUSE_UPSTREAM` (the file sits beside the database); `rm` it to resume. `/v1/status` shows `paused`.

## Incidents
- **Provider outage.** Calls fail with 502/5xx or `uncertain` (never retried by design). Optionally pause; tell users nothing is charged for `failed` calls, and `uncertain` ones keep their reservation.
- **Suspected key leak / abuse.** Per user: they use Stop everything (`POST /v1/me/stop-all`) and rotate the key at the provider. Server side: pause upstream first. **Never rotate `MK_MASTER_KEY` casually:** every stored provider key becomes undecryptable (requests then fail `no_key`) and users must re-add all keys. Rotating needs a re-encrypt migration that does not exist yet; if the master key itself leaked, plan: pause, tell users, delete stored keys (`DELETE FROM api_keys`), set a new master key, users re-add keys.
- **Disk full.** SQLite writes fail (500s, `db:false`). Delete old files in `/var/data/backups`, then grow the disk in the dashboard (disks can grow, not shrink). Restart.
- **Bad deploy.** Render dashboard -> service -> Events -> redeploy the last good deploy (or revert the commit on `main`). Migrations only add things, so an older binary still opens a newer database; if a migration itself is the problem, restore the pre-deploy backup.
- **Restore from backup.** (1) pause upstream; (2) stop the service or work on a copy; (3) keep the current `deleted-accounts.log`; (4) put the chosen `masterkey-*.db` in place of `masterkey.db` (remove stale `-wal`/`-shm` files); (5) `docs/ops/replay-deletions.sh /var/data/masterkey.db /path/to/deleted-accounts.log`; (6) start with the SAME `MK_MASTER_KEY`; (7) check `/v1/status`, unpause. Drill: `docs/ops/RESTORE_DRILL.md`.

## What is and is not recoverable
- Recoverable from a backup: users, grants, key ciphertexts, request history up to the backup time. Lost: everything after it (including approvals and spend counters since).
- Not recoverable: stored keys if `MK_MASTER_KEY` is lost (keep a copy in a password manager, separate from backups). Sessions and developer secrets: sessions are in the DB; app secrets carry their own sealed record and survive.
- Free-plan services (prod today) have no disk: everything resets on deploy/restart. Backups only make sense on a service with a disk.

## Deletion fails closed (`503 deletion_unavailable`)
The `deleted <id>` line is written and fsynced BEFORE the database commit. If the log cannot be written (disk full, permissions, path is a directory) the user gets `503 deletion_unavailable`, nothing is deleted and their session stays valid; fix the disk/permissions and ask them to retry. If the commit fails after the line was written, the server appends `undeleted <id> <time>`; replay uses the last line per id, so such an id is NOT re-deleted. Never hand-edit the log except to append lines.

## Backups and account deletion
Backups come from `MK_BACKUP_DIR` (in-process, daily, newest `MK_BACKUP_KEEP`=7 kept, mode 600) or `docs/ops/backup.sh`. A deleted account can
reappear from an older backup, so each deletion appends `deleted <user_id> <rfc3339>` (opaque id, no email) to `deleted-accounts.log` beside the
database and the restore step above replays it. Keep that log across restores (it is not inside the backup). Deleted data therefore lives in backups
for at most `MK_BACKUP_KEEP` days; state that in the privacy policy. Copy backups off the disk occasionally (`backup.sh` on a machine that can reach the file): a disk loss takes in-disk backups with it.
