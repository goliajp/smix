#!/usr/bin/env bash
# v2.10-C4 Android record -> generate e2e: record a live tap/fill on the Android
# runner, drain the IRAction, and generate a maestro flow. Device work — pins
# the emulator serial (a physical phone must never be touched), sweeps on exit.
set -euo pipefail

SERIAL="${1:-emulator-5554}"
PORT="${SMIX_ANDROID_PORT:-28080}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="$ROOT/target/release/smix"
R="http://localhost:$PORT"

log()  { printf '[c4-android] %s\n' "$*"; }
fail() { printf '[c4-android] FAIL: %s\n' "$*" >&2; exit 1; }

case "$SERIAL" in emulator-*) ;; *) fail "serial must be an emulator (got $SERIAL); never a physical phone" ;; esac
[ -x "$SMIX" ] || fail "smix not built at $SMIX"

WORK="$(mktemp -d)"
cleanup() {
  log "teardown: runner down + sweep"
  "$SMIX" runner down --platform android --device "$SERIAL" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

log "runner up $SERIAL (Android takes its target per-request, no --bundle)"
"$SMIX" runner up "$SERIAL" --platform android >"$WORK/up.log" 2>&1 &
for i in $(seq 1 90); do sleep 2; curl -sf --max-time 2 "$R/health" >/dev/null 2>&1 && break; [ "$i" = 90 ] && { cat "$WORK/up.log"; fail "runner never healthy"; }; done

log "launch Settings to record against"
adb -s "$SERIAL" shell am force-stop com.android.settings >/dev/null 2>&1 || true
adb -s "$SERIAL" shell am start -n com.android.settings/.Settings >/dev/null 2>&1
sleep 2

log "record a tap + fill"
curl -sf -X POST "$R/record/start" >/dev/null
curl -sf -X POST "$R/tap-by-id" -H 'Content-Type: application/json' --data '{"id":"search_action_bar"}' >/dev/null; sleep 1
curl -sf -X POST "$R/tap-by-id" -H 'Content-Type: application/json' --data '{"id":"search_src_text"}' >/dev/null; sleep 1
curl -sf -X POST "$R/input-text" -H 'Content-Type: application/json' --data '{"text":"smix"}' >/dev/null; sleep 1
curl -sf -X POST "$R/record/stop" -o "$WORK/stop.json"

log "extract IRAction + generate maestro"
python3 -c "import json,sys; json.dump(json.load(open('$WORK/stop.json'))['events'], open('$WORK/events.json','w'))"
"$SMIX" authoring generate "$WORK/events.json" --format maestro -o "$WORK/flow.yaml"

grep -q 'tapOn' "$WORK/flow.yaml" || fail "generated flow missing tapOn: $(cat "$WORK/flow.yaml")"
grep -q 'inputText' "$WORK/flow.yaml" || fail "generated flow missing inputText"
log "generated flow:"; sed 's/^/    /' "$WORK/flow.yaml"
log "C4-ANDROID-E2E-PASS"
