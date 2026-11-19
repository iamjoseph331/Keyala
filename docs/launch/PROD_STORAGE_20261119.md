# Production-first storage: reviewed approach

Joseph requested on 2026-11-19: try using the staging disk for both services;
otherwise prioritize production. No storage changes have been applied yet.

## Platform findings

- One Render disk is accessible to exactly one service instance. Sharing it
  between prod and staging is unsupported, and would also mix their SQLite state.
- The disk-update API accepts name, sizeGB and mountPath, not a new service ID.
  There is no supported reassignment in that API. Moving capacity therefore means
  provisioning production storage and retiring staging storage after recovery is
  secured, rather than attaching the same disk twice.
- Disks require paid compute. Current checkout declares staging starter + 1 GB,
  prod free. The live plan, price and actual resource IDs still need inspection.

Sources read: https://render.com/docs/disks.md and
https://api-docs.render.com/reference/update-disk on 2026-11-19.

## Concrete intended end state

Production: one paid instance with 1 GB mounted at /var/data;
MK_DATABASE_URL=sqlite:///var/data/masterkey.db?mode=rwc;
MK_BACKUP_DIR=/var/data/backups; MK_BACKUP_KEEP=7; daily consistent SQLite backups.
Keep its existing MK_MASTER_KEY and keep public dev login disabled.

Staging: return to free/ephemeral storage only after saving and verifying a
consistent backup plus its deletion log and retaining its separate encryption
master key. Its own MK_DATABASE_URL then points to /data/masterkey.db. Never
restore staging accounts or keys into production, and never share master keys.

This aims to move the existing paid allocation to production, rather than leave
two paid services running. A short overlap may be needed for safe migration;
verify exact dashboard charges before any paid mutation.

## Data safety before applying

The existing production database must be inventoried and preserved if accessible.
Do not call free-plan data disposable merely because it is ephemeral. Adding a
disk, changing compute or deploying can restart the instance. If free-service
Shell/SSH access prevents exporting existing state, explain that limitation and
obtain a decision about any data reset before proceeding.

After production is on persistent storage, prove a write survives a controlled
restart, run a consistent backup/restore rehearsal, and replay deletion records
before serving restored data. Only then retire the staging disk. Disk deletion
must not happen before its backup and restore evidence are verified.

Same-disk backups do not protect against losing the entire disk. Off-disk
encrypted recovery copies and separately held master keys remain an operator
responsibility; no stronger durability claim is published.

## Current blockers

The Render connector returned no selected workspace and explicitly required user
confirmation of Joseph's workspace (tea-d9e7jnj7uimc73f4jf0g). A confirmation
question is pending. The Mac is locked, blocking Dashboard disk/compute controls
that the connector does not expose. Source work and local verification can finish
without either action; live mutations and publication remain pending.
