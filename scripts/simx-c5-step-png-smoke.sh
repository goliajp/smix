#!/usr/bin/env bash
# simx-c5-step-png-smoke.sh — v0.4 C5 e2e smoke.
#
# Run an SDK test driving launch + tap on a dev sim. Prove that:
#   (a) the case exits zero (success path)
#   (b) .simx/trace/c5-step-png/steps.jsonl exists with >= 2 lines
#   (c) >= 2 step-<NNNN>.png files exist under that dir
#   (d) step-0001.png + step-0002.png each have the PNG magic prefix
#   (e) jsonl rows 1+2 carry seq=1 and seq=2 (1:1 alignment with file names)
#
# Single-line JSON summary (6 fields) to stdout; exit 0 iff all gates pass.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="$ROOT/swift-bridge/SimxRunner.xcodeproj"
SCHEME="SimxRunner"
PORT="${SIMX_RUNNER_PORT:-22087}"
CASE_SLUG="c5-step-png"
TRACE_DIR="$ROOT/.simx/trace/$CASE_SLUG"
JSONL_PATH="$TRACE_DIR/steps.jsonl"
STEP1_PATH="$TRACE_DIR/step-0001.png"
STEP2_PATH="$TRACE_DIR/step-0002.png"

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
LOGFILE="$LOGDIR/xcodebuild-c5-$$.log"
PIDFILE="$LOGDIR/runner-c5-$$.pid"
SIMX_OUT="/tmp/simx-c5-cli-$$.out"
SIMX_ERR="/tmp/simx-c5-cli-$$.err"

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
  "$ROOT/examples/_v04-tests/c5-step-png.test.ts" \
  --udid "$SIMX_UDID" \
  > "$SIMX_OUT" 2> "$SIMX_ERR" || EXIT_CODE=$?

LINE_COUNT=0
PNG_COUNT=0
STEP1_MAGIC="missing"
STEP2_MAGIC="missing"
SEQ_MATCH="false"
if [[ -f "$JSONL_PATH" ]]; then
  LINE_COUNT="$(grep -c . "$JSONL_PATH" 2>/dev/null || echo 0)"
fi
if [[ -d "$TRACE_DIR" ]]; then
  PNG_COUNT="$(find "$TRACE_DIR" -maxdepth 1 -name 'step-*.png' 2>/dev/null | wc -l | tr -d ' ')"
fi
if [[ -f "$STEP1_PATH" ]]; then
  MAGIC1="$(xxd -p -l 4 "$STEP1_PATH" 2>/dev/null || echo "")"
  if [[ "$MAGIC1" == "89504e47" ]]; then STEP1_MAGIC="ok"; else STEP1_MAGIC="$MAGIC1"; fi
fi
if [[ -f "$STEP2_PATH" ]]; then
  MAGIC2="$(xxd -p -l 4 "$STEP2_PATH" 2>/dev/null || echo "")"
  if [[ "$MAGIC2" == "89504e47" ]]; then STEP2_MAGIC="ok"; else STEP2_MAGIC="$MAGIC2"; fi
fi
if [[ -f "$JSONL_PATH" ]]; then
  SEQ1="$(sed -n '1p' "$JSONL_PATH" | jq -r '.seq // empty' 2>/dev/null || echo "")"
  SEQ2="$(sed -n '2p' "$JSONL_PATH" | jq -r '.seq // empty' 2>/dev/null || echo "")"
  if [[ "$SEQ1" == "1" && "$SEQ2" == "2" ]]; then SEQ_MATCH="ok"; fi
fi

if [[ "$EXIT_CODE" -ne 0 || "$LINE_COUNT" -lt 2 || "$PNG_COUNT" -lt 2 || "$STEP1_MAGIC" != "ok" || "$STEP2_MAGIC" != "ok" || "$SEQ_MATCH" != "ok" ]]; then
  {
    echo "--- c5 smoke gate failure ---"
    echo "case_exit=$EXIT_CODE (want 0)"
    echo "line_count=$LINE_COUNT (want >=2)"
    echo "png_count=$PNG_COUNT (want >=2)"
    echo "step1_magic=$STEP1_MAGIC (want ok)"
    echo "step2_magic=$STEP2_MAGIC (want ok)"
    echo "seq_match=$SEQ_MATCH (want ok)"
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

printf '{"case_exit":%d,"line_count":%d,"png_count":%d,"step1_magic":"%s","step2_magic":"%s","seq_match":"%s"}\n' \
  "$EXIT_CODE" "$LINE_COUNT" "$PNG_COUNT" "$STEP1_MAGIC" "$STEP2_MAGIC" "$SEQ_MATCH"

if [[ "$EXIT_CODE" -eq 0 && "$LINE_COUNT" -ge 2 && "$PNG_COUNT" -ge 2 && "$STEP1_MAGIC" == "ok" && "$STEP2_MAGIC" == "ok" && "$SEQ_MATCH" == "ok" ]]; then
  exit 0
fi
exit 1
