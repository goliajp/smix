#!/usr/bin/env bash
# simx-runner-health.sh — boot the SimxRunner XCUITest runner and curl /health.
# WHY: v0.2 C1 verification harness; replaces hand-typing the long xcodebuild line.
# Not wired into any TS/CLI yet (deferred to v0.2 C5+).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="$ROOT/swift-bridge/SimxRunner.xcodeproj"
SCHEME="SimxRunner"
PORT="${SIMX_RUNNER_PORT:-22087}"

if [[ -z "${SIMX_UDID:-}" ]]; then
  # Prefer the locked dev sim (created by `simctl create simx-dev ...`),
  # fall back to the first booted available sim if no lock file exists.
  if [[ -f "$ROOT/.simx/dev-sim.txt" ]]; then
    SIMX_UDID="$(cat "$ROOT/.simx/dev-sim.txt" | tr -d '[:space:]')"
  else
    SIMX_UDID="$(xcrun simctl list devices -j \
      | jq -r '[.devices | to_entries[] | .value[] | select(.isAvailable==true and .state=="Booted")][0].udid // empty')"
  fi
fi
if [[ -z "$SIMX_UDID" ]]; then
  echo "no booted iOS simulator (set SIMX_UDID or boot a sim)" >&2
  exit 2
fi

# Refuse to start if the port is already in use — avoids fighting another runner.
if lsof -iTCP:"$PORT" -sTCP:LISTEN > /dev/null 2>&1; then
  echo "port $PORT already in use; clean it before retry" >&2
  lsof -iTCP:"$PORT" -sTCP:LISTEN >&2 || true
  exit 2
fi

LOGDIR="$ROOT/.simx/runner"
mkdir -p "$LOGDIR"
LOGFILE="$LOGDIR/xcodebuild-$$.log"
PIDFILE="$LOGDIR/runner-$$.pid"

# Kick xcodebuild test in background; test_runForever never returns.
( xcodebuild -project "$PROJECT" \
             -scheme "$SCHEME" \
             -destination "platform=iOS Simulator,id=$SIMX_UDID" \
             -only-testing:SimxRunnerUITests/SimxRunnerUITests/test_runForever \
             test > "$LOGFILE" 2>&1 ) &
echo $! > "$PIDFILE"

cleanup() {
  local pid; pid="$(cat "$PIDFILE" 2>/dev/null || true)"
  if [[ -n "$pid" ]]; then
    pkill -P "$pid" 2>/dev/null || true
    kill -TERM "$pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT

# Poll /health up to 120s (cold install + boot can be slow on first run).
STATUS=""
for i in $(seq 1 120); do
  if STATUS="$(curl -sS -o /tmp/simx-health-$$.body -w '%{http_code}' \
                     --max-time 2 "http://127.0.0.1:$PORT/health" 2>/dev/null)"; then
    if [[ "$STATUS" = "200" ]]; then
      BODY="$(cat /tmp/simx-health-$$.body)"
      echo "{\"status\":$STATUS,\"body\":$BODY}"
      rm -f /tmp/simx-health-$$.body
      exit 0
    fi
  fi
  sleep 1
done

echo "runner did not respond on :$PORT within 120s" >&2
echo "--- xcodebuild log tail ---" >&2
tail -40 "$LOGFILE" >&2 || true
exit 1
