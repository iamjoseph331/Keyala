#!/usr/bin/env bash
# Local backup/restore drill with mock data (no real keys, no network). Run: bash docs/ops/restore-drill.sh
# Proves: a backup taken while the server runs restores into a new database; stored provider keys
# still decrypt with the SAME master key and do NOT with a different one; deletion replay works.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
for c in cargo curl jq sqlite3 openssl python3; do command -v "$c" >/dev/null || { echo "missing: $c" >&2; exit 1; }; done
TMP=$(mktemp -d); PIDS=()
trap 'for p in "${PIDS[@]:-}"; do [[ -n $p ]] && kill "$p" 2>/dev/null || true; done; rm -rf "$TMP"' EXIT
(cd "$ROOT/backend" && cargo build -q)
BIN="$ROOT/backend/target/debug/keyala-server"
FAILS=0
check() { if [[ $2 == "$3" ]]; then echo "PASS  $1"; else echo "FAIL  $1 (got '$2', want '$3')"; FAILS=$((FAILS+1)); fi; }

# start_server DB MASTER_KEY -> sets B (base URL) and PID
start_server() {
  local port; port=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
  KEYALA_MASTER_KEY=$2 KEYALA_DATABASE_URL="sqlite://$1?mode=rwc" PORT=$port KEYALA_DEV_LOGIN=1 KEYALA_VENDOR_MOCK=1 "$BIN" >"$TMP/server-$port.log" 2>&1 &
  PID=$!; disown "$PID"; PIDS+=("$PID"); B="http://127.0.0.1:$port"
  for _ in $(seq 100); do curl -sf "$B/healthz" >/dev/null && return 0; kill -0 "$PID" 2>/dev/null || break; sleep 0.1; done
  echo "server failed to start:"; cat "$TMP/server-$port.log"; exit 1
}
# one mock request: ask + wait 0, approve on the "phone", wait for the final status; prints the request id
run_request() { # TOKEN SESSION
  local rid st
  rid=$(curl -s "$B/v1/openai/chat/completions" -H "Authorization: Bearer $1" -H 'X-Keyala-Wait: 0' \
        -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"drill"}],"max_tokens":16}' | jq -r .request_id)
  curl -s -o /dev/null "$B/v1/me/requests/$rid/approve" -H "Authorization: Bearer $2" -d '{}'
  for _ in $(seq 100); do
    st=$(curl -s "$B/v1/me/requests/$rid" -H "Authorization: Bearer $2" | jq -r .status)
    [[ $st == completed || $st == failed ]] && break; sleep 0.1
  done
  echo "$rid"
}

MK1=$(openssl rand -base64 32); MK2=$(openssl rand -base64 32)
echo "== 1. original server: user, fake key, app, grant, one completed request"
start_server "$TMP/a.db" "$MK1"; PID_A=$PID
LOGIN=$(curl -s "$B/v1/auth/dev" -d '{"email":"drill@example.test"}')
SESS=$(jq -r .session_token <<<"$LOGIN"); UID_=$(jq -r .user.id <<<"$LOGIN")
curl -s -o /dev/null -X PUT "$B/v1/me/keys/openai" -H "Authorization: Bearer $SESS" -d '{"api_key":"sk-proj-DRILLFAKE-KEY-1234"}'
APP=$(curl -s "$B/v1/apps" -d '{"name":"Drill app"}' | jq -r .app_secret)
CODE=$(curl -s "$B/v1/connect" -H "Authorization: Bearer $APP" -d '{"vendor":"openai"}' | jq -r .connect_code)
curl -s -o /dev/null "$B/v1/me/connect/$CODE/approve" -H "Authorization: Bearer $SESS" -d '{"policy":{"mode":"ask"}}'
TOKEN=$(curl -s "$B/v1/connect/$CODE" -H "Authorization: Bearer $APP" | jq -r .grant_token)
R1=$(run_request "$TOKEN" "$SESS")
check "original: request completed" "$(curl -s "$B/v1/me/requests/$R1" -H "Authorization: Bearer $SESS" | jq -r .status)" completed

echo "== 2. backup while the server is running"
OUT=$("$ROOT/docs/ops/backup.sh" "$TMP/a.db" "$TMP/backups"); echo "$OUT"
BK=$(awk '/^backup:/{print $2}' <<<"$OUT")
check "backup integrity" "$(sqlite3 "$BK" 'PRAGMA integrity_check')" ok
check "backup mode 600" "$(stat -f %Lp "$BK" 2>/dev/null || stat -c %a "$BK")" 600
kill "$PID_A"; sleep 0.5

echo "== 3. restore into a NEW database, same master key"
cp "$BK" "$TMP/b.db"; start_server "$TMP/b.db" "$MK1"
check "restored: key last4 listed" "$(curl -s "$B/v1/me/keys" -H "Authorization: Bearer $SESS" | jq -r '.[0].last4')" 1234
check "restored: history has the request" "$(curl -s "$B/v1/me/requests" -H "Authorization: Bearer $SESS" | jq -r '[.[]|select(.status=="completed")]|length')" 1
R2=$(run_request "$TOKEN" "$SESS")
check "restored: key decrypts (mock saw last4)" "$(curl -s "$B/v1/me/requests/$R2" -H "Authorization: Bearer $SESS" | jq -r .response.mock_key_last4)" 1234

echo "== 4. restore with a DIFFERENT master key"
cp "$BK" "$TMP/c.db"; start_server "$TMP/c.db" "$MK2"
# provider keys AND each user's data key are sealed with the master key: the server refuses clearly, writes nothing
D=$(curl -s "$B/v1/openai/chat/completions" -H "Authorization: Bearer $TOKEN" -H 'X-Keyala-Wait: 0' \
      -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"drill"}],"max_tokens":16}')
check "wrong master key: new request refused" "$(jq -r .error.type <<<"$D")" data_key_unreadable
check "wrong master key: history refused" "$(curl -s "$B/v1/me/requests/$R1" -H "Authorization: Bearer $SESS" | jq -r .error.type)" data_key_unreadable
check "wrong master key: nothing stored" "$(sqlite3 "$TMP/c.db" "SELECT COUNT(*) FROM requests WHERE payload LIKE '%drill%'")" 0

echo "== 5. replay of deletions recorded after the backup"
echo "deleted $UID_ 2026-01-01T00:00:00.000Z" >"$TMP/deleted-accounts.log"
cp "$BK" "$TMP/d.db"
"$ROOT/docs/ops/replay-deletions.sh" "$TMP/d.db" "$TMP/deleted-accounts.log"
check "replay: user gone" "$(sqlite3 "$TMP/d.db" "SELECT COUNT(*) FROM users WHERE id='$UID_'")" 0
check "replay: requests gone" "$(sqlite3 "$TMP/d.db" "SELECT COUNT(*) FROM requests")" 0

if [[ $FAILS == 0 ]]; then echo; echo "RESTORE DRILL: PASS"; else echo; echo "RESTORE DRILL: FAIL ($FAILS)"; exit 1; fi
