#!/usr/bin/env bash
# Starts the MasterKey backend in mock + dev mode, registers the demo app, logs in a demo user,
# stores a FAKE key, and prints how to run the example.
#   sdk/demo.sh                    # then approve on the iPhone app (dev login: demo@masterkey.dev)
#   sdk/demo.sh --simulate-phone   # a script plays the phone: approves connects and requests
# Env: PORT (8080), SIM_DELAY seconds before the fake phone approves a request (3).
set -euo pipefail
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PORT="${PORT:-8080}"; B="http://localhost:$PORT"; SIM_DELAY="${SIM_DELAY:-3}"
SIM=0; [[ "${1:-}" == "--simulate-phone" ]] && SIM=1
[[ "$PORT" =~ ^[0-9]{1,5}$ ]] && (( 10#$PORT >= 1 && 10#$PORT <= 65535 )) || { echo "PORT must be an integer from 1 to 65535" >&2; exit 1; }
# Check before building or creating demo data. Bash's /dev/tcp needs no extra package.
if (exec 3<>/dev/tcp/127.0.0.1/"$PORT") 2>/dev/null; then
  echo "port $PORT is already accepting connections; choose another PORT" >&2
  exit 1
fi
for c in cargo curl jq; do command -v "$c" >/dev/null || { echo "missing: $c" >&2; exit 1; }; done
[[ $SIM == 1 ]] && ! command -v sqlite3 >/dev/null && { echo "--simulate-phone needs sqlite3" >&2; exit 1; }

TMP="$(mktemp -d)"; DB="$TMP/demo.db"
(cd "$ROOT/backend" && cargo build -q)
MK_MASTER_KEY="$(openssl rand -base64 32)" MK_DATABASE_URL="sqlite://$DB?mode=rwc" PORT="$PORT" \
  MK_PUBLIC_URL="$B" MK_DEV_LOGIN=1 MK_VENDOR_MOCK=1 \
  MK_PUBLIC_APPS='[{"id":"app_resume_tailor","name":"Resume Tailor","allowed_origins":["http://localhost:3001","http://127.0.0.1:3001"]}]' "$ROOT/backend/target/debug/masterkey-server" >"$TMP/server.log" 2>&1 &
SERVER=$!
trap 'kill $SERVER ${SIMPID:-} 2>/dev/null; rm -rf "$TMP"' EXIT
READY=0
for _ in $(seq 50); do
  if ! kill -0 "$SERVER" 2>/dev/null; then break; fi
  if curl -sf "$B/healthz" >/dev/null; then READY=1; break; fi
  sleep 0.2
done
sleep 0.1
if [[ $READY != 1 ]] || ! kill -0 "$SERVER" 2>/dev/null; then
  echo "the demo server did not start on port $PORT:" >&2
  cat "$TMP/server.log" >&2
  exit 1
fi

SESS=$(curl -s "$B/v1/auth/dev" -d '{"email":"demo@masterkey.dev"}' | jq -r .session_token)
curl -s -o /dev/null -X PUT "$B/v1/me/keys/openai" -H "Authorization: Bearer $SESS" -d '{"api_key":"sk-FAKE-demo-key-0000"}'
curl -s -o /dev/null -X PUT "$B/v1/me/keys/anthropic" -H "Authorization: Bearer $SESS" -d '{"api_key":"sk-ant-FAKE-demo-key-0000"}'
APP=$(curl -s "$B/v1/apps" -d '{"name":"Chat with your key"}' | jq -r .app_secret)

cat <<EOF

MasterKey demo server: $B   (mock vendor: no real credits, fake key stored)
User login for the iPhone app (dev login): demo@masterkey.dev

Run the example in another terminal:
  cd $ROOT/sdk/examples/chat-with-your-key && npm install
  MK_URL=$B MK_APP_SECRET=$APP npm start
  open http://localhost:3000

Or the resume tailor, a static page with no server of its own (OpenAI + Anthropic models):
  cd $ROOT/examples/resume-tailor && python3 -m http.server 3001
  open "http://localhost:3001/?mk=$B"
EOF

if [[ $SIM == 1 ]]; then
  echo; echo "Simulated phone is on: approving connect codes and requests (delay ${SIM_DELAY}s). Ctrl-C to stop."
  (
    while true; do
      # the user API has no "list connect codes", so read pending codes from the demo DB
      for code in $(sqlite3 "$DB" "SELECT code FROM connect_codes WHERE status='pending'" 2>/dev/null); do
        sleep 1
        curl -s -o /dev/null "$B/v1/me/connect/$code/approve" -H "Authorization: Bearer $SESS" -d '{"policy":{"mode":"ask"}}' && echo "[phone] approved connect $code"
      done
      for id in $(curl -s "$B/v1/me/requests?status=pending" -H "Authorization: Bearer $SESS" | jq -r '.[].id'); do
        sleep "$SIM_DELAY"
        curl -s -o /dev/null "$B/v1/me/requests/$id/approve" -H "Authorization: Bearer $SESS" -d '{}' && echo "[phone] approved request $id"
      done
      sleep 0.5
    done
  ) &
  SIMPID=$!
else
  echo; echo "Approve connections and requests in the MasterKey iPhone app. Ctrl-C to stop."
fi
wait $SERVER
