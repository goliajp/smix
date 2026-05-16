#!/usr/bin/env bash
# simx-c3-suggestions-smoke.sh — v0.4 C3 e2e smoke.
#
# Run an SDK test whose selector is a single-char typo of a real Settings cell
# ('Genral' → 'General'). Prove that:
#   (a) the case exits non-zero (matcher correctly throws)
#   (b) ExpectationFailure.suggestions is non-empty
#   (c) the top suggestion references 'General'
#   (d) the trace sink wrote .simx/trace/c3-suggestions/failure-0001.png
#   (e) the PNG has the canonical 89 50 4E 47 0D 0A 1A 0A magic header
#
# Single-line JSON summary to stdout; exit 0 iff all gates pass.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="$ROOT/swift-bridge/SimxRunner.xcodeproj"
SCHEME="SimxRunner"
PORT="${SIMX_RUNNER_PORT:-22087}"
CASE_SLUG="c3-suggestions"
TRACE_DIR="$ROOT/.simx/trace/$CASE_SLUG"
PNG_PATH="$TRACE_DIR/failure-0001.png"

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

# Cold-restart Settings so describe() sees a real General cell, not the
# sparse-tree XCUIApplication root that follows back-button navigation.
xcrun simctl terminate "$SIMX_UDID" com.apple.Preferences > /dev/null 2>&1 || true

rm -rf "$TRACE_DIR"

LOGDIR="$ROOT/.simx/runner"
mkdir -p "$LOGDIR"
LOGFILE="$LOGDIR/xcodebuild-c3-$$.log"
PIDFILE="$LOGDIR/runner-c3-$$.pid"
SIMX_OUT="/tmp/simx-c3-cli-$$.out"
SIMX_ERR="/tmp/simx-c3-cli-$$.err"

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
  "$ROOT/examples/_v04-tests/c3-suggestions.test.ts" \
  --udid "$SIMX_UDID" \
  > "$SIMX_OUT" 2> "$SIMX_ERR" || EXIT_CODE=$?

JSON_LINE="$(grep '__C3_SUGGESTIONS_JSON__' "$SIMX_OUT" 2>/dev/null | tail -1 | sed 's/^.*__C3_SUGGESTIONS_JSON__ //')"
SUG_COUNT=0
TOP_HAS_GENERAL="no"
if [[ -n "$JSON_LINE" ]]; then
  SUG_COUNT="$(printf '%s' "$JSON_LINE" | jq -r '.suggestions | length // 0' 2>/dev/null || echo 0)"
  TOP_HAS_GENERAL="$(printf '%s' "$JSON_LINE" | jq -r 'if (.suggestions[0] // "") | test("General") then "yes" else "no" end' 2>/dev/null || echo "no")"
fi

PNG_OK="missing"
PNG_MAGIC="bad"
if [[ -f "$PNG_PATH" ]]; then
  PNG_OK="ok"
  HEAD_HEX="$(xxd -l 8 -p "$PNG_PATH" 2>/dev/null | tr -d '\n')"
  if [[ "$HEAD_HEX" == "89504e470d0a1a0a" ]]; then
    PNG_MAGIC="ok"
  fi
fi

if [[ "$EXIT_CODE" -ne 1 || "$SUG_COUNT" -lt 1 || "$TOP_HAS_GENERAL" != "yes" || "$PNG_OK" != "ok" || "$PNG_MAGIC" != "ok" ]]; then
  {
    echo "--- c3 smoke gate failure ---"
    echo "case_exit=$EXIT_CODE (want 1)"
    echo "suggestions_count=$SUG_COUNT (want >=1)"
    echo "top_suggestion_has_general=$TOP_HAS_GENERAL (want yes)"
    echo "png_present=$PNG_OK (want ok)"
    echo "png_magic=$PNG_MAGIC (want ok)"
    echo "--- json envelope ---"
    echo "$JSON_LINE"
    echo "--- simx stdout ---"
    cat "$SIMX_OUT" 2>/dev/null || true
    echo "--- simx stderr ---"
    cat "$SIMX_ERR" 2>/dev/null || true
    echo "--- xcodebuild tail ---"
    tail -40 "$LOGFILE" 2>/dev/null || true
  } >&2
fi

printf '{"case_exit":%d,"suggestions_count":%d,"top_suggestion_has_general":"%s","png_present":"%s","png_magic":"%s"}\n' \
  "$EXIT_CODE" "$SUG_COUNT" "$TOP_HAS_GENERAL" "$PNG_OK" "$PNG_MAGIC"

if [[ "$EXIT_CODE" -eq 1 && "$SUG_COUNT" -ge 1 && "$TOP_HAS_GENERAL" == "yes" && "$PNG_OK" == "ok" && "$PNG_MAGIC" == "ok" ]]; then
  exit 0
fi
exit 1
