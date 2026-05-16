#!/usr/bin/env bash
# simx-c2-matcher-failure-smoke.sh — v0.4 C2 e2e smoke.
#
# Run an SDK test whose selector cannot resolve in Settings, prove that:
#   (a) the case exits non-zero (matcher correctly throws)
#   (b) the trace sink wrote .simx/trace/c2-matcher-failure/failure-0001.png
#   (c) the file is a legitimate PNG (>= 1000 bytes + PNG magic 89 50 4E 47 ...)
#
# Single-line JSON summary to stdout; exit 0 iff all gates pass.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="$ROOT/swift-bridge/SimxRunner.xcodeproj"
SCHEME="SimxRunner"
PORT="${SIMX_RUNNER_PORT:-22087}"
CASE_SLUG="c2-matcher-failure"
TRACE_DIR="$ROOT/.simx/trace/$CASE_SLUG"
PNG_PATH="$TRACE_DIR/failure-0001.png"

# Phase 0: dev sim UDID + state check (mirrors v0.3 acceptance pattern).
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

# Port must be free; we run our own runner instance here.
if lsof -iTCP:"$PORT" -sTCP:LISTEN > /dev/null 2>&1; then
  echo "port $PORT already in use; clean it before retry" >&2
  exit 2
fi

# Start from a known cold-Settings state so app.launch() does observable work.
xcrun simctl terminate "$SIMX_UDID" com.apple.Preferences > /dev/null 2>&1 || true

# Wipe any prior failure PNG so the existence check is meaningful.
rm -rf "$TRACE_DIR"

LOGDIR="$ROOT/.simx/runner"
mkdir -p "$LOGDIR"
LOGFILE="$LOGDIR/xcodebuild-c2-$$.log"
PIDFILE="$LOGDIR/runner-c2-$$.pid"
SIMX_OUT="/tmp/simx-c2-cli-$$.out"
SIMX_ERR="/tmp/simx-c2-cli-$$.err"

# Kick xcodebuild test backgrounded so the runner serves /tap, /tree, /health.
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

# Run the failing SDK test through the simx CLI. Expect exit 1 (case failed).
EXIT_CODE=0
SIMX_UDID="$SIMX_UDID" bun "$ROOT/src/cli/index.ts" run \
  "$ROOT/examples/_v04-tests/c2-matcher-failure.test.ts" \
  --udid "$SIMX_UDID" \
  > "$SIMX_OUT" 2> "$SIMX_ERR" || EXIT_CODE=$?

PNG_OK="missing"
PNG_SIZE=0
PNG_MAGIC="bad"
if [[ -f "$PNG_PATH" ]]; then
  PNG_OK="ok"
  PNG_SIZE="$(stat -f %z "$PNG_PATH" 2>/dev/null || stat -c %s "$PNG_PATH")"
  HEAD_HEX="$(xxd -l 8 -p "$PNG_PATH" 2>/dev/null | tr -d '\n')"
  if [[ "$HEAD_HEX" == "89504e470d0a1a0a" ]]; then
    PNG_MAGIC="ok"
  fi
fi

if [[ "$PNG_OK" != "ok" || "$PNG_MAGIC" != "ok" || "$EXIT_CODE" -eq 0 ]]; then
  {
    echo "--- c2 smoke gate failure ---"
    echo "case_exit=$EXIT_CODE (want 1)"
    echo "matcher_failure_png=$PNG_OK (want ok)"
    echo "png_size=$PNG_SIZE"
    echo "png_magic=$PNG_MAGIC (want ok)"
    echo "--- simx stdout ---"
    cat "$SIMX_OUT" 2>/dev/null || true
    echo "--- simx stderr ---"
    cat "$SIMX_ERR" 2>/dev/null || true
    echo "--- xcodebuild tail ---"
    tail -40 "$LOGFILE" 2>/dev/null || true
  } >&2
fi

printf '{"case_exit":%d,"matcher_failure_png":"%s","png_size":%s,"png_magic":"%s"}\n' \
  "$EXIT_CODE" "$PNG_OK" "$PNG_SIZE" "$PNG_MAGIC"

if [[ "$EXIT_CODE" -eq 1 && "$PNG_OK" == "ok" && "$PNG_MAGIC" == "ok" && "$PNG_SIZE" -ge 1000 ]]; then
  exit 0
fi
exit 1
