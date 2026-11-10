#!/usr/bin/env bash
# v25.0-C1 G9: a point touched on a screen with a video playing.
#
# A point touch waits for two screenshots in a row to agree, and a screen
# with a live video on it never had two: every point touch on a consumer's
# home screen waited out the whole 3 s limit. Now a part of the screen away
# from the point that keeps changing for over a second is left out as
# playing. On a real device:
#   1. iOS: on `-playing` (the top 45% redrawn every frame), a point touch on
#      the button below presses it, waiting under 2 s, and says what it left out
#   2. iOS: on `-playing`, a point touch inside the playing picture still
#      waits out the limit and says the screen was moving: motion near the
#      point is never left out
#   3. Android: on PlayingActivity, the same as 1
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port IOS_PORT
gate_free_port AND_PORT
IOS_APP="jp.golia.smix.fixture"
AND_APP="dev.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"

log()  { printf '[v25.0-c1-d] %s\n' "$*" >&2; }
fail() { printf '[v25.0-c1-d] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v25.0-c1-d] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" SERIAL="" IOS_UP=0 AND_UP=0 WE_BOOTED_IOS=0 WE_BOOTED_AND=0
cleanup() {
  local said
  if [ "$IOS_UP" = 1 ] && ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" 2>&1)"; then
    log "warning: the iOS runner on $IOS_PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  if [ "$AND_UP" = 1 ] \
    && ! said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$AND_PORT" 2>&1)"; then
    log "warning: the Android runner on $AND_PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  if [ "$WE_BOOTED_IOS" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  if [ "$WE_BOOTED_AND" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

# $1 device, $2 port, $3 id → the element's name, empty when absent
name_of() {
  "$SMIX" describe --json --device "$1" --port "$2" 2>/dev/null | python3 -c "
import json, sys
for e in json.load(sys.stdin)['elements']:
    if e.get('id') == sys.argv[1]:
        print(e.get('name') or '')
" "$3"
}

# $1 tap log → the milliseconds the touch says it waited, empty when it says none
waited_ms() { { grep -oE 'after [0-9]+ ms' "$1" || true; } | head -1 | tr -cd '0-9'; }

# --- iOS
UDID="$("$SMIX" sim resolve "$E2E_IOS" 2>/dev/null | tail -1 | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$E2E_IOS'"
case "$(simulator_name "$UDID")" in
  sim-smix-*) ;;
  *) cannot_judge "'$E2E_IOS' resolves to $UDID, which is not one of smix's own simulators" ;;
esac
if [ "$(simulator_state "$UDID")" != Booted ]; then
  with_deadline 300 "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED_IOS=1
fi
"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
"$SMIX" runner up "$UDID" --bundle "$IOS_APP" --runner-port "$IOS_PORT" >"$WORK/up.log" 2>&1 \
  || fail "iOS runner up: $(tail -5 "$WORK/up.log")"
IOS_UP=1

# $1 launch argument, $2 an id on that screen
open_ios() {
  xcrun simctl terminate "$UDID" "$IOS_APP" >/dev/null 2>&1 || true
  "$SMIX" sim launch "$UDID" "$IOS_APP" -- "$1" >/dev/null || fail "could not launch the fixture with $1"
  for _ in $(seq 1 20); do
    [ -n "$(name_of "$UDID" "$IOS_PORT" "$2")" ] && return 0
    sleep 0.5
  done
  cannot_judge "the $1 screen never showed"
}

log "1. iOS: a point touch beside a playing picture presses the button, without waiting out the limit"
open_ios -playing playing-press
"$SMIX" tap "point:50%,75%" --device "$UDID" --port "$IOS_PORT" >"$WORK/tap1.log" 2>&1 \
  || fail "1: $(tail -3 "$WORK/tap1.log")"
ms="$(waited_ms "$WORK/tap1.log")"
sleep 0.5
count="$(name_of "$UDID" "$IOS_PORT" playing-count)"
[ "$count" = "pressed 1" ] || fail "1: the button says '$count', not 'pressed 1'"
[ -n "$ms" ] && [ "$ms" -lt 2000 ] || fail "1: the touch waited '$ms' ms; a playing picture away from the point still held it: $(cat "$WORK/tap1.log")"
grep -q "video or animation playing" "$WORK/tap1.log" \
  || fail "1: the answer does not say a part was left out as playing: $(cat "$WORK/tap1.log")"
log "   waited $ms ms: $(grep -o 'part of the screen ([^)]*)' "$WORK/tap1.log" | head -1)"

log "2. iOS: a point touch inside the playing picture still waits and says it was moving"
"$SMIX" tap "point:50%,20%" --device "$UDID" --port "$IOS_PORT" >"$WORK/tap2.log" 2>&1 \
  || fail "2: $(tail -3 "$WORK/tap2.log")"
grep -q "still moving after" "$WORK/tap2.log" \
  || fail "2: a touch at a point that never stops moving did not say so: $(cat "$WORK/tap2.log")"

# --- Android
command -v adb >/dev/null 2>&1 || cannot_judge "no adb"
if ! SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tail -1)" || [ -z "$SERIAL" ]; then
  with_deadline 300 "$SMIX" sim boot "$E2E_ANDROID" --no-window >"$WORK/aboot.log" 2>&1 \
    || fail "boot: $(tail -3 "$WORK/aboot.log")"
  SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tail -1)" || fail "$E2E_ANDROID does not resolve"
  WE_BOOTED_AND=1
fi
case "$SERIAL" in emulator-*) : ;; *) fail "$E2E_ANDROID resolves to '$SERIAL' — refusing" ;; esac
"$SMIX" sim install "$SERIAL" "$APK" >/dev/null 2>&1 || fail "could not install the fixture"
"$SMIX" runner up "$SERIAL" --platform android --bundle "$AND_APP" --runner-port "$AND_PORT" >"$WORK/aup.log" 2>&1 \
  || fail "Android runner up: $(tail -5 "$WORK/aup.log")"
AND_UP=1

log "3. Android: a point touch beside a playing picture presses the button, without waiting out the limit"
adb -s "$SERIAL" shell am start -S -n "$AND_APP/.PlayingActivity" >/dev/null 2>&1 || fail "could not open PlayingActivity"
for _ in $(seq 1 20); do
  [ -n "$(name_of "$SERIAL" "$AND_PORT" playing-count)" ] && break
  sleep 0.5
done
# the middle of the button, read from the screen rather than assumed
at="$("$SMIX" describe --json --device "$SERIAL" --port "$AND_PORT" 2>/dev/null | python3 -c "
import json, sys
b = {e.get('id'): e['bounds'] for e in json.load(sys.stdin)['elements'] if e.get('id')}
s, c = b['playing-screen'], b['playing-press']
print(f\"{(c['x'] + c['w'] / 2) / s['w'] * 100:.1f}%,{(c['y'] + c['h'] / 2) / s['h'] * 100:.1f}%\")
")" || cannot_judge "PlayingActivity's button is not on screen"
"$SMIX" tap "point:$at" --device "$SERIAL" --port "$AND_PORT" >"$WORK/tap3.log" 2>&1 \
  || fail "3: $(tail -3 "$WORK/tap3.log")"
ms="$(waited_ms "$WORK/tap3.log")"
sleep 0.5
count="$(name_of "$SERIAL" "$AND_PORT" playing-count)"
[ "$count" = "pressed 1" ] || fail "3: the button says '$count', not 'pressed 1', after a touch at $at: $(cat "$WORK/tap3.log")"
[ -n "$ms" ] && [ "$ms" -lt 2000 ] || fail "3: the touch waited '$ms' ms; a playing picture away from the point still held it: $(cat "$WORK/tap3.log")"
grep -q "video or animation playing" "$WORK/tap3.log" \
  || fail "3: the answer does not say a part was left out as playing: $(cat "$WORK/tap3.log")"
log "   waited $ms ms: $(grep -o 'part of the screen ([^)]*)' "$WORK/tap3.log" | head -1)"

log "PASS"
