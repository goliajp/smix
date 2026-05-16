#!/usr/bin/env bash
# simx-c6-tap-e2e.sh — end-to-end smoke for v0.2 C6:
# SDK app.tap({text}) → SimctlDriver.tap → RunnerClient → POST /tap → XCUITest.
#
#   1. Boot C2 runner (SimxRunnerUITests/test_runForever) → auto-launches Settings.
#   2. Wait for /health = 200.
#   3. Run `bun src/cli/index.ts run examples/tap-text-selector.test.ts` → expect exit 0
#      and stdout contains "1 passed, 0 failed".
#   4. Post-probe (double-sided machine-checkable evidence):
#        a. POST /tap "Display & Brightness" → expect 404 (cell only exists
#           on root list, gone after push to General).
#        b. POST /tap "About" → expect 200 (positive: General sub-page cell).
#   5. Emit single-line summary JSON, exit 0.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="$ROOT/swift-bridge/SimxRunner.xcodeproj"
SCHEME="SimxRunner"
PORT="${SIMX_RUNNER_PORT:-22087}"

MISS_TEXT="${SIMX_PROBE_MISS_TEXT:-Display & Brightness}"
HIT_TEXT="${SIMX_PROBE_HIT_TEXT:-About}"

# Phase 0: dev sim UDID + state check (mirrors simx-hostid-digitizer-smoke.sh).
if [[ -z "${SIMX_UDID:-}" ]]; then
  if [[ -f "$ROOT/.simx/dev-sim.txt" ]]; then
    SIMX_UDID="$(tr -d '[:space:]' < "$ROOT/.simx/dev-sim.txt")"
  else
    SIMX_UDID="$(xcrun simctl list devices -j \
      | jq -r '[.devices | to_entries[] | .value[] | select(.isAvailable==true and .state=="Booted")][0].udid // empty')"
  fi
fi
if [[ -z "${SIMX_UDID:-}" ]]; then
  echo "no booted iOS simulator (set SIMX_UDID or boot a sim)" >&2
  exit 2
fi

DEV_STATE="$(xcrun simctl list devices -j \
  | jq -r --arg u "$SIMX_UDID" \
        '.devices | to_entries[] | .value[] | select(.udid==$u) | .state // empty')"
if [[ "$DEV_STATE" != "Booted" ]]; then
  echo "dev sim $SIMX_UDID is not Booted (state=$DEV_STATE)" >&2
  exit 2
fi

# Port must be free.
if lsof -iTCP:"$PORT" -sTCP:LISTEN > /dev/null 2>&1; then
  echo "port $PORT already in use; clean it before retry" >&2
  exit 2
fi

# Start from a known cold-Settings state.
xcrun simctl terminate "$SIMX_UDID" com.apple.Preferences > /dev/null 2>&1 || true

LOGDIR="$ROOT/.simx/runner"
mkdir -p "$LOGDIR"
LOGFILE="$LOGDIR/xcodebuild-c6-$$.log"
PIDFILE="$LOGDIR/runner-c6-$$.pid"

# Phase A: kick xcodebuild test backgrounded.
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
  xcrun simctl terminate "$SIMX_UDID" com.apple.Preferences > /dev/null 2>&1 || true
  rm -f "${MISS_BODY:-}" "${HIT_BODY:-}" 2>/dev/null || true
}
trap cleanup EXIT

# Phase B: poll /health up to 150s (cold xcodebuild + Settings launch).
HEALTH_STATUS=""
for _ in $(seq 1 150); do
  HEALTH_STATUS="$(curl -sS -o /dev/null -w '%{http_code}' \
                       --max-time 2 "http://127.0.0.1:$PORT/health" 2>/dev/null || echo "")"
  if [[ "$HEALTH_STATUS" = "200" ]]; then
    break
  fi
  sleep 1
done
if [[ "$HEALTH_STATUS" != "200" ]]; then
  echo "health did not reach 200 within 150s" >&2
  tail -40 "$LOGFILE" >&2 || true
  exit 1
fi

# Phase C: run the SDK test through simx CLI.
SIMX_OUT="/tmp/simx-c6-cli-$$.out"
SIMX_ERR="/tmp/simx-c6-cli-$$.err"
SIMX_EXIT=0
SIMX_UDID="$SIMX_UDID" bun "$ROOT/src/cli/index.ts" run \
  "$ROOT/examples/tap-text-selector.test.ts" \
  --udid "$SIMX_UDID" \
  > "$SIMX_OUT" 2> "$SIMX_ERR" || SIMX_EXIT=$?

CASE_PASSED="$(grep -oE '^[0-9]+ passed' "$SIMX_OUT" 2>/dev/null | head -1 | awk '{print $1}')"
CASE_FAILED="$(grep -oE '[0-9]+ failed' "$SIMX_OUT" 2>/dev/null | head -1 | awk '{print $1}')"
CASE_PASSED="${CASE_PASSED:-0}"
CASE_FAILED="${CASE_FAILED:-0}"

if [[ "$SIMX_EXIT" -ne 0 ]] || [[ "$CASE_PASSED" != "1" ]] || [[ "$CASE_FAILED" != "0" ]]; then
  echo "simx run failed: exit=$SIMX_EXIT passed=$CASE_PASSED failed=$CASE_FAILED" >&2
  echo "--- simx stdout ---" >&2
  cat "$SIMX_OUT" >&2 || true
  echo "--- simx stderr ---" >&2
  cat "$SIMX_ERR" >&2 || true
  echo "--- xcodebuild tail ---" >&2
  tail -60 "$LOGFILE" >&2 || true
  exit 1
fi

# Settle: let Settings push-animation finish (UIKit default ~0.35s + buffer).
sleep 2

# Phase D — probe after MISS: "Display & Brightness" must be gone from
# General sub-page. Expect 404 + .ok=false + .error="not_found".
MISS_BODY="/tmp/simx-c6-probe-miss-$$.json"
MISS_STATUS="$(curl -sS -o "$MISS_BODY" -w '%{http_code}' \
                   --max-time 15 \
                   -X POST -H 'Content-Type: application/json' \
                   -d "$(jq -Rn --arg t "$MISS_TEXT" '{selector:{text:$t}}')" \
                   "http://127.0.0.1:$PORT/tap" 2>/dev/null || echo "000")"
if [[ "$MISS_STATUS" != "404" ]]; then
  echo "probe_after_miss expected 404, got $MISS_STATUS" >&2
  cat "$MISS_BODY" >&2 || true
  tail -60 "$LOGFILE" >&2 || true
  exit 1
fi
if ! jq -e '.ok == false and .error == "not_found"' "$MISS_BODY" > /dev/null; then
  echo "probe_after_miss body schema wrong" >&2
  cat "$MISS_BODY" >&2
  exit 1
fi

# Phase E — probe after HIT: "About" must exist on General sub-page.
# Expect 200 + .ok=true.
HIT_BODY="/tmp/simx-c6-probe-hit-$$.json"
HIT_STATUS="$(curl -sS -o "$HIT_BODY" -w '%{http_code}' \
                  --max-time 15 \
                  -X POST -H 'Content-Type: application/json' \
                  -d "$(jq -Rn --arg t "$HIT_TEXT" '{selector:{text:$t}}')" \
                  "http://127.0.0.1:$PORT/tap" 2>/dev/null || echo "000")"
if [[ "$HIT_STATUS" != "200" ]]; then
  echo "probe_after_hit expected 200, got $HIT_STATUS" >&2
  cat "$HIT_BODY" >&2 || true
  tail -60 "$LOGFILE" >&2 || true
  exit 1
fi
if ! jq -e '.ok == true' "$HIT_BODY" > /dev/null; then
  echo "probe_after_hit body schema wrong" >&2
  cat "$HIT_BODY" >&2
  exit 1
fi

# Phase F — single-line JSON summary on stdout.
printf '{"health":%s,"case_passed":%s,"case_failed":%s,"probe_after_miss":%s,"probe_after_hit":%s}\n' \
       "$HEALTH_STATUS" "$CASE_PASSED" "$CASE_FAILED" "$MISS_STATUS" "$HIT_STATUS"

rm -f "$SIMX_OUT" "$SIMX_ERR"
exit 0
