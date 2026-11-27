#!/usr/bin/env bash
# After restoring a backup, delete again every account that was deleted since the backup was taken.
#   docs/ops/replay-deletions.sh RESTORED_DB [deleted-accounts.log]
# The log (written by the server next to the live database, one `deleted <user_id> <time>` line per
# deletion, opaque ids only) must be the one from BEFORE the restore. Run with the server stopped.
# Keep the statements below in step with backend/src/ops.rs delete_account and backend/src/coupons.rs detach_user.
# Works on backups from before and after migration 0017 (coupons) and before 0014/0016: a table or column the
# backup does not have is skipped. Each account is replayed in one transaction with foreign keys ON.
#
# Coupons, replayed exactly as the server does them:
#  * a deleted issuer's batches are revoked (provider key wiped), other people's waiting calls on them are denied and
#    their holds released, every open coupon's unused balance returns to the batch, stored mail addresses are erased;
#  * a deleted recipient's waiting calls are denied, unused balance returns, and the coupon is detached from the
#    account. A call whose outcome is unknown stays HELD (never counted as spent): that coupon is kept with no user
#    id so the money stays accounted for; its request row and content are deleted with the account.
set -euo pipefail
DB=${1:?usage: replay-deletions.sh RESTORED_DB [deleted-accounts.log]}
LOG=${2:-$(dirname "$DB")/deleted-accounts.log}
[[ -f $DB ]] || { echo "no such database: $DB" >&2; exit 1; }
[[ -f $LOG ]] || { echo "no deletion log at $LOG: nothing to replay"; exit 0; }
has_table() { [[ -n $(sqlite3 "$DB" "SELECT 1 FROM sqlite_master WHERE type='table' AND name='$1'") ]]; }
has_col() { [[ -n $(sqlite3 "$DB" "SELECT 1 FROM pragma_table_info('$1') WHERE name='$2'") ]]; }

# $1 = user id (already checked against ^usr_[A-Za-z0-9_-]+$), $2 = timestamp
sql_for() {
  local id=$1 t=$2
  echo "PRAGMA foreign_keys=ON; BEGIN;"
  if has_table coupon_batches; then cat <<SQL
CREATE TEMP TABLE tgt_batch AS SELECT id FROM coupon_batches WHERE owner_user_id='$id';
CREATE TEMP TABLE tgt_coupon AS SELECT id FROM coupons WHERE user_id='$id' OR batch_id IN (SELECT id FROM tgt_batch);
-- calls that never left: denied, their holds go back to the coupon
CREATE TEMP TABLE rel AS SELECT r.id AS req, h.coupon_id AS coupon, h.batch_id AS batch, h.micro AS micro, h.model AS model
  FROM requests r JOIN coupon_ledger h ON h.ukey='hold:'||r.id
  WHERE r.coupon_id IN (SELECT id FROM tgt_coupon) AND r.status IN ('pending','approved')
  AND NOT EXISTS (SELECT 1 FROM coupon_ledger c WHERE c.ukey='close:'||r.id);
INSERT INTO coupon_ledger (batch_id,coupon_id,request_id,kind,micro,ukey,model,created_at)
  SELECT batch,coupon,req,'release',0,'close:'||req,model,'$t' FROM rel;
UPDATE coupons SET held_micro=held_micro-(SELECT SUM(micro) FROM rel WHERE coupon=coupons.id),
  remaining_micro=remaining_micro+(SELECT SUM(micro) FROM rel WHERE coupon=coupons.id) WHERE id IN (SELECT coupon FROM rel);
UPDATE requests SET status='denied', decided_at='$t' WHERE coupon_id IN (SELECT id FROM tgt_coupon) AND status IN ('pending','approved');
-- unused balance of every open coupon returns to its batch, once
INSERT OR IGNORE INTO coupon_ledger (batch_id,coupon_id,request_id,kind,micro,ukey,model,created_at)
  SELECT batch_id,id,NULL,'return',remaining_micro,'return:'||id,NULL,'$t' FROM coupons WHERE id IN (SELECT id FROM tgt_coupon) AND closed_at IS NULL;
UPDATE coupon_batches SET available_micro=available_micro+COALESCE((SELECT SUM(remaining_micro) FROM coupons
  WHERE batch_id=coupon_batches.id AND closed_at IS NULL AND id IN (SELECT id FROM tgt_coupon)),0)
  WHERE id IN (SELECT batch_id FROM coupons WHERE closed_at IS NULL AND id IN (SELECT id FROM tgt_coupon));
UPDATE coupons SET remaining_micro=0, closed_at='$t' WHERE closed_at IS NULL AND id IN (SELECT id FROM tgt_coupon);
-- issuer: batch revoked, key wiped, no address kept
UPDATE coupon_batches SET revoked_at=COALESCE(revoked_at,'$t'), key_ct=NULL, key_nonce=NULL, owner_user_id=NULL WHERE id IN (SELECT id FROM tgt_batch);
UPDATE coupon_outbox SET status=CASE WHEN status IN ('pending','sending') THEN 'skipped' ELSE status END,
  last_error=CASE WHEN status IN ('pending','sending') THEN 'issuer_deleted' ELSE last_error END,
  from_addr=NULL, recipient=NULL, subject=NULL, body=NULL WHERE batch_id IN (SELECT id FROM tgt_batch);
-- recipient: whatever is still held stays, anonymously
UPDATE coupons SET user_id=NULL WHERE user_id='$id' AND held_micro>0;
SQL
  fi
  echo "DELETE FROM connect_codes WHERE grant_id IN (SELECT id FROM grants WHERE user_id='$id');"
  echo "DELETE FROM requests WHERE user_id='$id'; DELETE FROM grants WHERE user_id='$id';"
  has_table coupons && echo "DELETE FROM coupons WHERE user_id='$id';"
  for tb in api_keys devices sessions enrollments oauth_flows web_logins passkeys passkey_challenges; do
    has_table "$tb" && echo "DELETE FROM $tb WHERE user_id='$id';"
  done
  has_col apps owner_user_id && echo "UPDATE apps SET owner_user_id=NULL WHERE owner_user_id='$id';"
  echo "DELETE FROM users WHERE id='$id'; COMMIT;"
  return 0
}

n=0
NOW=$(date -u +%Y-%m-%dT%H:%M:%S.000Z)
# last line per id wins: an `undeleted` line cancels an earlier `deleted` (the commit failed after the tombstone)
while read -r id; do
  [[ $id =~ ^usr_[A-Za-z0-9_-]+$ ]] || continue
  sql_for "$id" "$NOW" | sqlite3 -bail "$DB"
  n=$((n+1))
done < <(awk '$1=="deleted"||$1=="undeleted"{s[$2]=$1} END{for(i in s) if(s[i]=="deleted") print i}' "$LOG")
echo "replayed $n deletion(s) from $LOG"
