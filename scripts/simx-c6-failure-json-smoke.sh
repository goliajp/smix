#!/usr/bin/env bash
# simx-c6-failure-json-smoke.sh — v0.4 C6 e2e smoke.
#
# Run an SDK test whose selector cannot resolve in Settings, prove that:
#   (a) the case exits non-zero (matcher correctly throws)
#   (b) .simx/trace/c6-failure-json/failure-0001.png landed (regression of C2)
#   (c) .simx/trace/c6-failure-json/failure-0001.json landed (C6 new)
#   (d) JSON seq NNNN matches PNG seq NNNN (1:1 alignment within case)
#   (e) JSON has the failure code field populated
#   (f) JSON has screenshot_path referencing the sibling PNG file
#   (g) JSON does NOT embed a base64 screenshot field (decision 2)
#   (h) JSON shape matches FailurePayload schema (ok=false + 5 required fields)
#
# Single-line JSON summary to stdout; exit 0 iff all gates pass.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="$ROOT/swift-bridge/SimxRunner.xcodeproj"
SCHEME="SimxRunner"
PORT="${SIMX_RUNNER_PORT:-22087}"
CASE_SLUG="c6-failure-json"
TRACE_DIR="$ROOT/.simx/trace/$CASE_SLUG"
PNG_PATH="$TRACE_DIR/failure-0001.png"
JSON_PATH="$TRACE_DIR/failure-0001.json"

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

if lsof -iTCP:"$PORT" -sTCP:LISTEN > /dev/null 2>&1; then
  echo "port $PORT already in use; clean it before retry" >&2
  exit 2
fi

xcrun simctl terminate "$SIMX_UDID" com.apple.Preferences > /dev/null 2>&1 || true

# Wipe any prior failure artefacts so existence checks are meaningful.
rm -rf "$TRACE_DIR"

LOGDIR="$ROOT/.simx/runner"
mkdir -p "$LOGDIR"
LOGFILE="$LOGDIR/xcodebuild-c6-$$.log"
PIDFILE="$LOGDIR/runner-c6-$$.pid"
SIMX_OUT="/tmp/simx-c6-cli-$$.out"
SIMX_ERR="/tmp/simx-c6-cli-$$.err"

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
  "$ROOT/examples/_v04-tests/c6-failure-json.test.ts" \
  --udid "$SIMX_UDID" \
  > "$SIMX_OUT" 2> "$SIMX_ERR" || EXIT_CODE=$?

# --- gate evaluation ---
PNG_PRESENT="missing"
PNG_SIZE=0
PNG_MAGIC="bad"
if [[ -f "$PNG_PATH" ]]; then
  PNG_PRESENT="ok"
  PNG_SIZE="$(stat -f %z "$PNG_PATH" 2>/dev/null || stat -c %s "$PNG_PATH")"
  HEAD_HEX="$(xxd -l 8 -p "$PNG_PATH" 2>/dev/null | tr -d '\n')"
  if [[ "$HEAD_HEX" == "89504e470d0a1a0a" ]]; then
    PNG_MAGIC="ok"
  fi
fi

JSON_PRESENT="missing"
JSON_HAS_CODE="no"
JSON_HAS_SCREENSHOT_PATH="no"
JSON_NO_BASE64="yes"
JSON_SEQ_MATCHES_PNG="no"
JSON_SCHEMA_OK="no"
if [[ -f "$JSON_PATH" ]]; then
  JSON_PRESENT="ok"
  if jq -e '.code | type == "string" and length > 0' "$JSON_PATH" > /dev/null 2>&1; then
    JSON_HAS_CODE="yes"
  fi
  if jq -e '.screenshot_path == "failure-0001.png"' "$JSON_PATH" > /dev/null 2>&1; then
    JSON_HAS_SCREENSHOT_PATH="yes"
    # Same NNNN suffix on JSON file name + JSON screenshot_path field +
    # actual PNG file existence ⇒ seq alignment.
    if [[ -f "$PNG_PATH" ]]; then
      JSON_SEQ_MATCHES_PNG="ok"
    fi
  fi
  if jq -e '.screenshot != null' "$JSON_PATH" > /dev/null 2>&1; then
    JSON_NO_BASE64="no"
  fi
  if jq -e '
    .ok == false and
    (.code | type == "string") and
    (.message | type == "string") and
    (.suggestions | type == "array") and
    (.visibleElements | type == "array")
  ' "$JSON_PATH" > /dev/null 2>&1; then
    JSON_SCHEMA_OK="ok"
  fi
fi

# Map to plan-hot 7-field names too (failure_png_ok / failure_json_ok).
FAILURE_PNG_OK="$PNG_PRESENT"
FAILURE_JSON_OK="$JSON_PRESENT"
# Normalize to the task gate's "ok" string for the boolean-ish fields.
if [[ "$JSON_NO_BASE64" == "yes" ]]; then JSON_NO_BASE64_TXT="ok"; else JSON_NO_BASE64_TXT="no"; fi
if [[ "$JSON_HAS_SCREENSHOT_PATH" == "yes" ]]; then JSON_HAS_SCREENSHOT_PATH_TXT="ok"; else JSON_HAS_SCREENSHOT_PATH_TXT="no"; fi

if [[ "$EXIT_CODE" -ne 1 || "$PNG_PRESENT" != "ok" || "$PNG_MAGIC" != "ok" \
   || "$JSON_PRESENT" != "ok" || "$JSON_HAS_CODE" != "yes" \
   || "$JSON_HAS_SCREENSHOT_PATH" != "yes" || "$JSON_NO_BASE64" != "yes" \
   || "$JSON_SEQ_MATCHES_PNG" != "ok" || "$JSON_SCHEMA_OK" != "ok" ]]; then
  {
    echo "--- c6 smoke gate failure ---"
    echo "case_exit=$EXIT_CODE (want 1)"
    echo "png_present=$PNG_PRESENT (want ok)"
    echo "png_size=$PNG_SIZE"
    echo "png_magic=$PNG_MAGIC (want ok)"
    echo "json_present=$JSON_PRESENT (want ok)"
    echo "json_has_code=$JSON_HAS_CODE (want yes)"
    echo "json_has_screenshot_path=$JSON_HAS_SCREENSHOT_PATH (want yes)"
    echo "json_no_base64=$JSON_NO_BASE64 (want yes)"
    echo "json_seq_matches_png=$JSON_SEQ_MATCHES_PNG (want ok)"
    echo "json_schema_ok=$JSON_SCHEMA_OK (want ok)"
    echo "--- failure-0001.json ---"
    cat "$JSON_PATH" 2>/dev/null || echo "(missing)"
    echo "--- simx stdout ---"
    cat "$SIMX_OUT" 2>/dev/null || true
    echo "--- simx stderr ---"
    cat "$SIMX_ERR" 2>/dev/null || true
    echo "--- xcodebuild tail ---"
    tail -40 "$LOGFILE" 2>/dev/null || true
  } >&2
fi

# Emit superset JSON covering both plan-hot field names (failure_png_ok/
# failure_json_ok/json_has_code) and the task-gate names (png_present/
# json_present/json_seq_matches_png/json_schema_ok). All consumers stay
# satisfied by a single line.
printf '{"case_exit":%d,"failure_png_ok":"%s","png_present":"%s","png_size":%s,"png_magic":"%s","failure_json_ok":"%s","json_present":"%s","json_has_code":"%s","json_has_screenshot_path":"%s","json_no_base64":"%s","json_seq_matches_png":"%s","json_schema_ok":"%s"}\n' \
  "$EXIT_CODE" "$FAILURE_PNG_OK" "$PNG_PRESENT" "$PNG_SIZE" "$PNG_MAGIC" \
  "$FAILURE_JSON_OK" "$JSON_PRESENT" "$JSON_HAS_CODE" "$JSON_HAS_SCREENSHOT_PATH_TXT" \
  "$JSON_NO_BASE64_TXT" "$JSON_SEQ_MATCHES_PNG" "$JSON_SCHEMA_OK"

if [[ "$EXIT_CODE" -eq 1 && "$PNG_PRESENT" == "ok" && "$PNG_MAGIC" == "ok" \
   && "$JSON_PRESENT" == "ok" && "$JSON_HAS_CODE" == "yes" \
   && "$JSON_HAS_SCREENSHOT_PATH" == "yes" && "$JSON_NO_BASE64" == "yes" \
   && "$JSON_SEQ_MATCHES_PNG" == "ok" && "$JSON_SCHEMA_OK" == "ok" \
   && "$PNG_SIZE" -ge 1000 ]]; then
  exit 0
fi
exit 1
