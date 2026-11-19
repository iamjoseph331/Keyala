#!/usr/bin/env bash
# After restoring a backup, delete again every account that was deleted since the backup was taken.
#   docs/ops/replay-deletions.sh RESTORED_DB [deleted-accounts.log]
# The log (written by the server next to the live database, one `deleted <user_id> <time>` line per
# deletion, opaque ids only; keep the table list below in step with backend/src/ops.rs delete_account) must be the one from BEFORE the restore. Run with the server stopped.
set -euo pipefail
DB=${1:?usage: replay-deletions.sh RESTORED_DB [deleted-accounts.log]}
LOG=${2:-$(dirname "$DB")/deleted-accounts.log}
[[ -f $DB ]] || { echo "no such database: $DB" >&2; exit 1; }
[[ -f $LOG ]] || { echo "no deletion log at $LOG: nothing to replay"; exit 0; }
n=0
# last line per id wins: an `undeleted` line cancels an earlier `deleted` (the commit failed after the tombstone)
while read -r id; do
  [[ $id =~ ^usr_[A-Za-z0-9_-]+$ ]] || continue
  sqlite3 "$DB" "PRAGMA foreign_keys=ON; BEGIN;
    DELETE FROM connect_codes WHERE grant_id IN (SELECT id FROM grants WHERE user_id='$id');
    DELETE FROM requests WHERE user_id='$id'; DELETE FROM grants WHERE user_id='$id';
    DELETE FROM api_keys WHERE user_id='$id'; DELETE FROM devices WHERE user_id='$id';
    DELETE FROM sessions WHERE user_id='$id'; DELETE FROM enrollments WHERE user_id='$id';
    DELETE FROM oauth_flows WHERE user_id='$id'; DELETE FROM users WHERE id='$id'; COMMIT;"
  n=$((n+1))
done < <(awk '$1=="deleted"||$1=="undeleted"{s[$2]=$1} END{for(i in s) if(s[i]=="deleted") print i}' "$LOG")
echo "replayed $n deletion(s) from $LOG"
