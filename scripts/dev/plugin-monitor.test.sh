#!/usr/bin/env bash
# The runner monitor says something exactly when something changed.
#
# Every line a monitor prints becomes a notification in the session, so
# what is under test is as much the silence as the speech: a watch that
# narrates each poll gets switched off, and then it reports nothing at all
# when it matters.
#
# Device-free. A trivial local HTTP server stands in for the runner, and
# starting or stopping it is the event.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WATCH="$ROOT/plugin/scripts/watch-runner.sh"

pass=0
fail=0
check() { # <name> <condition-description> <actual> <expected>
  if [ "$3" = "$4" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FAIL: %s — expected %s, got %s\n' "$1" "$4" "$3" >&2
  fi
}

[ -x "$WATCH" ] || { echo "watch-runner.sh missing or not executable" >&2; exit 1; }

# A port nothing else is on. The monitor only ever reads /health.
PORT=22193
SERVER_PID=""
start_server() {
  python3 -c "
import http.server, socketserver, sys
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200); self.end_headers(); self.wfile.write(b'ok')
    def log_message(self, *a): pass
socketserver.TCPServer.allow_reuse_address = True
with socketserver.TCPServer(('127.0.0.1', $PORT), H) as s:
    s.serve_forever()
" &
  SERVER_PID=$!
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    curl -sf -m 1 "http://127.0.0.1:$PORT/health" >/dev/null 2>&1 && return 0
    sleep 0.3
  done
  return 1
}
stop_server() {
  [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null
  wait "$SERVER_PID" 2>/dev/null
  SERVER_PID=""
}
trap stop_server EXIT

run_watch() { # <iterations> -> stdout
  env SMIX_RUNNER_PORT="$PORT" SMIX_WATCH_INTERVAL_S=0.3 \
      SMIX_WATCH_ITERATIONS="$1" bash "$WATCH" 2>/dev/null
}

# 1. A healthy runner is not news.
start_server || { echo "could not start the stand-in server" >&2; exit 1; }
OUT="$(run_watch 3)"
check "steady up is silent" "" "$(printf '%s' "$OUT" | tr -d '[:space:]')" ""

# 2. Going away is the event this exists for.
# Killed quietly: the shell announcing a terminated job is noise in a
# test whose subject is what gets said.
( sleep 0.8; kill "$SERVER_PID" 2>/dev/null ) & disown 2>/dev/null || true
OUT="$(run_watch 8)"
SERVER_PID=""
case "$OUT" in
  *"$PORT"*stopped*) check "loss is reported with the port" "yes" "yes" "yes" ;;
  *) check "loss is reported with the port" "yes" "no($OUT)" "yes" ;;
esac
check "loss is reported once" "1" "$(printf '%s\n' "$OUT" | grep -c 'stopped answering')" "1"

# 3. A session that never started a runner is not told it is down.
OUT="$(run_watch 3)"
check "never-up stays silent" "" "$(printf '%s' "$OUT" | tr -d '[:space:]')" ""

printf 'plugin-monitor: %d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
