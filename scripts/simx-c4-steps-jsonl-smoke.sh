#!/usr/bin/env bash
# simx-c4-steps-jsonl-smoke.sh — v0.4 C4 e2e smoke.
#
# Run an SDK test driving launch + tap on a dev sim. Prove that:
#   (a) the case exits zero (success path)
#   (b) .simx/trace/c4-steps-jsonl/steps.jsonl exists with >= 2 lines
#   (c) first line type == "launch"
#   (d) every line has ok == true
#   (e) every line satisfies the 6-field schema (ts/seq/type/args/ok/duration_ms)
#
# Single-line JSON summary to stdout; exit 0 iff all gates pass.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="$ROOT/swift-bridge/SimxRunner.xcodeproj"
SCHEME="SimxRunner"
PORT="${SIMX_RUNNER_PORT:-22087}"
CASE_SLUG="c4-steps-jsonl"
TRACE_DIR="$ROOT/.simx/trace/$CASE_SLUG"
JSONL_PATH="$TRACE_DIR/steps.jsonl"

# Phase 0: dev sim UDID + state check.
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

if lsof -iTCP:"$PORT" -sTCP:LISTEN > /dev/null 2>&1; then
  echo "port $PORT already in use; clean it before retry" >&2
  exit 2
fi

# Cold-restart Settings so app.launch() does observable work.
xcrun simctl terminate "$SIMX_UDID" com.apple.Preferences > /dev/null 2>&1 || true

rm -rf "$TRACE_DIR"

LOGDIR="$ROOT/.simx/runner"
mkdir -p "$LOGDIR"
LOGFILE="$LOGDIR/xcodebuild-c4-$$.log"
PIDFILE="$LOGDIR/runner-c4-$$.pid"
SIMX_OUT="/tmp/simx-c4-cli-$$.out"
SIMX_ERR="/tmp/simx-c4-cli-$$.err"

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
  rm -f "$SIMX_OUT" "$SIMX_ERR" 2>/dev/null || true
}
trap cleanup EXIT

# Poll /health up to 150s (cold xcodebuild + Settings auto-launch).
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

EXIT_CODE=0
SIMX_UDID="$SIMX_UDID" bun "$ROOT/src/cli/index.ts" run \
  "$ROOT/examples/_v04-tests/c4-steps-jsonl.test.ts" \
  --udid "$SIMX_UDID" \
  > "$SIMX_OUT" 2> "$SIMX_ERR" || EXIT_CODE=$?

LINE_COUNT=0
FIRST_TYPE="missing"
LAST_TYPE="missing"
ALL_OK="false"
SCHEMA_OK="false"
if [[ -f "$JSONL_PATH" ]]; then
  LINE_COUNT="$(grep -c . "$JSONL_PATH" 2>/dev/null || echo 0)"
  FIRST_TYPE="$(head -1 "$JSONL_PATH" | jq -r '.type // "missing"' 2>/dev/null || echo missing)"
  LAST_TYPE="$(tail -1 "$JSONL_PATH" | jq -r '.type // "missing"' 2>/dev/null || echo missing)"
  ALL_OK="$(jq -s 'all(.ok == true)' "$JSONL_PATH" 2>/dev/null || echo false)"
  SCHEMA_OK="$(jq -s 'all((.ts|type=="number") and (.seq|type=="number") and (.type|type=="string") and (.args|type=="object") and (.ok|type=="boolean") and (.duration_ms|type=="number"))' "$JSONL_PATH" 2>/dev/null || echo false)"
fi

if [[ "$EXIT_CODE" -ne 0 || "$LINE_COUNT" -lt 2 || "$FIRST_TYPE" != "launch" || "$ALL_OK" != "true" || "$SCHEMA_OK" != "true" ]]; then
  {
    echo "--- c4 smoke gate failure ---"
    echo "case_exit=$EXIT_CODE (want 0)"
    echo "line_count=$LINE_COUNT (want >=2)"
    echo "first_type=$FIRST_TYPE (want launch)"
    echo "last_type=$LAST_TYPE"
    echo "all_ok=$ALL_OK (want true)"
    echo "schema_ok=$SCHEMA_OK (want true)"
    echo "--- jsonl ---"
    cat "$JSONL_PATH" 2>/dev/null || echo "(missing)"
    echo "--- simx stdout ---"
    cat "$SIMX_OUT" 2>/dev/null || true
    echo "--- simx stderr ---"
    cat "$SIMX_ERR" 2>/dev/null || true
    echo "--- xcodebuild tail ---"
    tail -40 "$LOGFILE" 2>/dev/null || true
  } >&2
fi

printf '{"case_exit":%d,"line_count":%d,"first_type":"%s","last_type":"%s","all_ok":%s,"schema_ok":%s}\n' \
  "$EXIT_CODE" "$LINE_COUNT" "$FIRST_TYPE" "$LAST_TYPE" "$ALL_OK" "$SCHEMA_OK"

if [[ "$EXIT_CODE" -eq 0 && "$LINE_COUNT" -ge 2 && "$FIRST_TYPE" == "launch" && "$ALL_OK" == "true" && "$SCHEMA_OK" == "true" ]]; then
  exit 0
fi
exit 1
