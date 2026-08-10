#!/usr/bin/env bash
# v2.10-C4 Android record -> generate e2e: record a live tap/fill on the Android
# runner, drain the IRAction, and generate a maestro flow. Device work — pins
# the emulator serial (a physical phone must never be touched), sweeps on exit.
set -euo pipefail

SERIAL="${1:-emulator-5554}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# A port of this gate's own, so a bystander runner cannot turn it red.
. "$ROOT/scripts/lib/gate-port.sh"
# The port the runner is actually on, which since gate-port.sh landed is
# not 28080 unless someone says so.
#
# `runner up` reads SMIX_RUNNER_PORT from the environment, and
# gate-port.sh exports a free one so a bystander's runner cannot turn
# this red. This line kept probing 28080 regardless, so the health loop
# knocked ninety times on a door nobody was behind and reported "runner
# never healthy" — a self-inflicted break that reads exactly like the
# product failing to start.
PORT="${SMIX_ANDROID_PORT:-$SMIX_RUNNER_PORT}"

SMIX="$ROOT/target/release/smix"
R="http://localhost:$PORT"

log()  { printf '[c4-android] %s\n' "$*"; }
fail() { printf '[c4-android] FAIL: %s\n' "$*" >&2; exit 1; }

# A precondition this script detects and cannot satisfy is a SKIP with
# what to do about it — not a FAIL. Yielding to somebody else's batch, or
# an unset target, says nothing about whether smix works, and FAIL says it
# does not to whoever reads the suite next.
skip() { printf '[c4-android] %s\n' "$*" >&2; printf '%s\n' "C4-ANDROID-RECORD-SKIP"; exit 0; }


case "$SERIAL" in emulator-*) ;; *) fail "serial must be an emulator (got $SERIAL); never a physical phone" ;; esac
[ -x "$SMIX" ] || fail "smix not built at $SMIX"

WORK="$(mktemp -d)"
cleanup() {
  # To stderr: the EXIT trap runs *after* the verdict is printed, so a
  # teardown note on stdout becomes the last line and a reader tailing
  # the output sees housekeeping where the result should be.
  log "teardown: runner down + sweep" >&2
  "$SMIX" runner down --platform android --device "$SERIAL" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

# Before anything touches the device: is there one? Without this the run
# reached `runner up`, failed there, and reported a defect — while the
# actual state was "no emulator is running", which says nothing about
# smix. The verdict also has to be the last line, and a trap's teardown
# prints after a FAIL, so a reader tailing the output saw the teardown.
if ! adb devices 2>/dev/null | awk -F'\t' -v s="$SERIAL" '$1==s && $2=="device" {found=1} END {exit !found}'; then
  skip "no ready adb device \"$SERIAL\" — start one (emulator -avd sim-smix-android-01) or pass a running serial"
fi

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
