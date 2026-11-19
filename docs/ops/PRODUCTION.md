# Durable production

## Status: LIVE and persistent (verified 2026-11-19)
- **Owner decision:** Joseph approved upgrading prod and resetting its old free-plan data. That data was NOT migrated and is gone by his explicit choice.
- **Service:** `masterkey-prod` (srv-dbsm5m0ae00c738euic0) is Starter with disk `dsk-dbvggvl9el5s73bqe9k0` (name `disk`, 1 GB, mounted at `/var/data`). Existing `MK_MASTER_KEY` unchanged (never regenerated).
- **Live env:** `MK_DATABASE_URL=sqlite:///var/data/masterkey.db?mode=rwc`, `MK_BACKUP_DIR=/var/data/backups`, `MK_BACKUP_KEEP=7`, `MK_BACKUP_INTERVAL_SECS=86400`. `MK_DEV_LOGIN` stays `0`; a valid-shaped dev login returns 404.
- **Deploy:** PR 12 (dev -> staging, merge 15c5da) and PR 13 (staging -> main, merge a60c9b); live deploy `dep-dbvgnanf3r2c73fv463g` at 14:49:38Z. `/v1/status` reports `ok` and `db` true.
- **Persistence check:** a pending connect row created BEFORE the deploy survived onto the new instance. This is real database persistence, not an app token being re-created from its sealed secret.
- **Backup check:** the automatic backup exists (mode 600, 135168 bytes). It was restored into `/tmp/prod-restore-check.db`: `PRAGMA integrity_check` ok and users/requests/api_keys counts match the backup. No sensitive content was read.
- **What the check proves:** a sidecar, non-destructive rehearsal. It is not a real rollback, not a deletion-log replay (`replay-deletions.sh`), and not an offsite disaster-recovery test.

### Staging (changed 2026-11-19, also verified)
- Staging disk `dsk-dbuielgm7kps73cgk420` is preserved. A consistent `VACUUM INTO` copy `/var/data/backups/pre-prod-20261119T1443.db` survived a restart, restored sidecar, integrity ok, counts match.
- Staging compute was manually SUSPENDED in the Dashboard (Render API `updatedAt` 2026-11-19T14:51:52Z) to prioritise prod cost. No disk deleted, no downgrade to free. Staging is unavailable until someone intentionally resumes it; `render.yaml` still says starter + disk, so it resumes safely. Do not point testers at staging while suspended.

## Remaining risks (unresolved)
- Backups live on the SAME disk as the database: they protect against bad deploys and mistakes, not disk loss. No offsite copy or independently held master-key escrow was verified in this session, because Joseph's password manager was not inspected.
- `MK_MASTER_KEY` escrow is unresolved (unverified, see above). Lose the key and every stored provider key is undecryptable (users re-add keys). Store a copy in a password manager.
- A restore with deletion-log replay has not been rehearsed on prod.
- Apple deletion revocation key (`MK_APPLE_SIGNIN_KEY_ID` / `_P8`) is still not set: deletion works and reports `apple_revoked:false`.

## Cost (estimate, not an invoice)
About $7.25/month for prod (Starter plus the 1 GB disk) plus about $0.25/month for the retained staging disk, so roughly $7.50/month baseline. Excludes overlap, bandwidth, build minutes and tax. The actual invoice has not been checked, so do not claim it as proven. Avoid duplicate paid resources. Verified from Render's docs: a disk attaches to exactly one service instance (prod and staging cannot share one) and cannot be added to a cron job.

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
       - key: MK_DATABASE_URL
-        value: sqlite:///data/masterkey.db?mode=rwc
+        value: sqlite:///var/data/masterkey.db?mode=rwc
+      - key: MK_BACKUP_DIR
+        value: /var/data/backups
+      - key: MK_BACKUP_KEEP
+        value: "7"
+      - key: MK_BACKUP_INTERVAL_SECS
+        value: "86400"
```
This is in `render.yaml` for prod and now matches the live service; staging has no backup env. `MK_MASTER_KEY` stays `generateValue: true` (Render keeps the existing value). `MK_DEV_LOGIN` stays `0`.

## Backups
The disk belongs to one service, so no cron job or GitHub Action can read it. The server does it itself: `MK_BACKUP_DIR` writes `masterkey-<utc>.db` (`VACUUM INTO`, consistent, mode 600) once per 24 h, counted from the newest file so restarts neither duplicate nor evict backups, keeps the newest `MK_BACKUP_KEEP` (7). Logged: path only. These live on the same disk (protects against bad deploys and mistakes, not disk loss); for off-disk copies run `backup.sh` plus `scp`/`render ssh` from an operator machine now and then. Test: `in_process_backup_runs_prunes_and_is_a_database`.

## Rollout (done 2026-11-19; kept as the procedure for future changes)
1. Disk and env vars confirmed on the live service before deploying.
2. dev -> staging -> main by PR; wait for `/v1/status` ok; create a throwaway record, redeploy, confirm it survives; confirm `/var/data/backups` has a file.
3. Keep `MK_DEV_LOGIN=0`. For deletion revocation set `MK_APPLE_SIGNIN_KEY_ID` + `MK_APPLE_SIGNIN_KEY_P8` in the dashboard (sync: false). Still open.
4. Keep a copy of `MK_MASTER_KEY` in a password manager; it cannot be recovered from the DB. Still open.

## Rollback
Redeploy the previous deploy in the dashboard. Data model changes are additive, so the old binary reads the new DB. Going back to free plan discards the disk data: take a backup first (`backup.sh`). This rollback path has not been exercised on prod.
