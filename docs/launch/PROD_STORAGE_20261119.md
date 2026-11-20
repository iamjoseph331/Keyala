# Production storage: verified 2026-11-19

Joseph approved the production data reset and personally completed the Starter
compute upgrade and disk purchase. Production and staging cannot share a Render
disk; production now has its own storage. No staging accounts or keys were copied.

## Live state

- Production: `srv-dbsm5m0ae00c738euic0`, Starter, 1 GB disk
  `dsk-dbvggvl9el5s73bqe9k0`, mounted at `/var/data`.
- SQLite: `sqlite:///var/data/masterkey.db?mode=rwc`. Existing master key preserved.
- Daily consistent SQLite backups: `/var/data/backups`, newest seven retained.
- Staging: `srv-dbsm5m0ae00c738euib0`, compute suspended, original 1 GB disk
  `dsk-dbuielgm7kps73cgk420` retained. It is unavailable until deliberately resumed.
- Approximate baseline: production US$7.25/month plus retained staging disk
  US$0.25/month. This excludes overlap, additional usage and taxes.

## Evidence

PR #12 promoted to staging (`15c5da0`); PR #13 promoted to main (`a60c9b7`).
Production deploy `dep-dbvgnanf3r2c73fv463g` became live at 14:49:38 UTC.
A pending connection created before deployment remained accessible afterward.
This checks a stored row, rather than only an app secret that can reconstruct an app.

Production automatic backup existed with mode 600 and size 135168 bytes. It was
restored into `/tmp/prod-restore-check.db`, leaving the live database untouched.
`PRAGMA integrity_check` returned `ok`; users, requests and api_keys counts matched
the backup. No credentials or request contents were printed.

Before staging suspension, a consistent `VACUUM INTO` backup was saved at
`/var/data/backups/pre-prod-20261119T1443.db`. It survived a service restart and
passed the same sidecar restore integrity/count checks. The original staging disk
and separate master-key configuration remain in place. No disk was deleted.

Production `/v1/status` reports database and service healthy. `/about`, `/privacy`,
`/add-key` and AASA return successfully; `/add-key` links to privacy. A valid-shaped
public dev-login request returns 404. GitHub Pages privacy and support return 200.

## Remaining recovery limits

Same-disk backups do not protect against losing the disk. An encrypted off-disk
copy and independently retained encryption master key still need operator setup.
This rehearsal did not replace the live database or prove an end-to-end recovery
with deletion-record replay. Follow `docs/ops/RUNBOOK.md` before any real restore.
Do not delete staging storage or downgrade it to ephemeral storage as a shortcut.

Platform references read: https://render.com/docs/disks.md and
https://api-docs.render.com/reference/update-disk.
