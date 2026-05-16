#!/usr/bin/env bash
# simx-c8-dogfood-smoke.sh — v0.4 C8 dogfood smoke (v0.4 收官 e2e).
#
# Runs the 3 dogfood fixtures (typo / case / partial), confirms each writes
# .simx/trace/<slug>/failure-0001.json, then feeds each JSON to a real
# `claude -p ... --output-format text` subprocess and asserts the stdout
# proposes 'General' via case-insensitive regex /general/i. 3/3 strict
# (no retry). Single-line 9-field JSON to stdout; exit 0 iff all gates pass.
#
# Implementation note — plan-hot decision 8 fallback path (3× independent
# xcodebuild test sessions):
#   - Originally decision 8 favoured a single runner session for ~60-90s
#     savings, but the SimxRunner UITest binds XCUIApplication exactly once
#     with English launchArguments. Mid-session `simctl terminate Preferences`
#     unbinds the XCUI launch state; the SDK's `app.launch()` then comes back
#     via plain `simctl launch` which lacks the launchArguments → cell-list
#     locale + Apple Account banner geometry both drift, and the runner's
#     POST /tap predicate `label==% OR identifier==%` cannot find "General".
#   - Fix: 3 cold xcodebuild sessions (mirror c2/c3/c6 smoke shape exactly).
#     Time cost ~3× ≈ 90s extra but reliability beats decision 8 savings.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="$ROOT/swift-bridge/SimxRunner.xcodeproj"
SCHEME="SimxRunner"
PORT="${SIMX_RUNNER_PORT:-22087}"

SLUGS=(c8-dogfood-typo c8-dogfood-case c8-dogfood-partial)
LABELS=(a b c)
FIXTURES=(
  "$ROOT/examples/_v04-tests/c8-dogfood-typo.test.ts"
  "$ROOT/examples/_v04-tests/c8-dogfood-case.test.ts"
  "$ROOT/examples/_v04-tests/c8-dogfood-partial.test.ts"
)

# Phase 0: dev sim UDID + state + port + claude binary.
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

if ! command -v claude > /dev/null 2>&1; then
  echo "claude CLI not on PATH (C8 cannot run without it)" >&2
  exit 2
fi

# Wipe any prior trace dirs so existence checks are meaningful.
for slug in "${SLUGS[@]}"; do
  rm -rf "$ROOT/.simx/trace/$slug"
done

LOGDIR="$ROOT/.simx/runner"
mkdir -p "$LOGDIR"
TMP_STDOUT="/tmp/simx-c8-cli-$$"
mkdir -p "$TMP_STDOUT"

CASE_EXIT=(0 0 0)
JSON_PRESENT=(fail fail fail)
# Naming follows plan-hot S1 phase 6 envelope spec: json_present uses ok/fail
# while claude_general uses yes/no — keep both literal forms so downstream jq
# gates in the task description ("claude_general == 'yes'") pass verbatim.
CLAUDE_GENERAL=(no no no)
CLAUDE_TAIL=("" "" "")

# Cleanup state (one runner at a time; PID rewritten per case).
PIDFILE="$LOGDIR/runner-c8-$$.pid"

cleanup() {
  local pid; pid="$(cat "$PIDFILE" 2>/dev/null || true)"
  if [[ -n "$pid" ]]; then
    pkill -P "$pid" 2>/dev/null || true
    kill -TERM "$pid" 2>/dev/null || true
  fi
  xcrun simctl terminate "$SIMX_UDID" com.apple.Preferences > /dev/null 2>&1 || true
  rm -rf "$TMP_STDOUT" 2>/dev/null || true
  rm -f "$PIDFILE" 2>/dev/null || true
}
trap cleanup EXIT

# Phase 1-3 per case: cold xcodebuild → /health → run fixture.
for i in 0 1 2; do
  SLUG="${SLUGS[$i]}"
  LABEL="${LABELS[$i]}"
  FIXTURE="${FIXTURES[$i]}"
  STDOUT="$TMP_STDOUT/simx-$LABEL.out"
  STDERR="$TMP_STDOUT/simx-$LABEL.err"
  LOGFILE="$LOGDIR/xcodebuild-c8-$$-$LABEL.log"

  if lsof -iTCP:"$PORT" -sTCP:LISTEN > /dev/null 2>&1; then
    echo "port $PORT in use before case $LABEL; aborting" >&2
    exit 2
  fi

  xcrun simctl terminate "$SIMX_UDID" com.apple.Preferences > /dev/null 2>&1 || true
  sleep 1

  ( xcodebuild -project "$PROJECT" \
               -scheme "$SCHEME" \
               -destination "platform=iOS Simulator,id=$SIMX_UDID" \
               -only-testing:SimxRunnerUITests/SimxRunnerUITests/test_runForever \
               test > "$LOGFILE" 2>&1 ) &
  echo $! > "$PIDFILE"

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
    echo "case $LABEL: health did not reach 200 within 150s" >&2
    tail -40 "$LOGFILE" >&2 || true
    # tear down this xcodebuild before moving on
    case_pid="$(cat "$PIDFILE" 2>/dev/null || true)"
    if [[ -n "$case_pid" ]]; then
      pkill -P "$case_pid" 2>/dev/null || true
      kill -TERM "$case_pid" 2>/dev/null || true
    fi
    continue
  fi

  EXIT_CODE=0
  SIMX_UDID="$SIMX_UDID" bun "$ROOT/src/cli/index.ts" run \
    "$FIXTURE" \
    --udid "$SIMX_UDID" \
    > "$STDOUT" 2> "$STDERR" || EXIT_CODE=$?
  CASE_EXIT[$i]="$EXIT_CODE"

  JSON_PATH="$ROOT/.simx/trace/$SLUG/failure-0001.json"
  if [[ -f "$JSON_PATH" ]]; then
    JSON_PRESENT[$i]="ok"
  else
    JSON_PRESENT[$i]="fail"
  fi

  # Tear down this xcodebuild session before next case.
  pid="$(cat "$PIDFILE" 2>/dev/null || true)"
  if [[ -n "$pid" ]]; then
    pkill -P "$pid" 2>/dev/null || true
    kill -TERM "$pid" 2>/dev/null || true
  fi
  rm -f "$PIDFILE" 2>/dev/null || true

  # Wait briefly for port to free before next iteration.
  for _ in $(seq 1 20); do
    if ! lsof -iTCP:"$PORT" -sTCP:LISTEN > /dev/null 2>&1; then
      break
    fi
    sleep 1
  done
done

# Phase 4: feed each failure.json to claude -p sequentially. Done AFTER all
# fixtures so runner sessions are torn down before potentially-slow LLM calls.
# Decision 4: strict 3/3, no retry on claude flake.
for i in 0 1 2; do
  SLUG="${SLUGS[$i]}"
  LABEL="${LABELS[$i]}"
  JSON_PATH="$ROOT/.simx/trace/$SLUG/failure-0001.json"

  if [[ ! -f "$JSON_PATH" ]]; then
    continue
  fi

  CLAUDE_OUT_FILE="$TMP_STDOUT/claude-$LABEL.out"
  # Plan-hot decision 2 + decision 6: literal prompt, text output, no
  # engineered prefix, no --image (PNG path is v0.6 MCP `explain_screen`).
  # `--tools ""` disables claude's filesystem tools so the agentic CLI does
  # NOT auto-edit our dogfood fixtures (observed during first smoke run:
  # `claude -p` with default tools edited examples/_v04-tests/c8-dogfood-*.ts
  # to "fix" the typo'd selectors in-place, defeating the dogfood loop).
  # Pure text completion is the contract C8 needs.
  PROMPT="Fix the selector based on this failure: $(cat "$JSON_PATH")"
  if claude -p "$PROMPT" --output-format text --tools "" > "$CLAUDE_OUT_FILE" 2>&1; then
    :
  fi

  if grep -qiE 'general' "$CLAUDE_OUT_FILE" 2>/dev/null; then
    CLAUDE_GENERAL[$i]="yes"
  fi
  CLAUDE_TAIL[$i]="$(tr '\n' ' ' < "$CLAUDE_OUT_FILE" | tail -c 200 || true)"
done

# Phase 5: emit 9-field JSON + diagnostic on failure.
PASS=1
for i in 0 1 2; do
  [[ "${CASE_EXIT[$i]}" == "1" ]] || PASS=0
  [[ "${JSON_PRESENT[$i]}" == "ok" ]] || PASS=0
  [[ "${CLAUDE_GENERAL[$i]}" == "yes" ]] || PASS=0
done

if [[ "$PASS" != "1" ]]; then
  {
    echo "--- c8 dogfood smoke gate failure ---"
    for i in 0 1 2; do
      L="${LABELS[$i]}"
      echo "case_exit_$L=${CASE_EXIT[$i]} (want 1)"
      echo "json_${L}_present=${JSON_PRESENT[$i]} (want ok)"
      echo "claude_${L}_general=${CLAUDE_GENERAL[$i]} (want yes); tail: ${CLAUDE_TAIL[$i]}"
    done
    echo "--- simx stdout/err per case ---"
    for L in a b c; do
      echo "[$L stdout]"; cat "$TMP_STDOUT/simx-$L.out" 2>/dev/null || true
      echo "[$L stderr]"; cat "$TMP_STDOUT/simx-$L.err" 2>/dev/null || true
    done
    echo "--- xcodebuild tails ---"
    for L in a b c; do
      echo "[$L xcodebuild tail]"; tail -20 "$LOGDIR/xcodebuild-c8-$$-$L.log" 2>/dev/null || true
    done
  } >&2
fi

printf '{"typo_case_exit":%d,"typo_json_present":"%s","typo_claude_general":"%s","case_case_exit":%d,"case_json_present":"%s","case_claude_general":"%s","partial_case_exit":%d,"partial_json_present":"%s","partial_claude_general":"%s"}\n' \
  "${CASE_EXIT[0]}" "${JSON_PRESENT[0]}" "${CLAUDE_GENERAL[0]}" \
  "${CASE_EXIT[1]}" "${JSON_PRESENT[1]}" "${CLAUDE_GENERAL[1]}" \
  "${CASE_EXIT[2]}" "${JSON_PRESENT[2]}" "${CLAUDE_GENERAL[2]}"

if [[ "$PASS" == "1" ]]; then
  exit 0
fi
exit 1
