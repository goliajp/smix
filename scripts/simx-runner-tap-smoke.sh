#!/usr/bin/env bash
# simx-runner-tap-smoke.sh — boot SimxRunner, hit POST /tap on Settings "General",
# verify both hit and miss paths.
# WHY: v0.2 C2 verification harness; replaces hand-typing the long curl line.
# Target app (com.apple.Preferences) is hard-coded inside SimxRunnerUITests.swift;
# SIMX_TAP_BUNDLE_ID below is documentation only.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="$ROOT/swift-bridge/SimxRunner.xcodeproj"
SCHEME="SimxRunner"
PORT="${SIMX_RUNNER_PORT:-22087}"
# shellcheck disable=SC2034
SIMX_TAP_BUNDLE_ID="${SIMX_TAP_BUNDLE_ID:-com.apple.Preferences}"  # documentation only
TAP_TEXT="${SIMX_TAP_TEXT:-General}"
MISS_TEXT="${SIMX_MISS_TEXT:-ZZZZZ_no_such_button_ZZZZZ}"

if [[ -z "${SIMX_UDID:-}" ]]; then
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

if lsof -iTCP:"$PORT" -sTCP:LISTEN > /dev/null 2>&1; then
  echo "port $PORT already in use; clean it before retry" >&2
  exit 2
fi

LOGDIR="$ROOT/.simx/runner"
mkdir -p "$LOGDIR"
LOGFILE="$LOGDIR/xcodebuild-tap-$$.log"
PIDFILE="$LOGDIR/runner-tap-$$.pid"

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

# Phase A: poll /health up to 150s (cold build + Settings launch).
HEALTH_STATUS=""
for i in $(seq 1 150); do
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

# Phase B: POST /tap with matching text — expect 200 + .ok=true + .matched.label
HIT_BODY="/tmp/simx-tap-hit-$$.json"
HIT_STATUS="$(curl -sS -o "$HIT_BODY" -w '%{http_code}' \
                  --max-time 10 \
                  -X POST -H 'Content-Type: application/json' \
                  -d "{\"selector\":{\"text\":\"$TAP_TEXT\"}}" \
                  "http://127.0.0.1:$PORT/tap")"
if [[ "$HIT_STATUS" != "200" ]]; then
  echo "tap hit expected 200, got $HIT_STATUS" >&2
  cat "$HIT_BODY" >&2 || true
  exit 1
fi
if ! jq -e '.ok == true' "$HIT_BODY" > /dev/null; then
  echo "tap hit body missing .ok=true" >&2
  cat "$HIT_BODY" >&2
  exit 1
fi
MATCHED_LABEL="$(jq -r '.matched.label // empty' "$HIT_BODY")"
if [[ -z "$MATCHED_LABEL" ]]; then
  echo "tap hit body missing .matched.label" >&2
  cat "$HIT_BODY" >&2
  exit 1
fi

# Phase C: POST /tap with non-existent text — expect 404 + .ok=false + .error=not_found
MISS_BODY="/tmp/simx-tap-miss-$$.json"
MISS_STATUS="$(curl -sS -o "$MISS_BODY" -w '%{http_code}' \
                   --max-time 10 \
                   -X POST -H 'Content-Type: application/json' \
                   -d "{\"selector\":{\"text\":\"$MISS_TEXT\"}}" \
                   "http://127.0.0.1:$PORT/tap")"
if [[ "$MISS_STATUS" != "404" ]]; then
  echo "tap miss expected 404, got $MISS_STATUS" >&2
  cat "$MISS_BODY" >&2 || true
  exit 1
fi
if ! jq -e '.ok == false and .error == "not_found"' "$MISS_BODY" > /dev/null; then
  echo "tap miss body schema wrong" >&2
  cat "$MISS_BODY" >&2
  exit 1
fi

# Single-line JSON summary on stdout.
printf '{"health":%s,"tap_hit":%s,"tap_miss":%s,"matched_label":"%s"}\n' \
       "$HEALTH_STATUS" "$HIT_STATUS" "$MISS_STATUS" "$MATCHED_LABEL"

rm -f "$HIT_BODY" "$MISS_BODY"
exit 0
