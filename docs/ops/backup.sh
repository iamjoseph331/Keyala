#!/usr/bin/env bash
# Consistent online backup of the MasterKey SQLite database (safe while the server runs).
#   docs/ops/backup.sh [DB_FILE] [OUT_DIR]      (or env MK_DB, MK_BACKUP_OUT)
# Never reads, prints or copies MK_MASTER_KEY: the backup holds users, history and ENCRYPTED provider
# keys, and is useless without the master key, which must be stored separately.
set -euo pipefail
DB=${1:-${MK_DB:-/var/data/masterkey.db}}
OUT_DIR=${2:-${MK_BACKUP_OUT:-$(dirname "$DB")/backups}}
command -v sqlite3 >/dev/null || { echo "sqlite3 not found" >&2; exit 1; }
[[ -f $DB ]] || { echo "no such database: $DB" >&2; exit 1; }
mkdir -p "$OUT_DIR"
OUT="$OUT_DIR/masterkey-$(date -u +%Y%m%dT%H%M%S000Z).db"
[[ -e $OUT ]] && { echo "refusing to overwrite $OUT" >&2; exit 1; }
umask 077
sqlite3 "$DB" "VACUUM INTO '${OUT//\'/\'\'}'"
chmod 600 "$OUT"
echo "backup:    $OUT"
echo "size:      $(wc -c <"$OUT" | tr -d ' ') bytes"
CHECK=$(sqlite3 "$OUT" 'PRAGMA integrity_check')
echo "integrity: $CHECK"
[[ $CHECK == ok ]] || { echo "BACKUP FAILED INTEGRITY CHECK" >&2; exit 1; }
