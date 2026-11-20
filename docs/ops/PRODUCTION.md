# Durable production

## Status: LIVE and persistent (verified 2026-11-19)
- **Owner decision:** Joseph approved upgrading prod and resetting its old free-plan data. That data was NOT migrated and is gone by his explicit choice.
- **Service:** `masterkey-prod` (srv-dbsm5m0ae00c738euic0) is Starter with disk `dsk-dbvggvl9el5s73bqe9k0` (name `disk`, 1 GB, mounted at `/var/data`). Existing secret binding `MK_MASTER_KEY` unchanged (never regenerated). It remains the ACTUAL secret name on the live service as of 2026-11-20; the Keyala code reads `KEYALA_MASTER_KEY` first, else falls back to `MK_MASTER_KEY`.
- **Live env (2026-11-20):** the non-secret `KEYALA_*` env (`KEYALA_DATABASE_URL=sqlite:///var/data/masterkey.db?mode=rwc`, `KEYALA_BACKUP_DIR=/var/data/backups`, `KEYALA_BACKUP_KEEP=7`, `KEYALA_BACKUP_INTERVAL_SECS=86400`, dev login `0`) is applied on both live services. The old `MK_*` variables were deliberately retained for rollback; the new binary ignores them except the `MK_MASTER_KEY` fallback. A valid-shaped dev login returns 404.

### Keyala 0.2.0 release (2026-11-20, LIVE)
- **Prod:** main merge `5ad6fca0ef4e0f357e6824b54f547dca054e8545` (PR 16), deploy `dep-dbvvdcc9v7es73cc2sv0`, live 07:32:46Z. **Staging:** `36b5ef1` (PR 18), deploy `dep-dbvv9koae00c73ammatg`, live 07:24:11Z. Pages workflow 44694915696 and Backend CI 44694915708 succeeded.
- **Verified live on both:** `/v1/status` is 0.2.0, healthy, `db` true; dev login 404; `GET /v1/me/preferences` without auth 401; `/sdk/keyala.js` 200; `/about` and `/privacy` 200 and say Keyala; AASA 200 and still carries the old Apple bundle id `app.sorevan.masterkey`; public pairing returns a `KY-` code and a `keyala_cs_` secret, `seen` goes to `web`, the connect page leads with `keyala://`; Pages links under `/Keyala/` and all public pages 200.
- **Data check (prod):** before deploy a consistent backup `/var/data/backups/pre-keyala-20261120.db` was taken (`integrity_check` ok; the deletion log was copied if present). Counts users/keys/grants/requests were 1/2/0/0 before AND after the deploy, post-deploy `integrity_check` ok, and `SELECT count(*) FROM users WHERE openrouter_for_direct_apis != 0` returned 0. Only counts and integrity were checked: no private contents were read, no live model calls were made, so a real decrypt of a stored key and a paid response are NOT proven, only that the invariants held.
- **Unchanged on purpose:** Apple bundle id and APNs topic, Render service names, ids and hosts, the prod disk, `masterkey.db` and the `masterkey-` backup prefix, the `MK_MASTER_KEY` secret binding (fallback). See `KEYALA_CUTOVER.md`.
- **Deploy:** PR 12 (dev -> staging, merge 15c5da) and PR 13 (staging -> main, merge a60c9b); live deploy `dep-dbvgnanf3r2c73fv463g` at 14:49:38Z. `/v1/status` reports `ok` and `db` true.
- **Persistence check:** a pending connect row created BEFORE the deploy survived onto the new instance. This is real database persistence, not an app token being re-created from its sealed secret.
- **Backup check:** the automatic backup exists (mode 600, 135168 bytes). It was restored into `/tmp/prod-restore-check.db`: `PRAGMA integrity_check` ok and users/requests/api_keys counts match the backup. No sensitive content was read.
- **What the check proves:** a sidecar, non-destructive rehearsal. It is not a real rollback, not a deletion-log replay (`replay-deletions.sh`), and not an offsite disaster-recovery test.

### Staging (CURRENT as of 2026-11-20)
- **Disk permanently deleted.** Joseph explicitly confirmed irreversible deletion, including the same-disk backups and snapshots. Disk `dsk-dbuielgm7kps73cgk420` was deleted in the Render Dashboard. Nothing from it can be restored.
- **Live service** `srv-dbsm5m0ae00c738euib0`: plan `free`, no disk, `not_suspended`, live deploy `dep-dbvuikn40ujc73eu37ng`. Staging data resets on deploy/sleep again. `render.yaml` (free, no disk) matches; it must be promoted to `main` so the Blueprint does not re-create a paid staging.
- **Cost baseline:** prod about $7.25/month, staging $0, excluding extra usage and tax (estimate, not an invoice).

### Staging history (2026-11-19, superseded, kept as dated evidence only)
- On 2026-11-19 staging disk `dsk-dbuielgm7kps73cgk420` was preserved and a consistent `VACUUM INTO` copy `/var/data/backups/pre-prod-20261119T1443.db` survived a restart and restored sidecar (integrity ok, counts match). Staging compute was then SUSPENDED (Render API `updatedAt` 2026-11-19T14:51:52Z). All of this is no longer current: the disk and that backup are gone (see above) and staging is running on Free.

## Remaining risks (unresolved)
- Backups live on the SAME disk as the database: they protect against bad deploys and mistakes, not disk loss. No offsite copy or independently held master-key escrow was verified in this session, because Joseph's password manager was not inspected.
- `KEYALA_MASTER_KEY` escrow is unresolved (unverified, see above). Lose the key and every stored provider key is undecryptable (users re-add keys). Store a copy in a password manager.
- A restore with deletion-log replay has not been rehearsed on prod.
- Apple deletion revocation key (`KEYALA_APPLE_SIGNIN_KEY_ID` / `_P8`) is still not set: deletion works and reports `apple_revoked:false`.
- Real Apple deletion, APNs delivery and OpenRouter OAuth/billing on the live services are unverified.
- The manual `pre-keyala-20261120.db` backup is NOT part of the seven-copy automatic retention, so it never rolls off by itself. It holds user data: the operator must retire it (delete it) after the rollback window closes, and must re-apply deletions (`replay-deletions.sh`) to any restore made from it. No retention deadline has been set.

## Cost (estimate, not an invoice)
About $7.25/month for prod (Starter plus the 1 GB disk); staging is Free with no disk ($0) since 2026-11-20 (the earlier ~$0.25/month retained-disk line is obsolete). Baseline roughly $7.25/month. Excludes extra usage, overlap, bandwidth, build minutes and tax. The actual invoice has not been checked, so do not claim it as proven. Avoid duplicate paid resources. Verified from Render's docs: a disk attaches to exactly one service instance (prod and staging cannot share one) and cannot be added to a cron job.

## render.yaml diff for `masterkey-prod`
```diff
   - type: web
     name: masterkey-prod
     runtime: docker
-    plan: free
+    plan: starter
+    disk:
+      name: disk
+      mountPath: /var/data
+      sizeGB: 1
 ...
       - key: KEYALA_DATABASE_URL
-        value: sqlite:///data/masterkey.db?mode=rwc
+        value: sqlite:///var/data/masterkey.db?mode=rwc
+      - key: KEYALA_BACKUP_DIR
+        value: /var/data/backups
+      - key: KEYALA_BACKUP_KEEP
+        value: "7"
+      - key: KEYALA_BACKUP_INTERVAL_SECS
+        value: "86400"
```
This is in `render.yaml` for prod and now matches the live service; staging has no backup env. `KEYALA_MASTER_KEY` stays `generateValue: true` (Render keeps the existing value). `KEYALA_DEV_LOGIN` stays `0`.

## Backups
The disk belongs to one service, so no cron job or GitHub Action can read it. The server does it itself: `KEYALA_BACKUP_DIR` writes `masterkey-<utc>.db` (`VACUUM INTO`, consistent, mode 600) once per 24 h, counted from the newest file so restarts neither duplicate nor evict backups, keeps the newest `KEYALA_BACKUP_KEEP` (7). Logged: path only. These live on the same disk (protects against bad deploys and mistakes, not disk loss); for off-disk copies run `backup.sh` plus `scp`/`render ssh` from an operator machine now and then. Test: `in_process_backup_runs_prunes_and_is_a_database`.

## Rollout (done 2026-11-19; kept as the procedure for future changes)
1. Disk and env vars confirmed on the live service before deploying.
2. dev -> staging -> main by PR; wait for `/v1/status` ok; create a throwaway record, redeploy, confirm it survives; confirm `/var/data/backups` has a file.
3. Keep `KEYALA_DEV_LOGIN=0`. For deletion revocation set `KEYALA_APPLE_SIGNIN_KEY_ID` + `KEYALA_APPLE_SIGNIN_KEY_P8` in the dashboard (sync: false). Still open.
4. Keep a copy of `KEYALA_MASTER_KEY` in a password manager; it cannot be recovered from the DB. Still open.

## Rollback
Do NOT assume that redeploying the previous image alone works. Migration `0012` has been applied to the prod database, and an older SQLx binary can refuse a database that has migrations it does not know. A real rollback needs: (1) the pause switch on, (2) a verified pre-migration database recovery (restore a copy of `pre-keyala-20261120.db` as a sidecar first, check `integrity_check` and counts), (3) deletion-log replay (`replay-deletions.sh`) on the restored copy, and (4) preservation of anything written after cutover (new users, keys, requests) before it is overwritten. The old `MK_*` env and `MK_MASTER_KEY` were kept so an old build can still boot. Never run a restore or overwrite the live DB without that sequence. Going back to the free plan discards the disk data: take a backup first (`backup.sh`). This rollback path has not been exercised on prod.
