#!/usr/bin/env bash
# simx-hostid-digitizer-smoke.sh — end-to-end smoke for the C4 IOHIDEvent
# digitizer-tree tap path:
#   1. Boot C2 runner (SimxRunnerUITests/test_runForever) → auto-launches Settings.
#   2. Wait for /health = 200.
#   3. Invoke simx-host-hid tap --x 0.5 --y 0.25 → must hit Settings root list's
#      General cell and push to General sub-page.
#   4. Post-probe (double-sided machine-checkable evidence):
#        a. POST /tap "Display & Brightness" → expect 404 (negative: cell
#           only exists on root list, gone after push to General).
#        b. POST /tap "About" → expect 200 (positive: General sub-page cell).
#   5. Emit single-line summary JSON, exit 0.
#
# Settings launches with English locale (matches SimxRunner-UITests config).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="$ROOT/swift-bridge/SimxRunner.xcodeproj"
SCHEME="SimxRunner"
PORT="${SIMX_RUNNER_PORT:-22087}"

TAP_X="${SIMX_TAP_X:-0.5}"
# C4 deviation from plan-hot.md §S2 (which locked 0.25): on iPhone 17 Pro /
# iOS 26.4 the Settings root layout puts a large "Apple Account" cell from
# y~0.18 to y~0.31 (occupying the y=0.25 region the plan targeted). The
# `General` cell sits at y~0.48. Verified by before/after screenshots
# during C4 real validation; tap delivery itself is unaffected.
TAP_Y="${SIMX_TAP_Y:-0.48}"

MISS_TEXT="${SIMX_PROBE_MISS_TEXT:-Display & Brightness}"
HIT_TEXT="${SIMX_PROBE_HIT_TEXT:-About}"

HOSTID_BIN="$ROOT/swift-bridge/.build/debug/simx-host-hid"
HOSTID_OUT="${SIMX_HOSTID_OUT:-/tmp/simx-c4-result.json}"

# UDID: prefer .simx/dev-sim.txt; fallback first booted (matches
# simx-runner-tap-smoke.sh).
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

# Ensure simx-host-hid binary exists.
if [[ ! -x "$HOSTID_BIN" ]]; then
  echo "simx-host-hid not built; running swift build --product simx-host-hid" >&2
  if ! swift build --product simx-host-hid --package-path "$ROOT/swift-bridge" >&2; then
    echo "swift build simx-host-hid failed" >&2
    exit 1
  fi
fi

# Port must be free.
if lsof -iTCP:"$PORT" -sTCP:LISTEN > /dev/null 2>&1; then
  echo "port $PORT already in use; clean it before retry" >&2
  exit 2
fi

# Start from known cold-Settings state.
xcrun simctl terminate "$SIMX_UDID" com.apple.Preferences > /dev/null 2>&1 || true

LOGDIR="$ROOT/.simx/runner"
mkdir -p "$LOGDIR"
LOGFILE="$LOGDIR/xcodebuild-digitizer-$$.log"
PIDFILE="$LOGDIR/runner-digitizer-$$.pid"

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

# Phase A: poll /health up to 150s (cold xcodebuild + Settings launch).
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

# Phase B: simx-host-hid tap on root list (default path = digitizer).
HOSTID_TAP="fail"
if "$HOSTID_BIN" tap \
       --udid "$SIMX_UDID" \
       --x "$TAP_X" --y "$TAP_Y" \
       > "$HOSTID_OUT" 2> /tmp/simx-c4-hostid-err.log; then
  if jq -e '.ok == true and .path == "digitizer"' "$HOSTID_OUT" > /dev/null; then
    HOSTID_TAP="ok"
  fi
fi
if [[ "$HOSTID_TAP" != "ok" ]]; then
  echo "simx-host-hid tap failed" >&2
  cat "$HOSTID_OUT" >&2 || true
  cat /tmp/simx-c4-hostid-err.log >&2 || true
  tail -40 "$LOGFILE" >&2 || true
  exit 1
fi

PATH_FIELD="$(jq -r '.path' "$HOSTID_OUT")"

# Settle: let Settings push-animation finish (UIKit default ~0.35s + buffer).
sleep 2

# Phase C — probe after MISS: "Display & Brightness" must be gone from
# General sub-page. Expect 404 + .ok=false + .error="not_found".
MISS_BODY="/tmp/simx-c4-probe-miss-$$.json"
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

# Phase D — probe after HIT: "About" must exist on General sub-page.
# Expect 200 + .ok=true.
HIT_BODY="/tmp/simx-c4-probe-hit-$$.json"
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

# Single-line JSON summary on stdout.
printf '{"health":%s,"hostid_tap":"%s","path":"%s","probe_after_miss":%s,"probe_after_hit":%s}\n' \
       "$HEALTH_STATUS" "$HOSTID_TAP" "$PATH_FIELD" "$MISS_STATUS" "$HIT_STATUS"

exit 0
