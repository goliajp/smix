#!/usr/bin/env bash
# v24.0-C1 G9: a point touched while a screen is still arriving.
#
# A point touch had no wait at all. Sent while a pushed screen was still
# coming in, it was dropped by UIKit and answered `tapped` — five times in
# a row on a consumer's screen. Sent straight after `launchApp`, before the
# app had a frame, it ended the iOS runner (XCTest asserts on a touch at a
# point scaled by nothing). Now a point touch waits for the app to have a
# frame and for two screenshots in a row to agree. On a real device:
#   1. iOS: on the fixture's `-arrive` screen, a point touch on the button
#      that opens a screen, then one at once on the button the new screen
#      brings, presses that button
#   2. iOS: a point touch straight after launchApp passes and the runner is
#      still up after it
#   3. Android: a point touch on a card opens it
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

log()  { printf '[v24.0-c1-a] %s\n' "$*" >&2; }
fail() { printf '[v24.0-c1-a] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v24.0-c1-a] cannot judge: %s\n' "$*" >&2; exit 2; }

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

log "1. iOS: a point touch at once on a screen still arriving presses its button"
printf 'appId: %s\n---\n- tapOn: { point: "50%%,50%%" }\n- tapOn: { point: "50%%,50%%" }\n' \
  "$IOS_APP" >"$WORK/arrive.yaml"
xcrun simctl terminate "$UDID" "$IOS_APP" >/dev/null 2>&1 || true
"$SMIX" sim launch "$UDID" "$IOS_APP" -- -arrive >/dev/null || fail "could not launch the fixture"
for _ in $(seq 1 20); do
  [ "$(name_of "$UDID" "$IOS_PORT" arrive-open)" = "Open slowly" ] && break
  sleep 0.5
done
[ "$(name_of "$UDID" "$IOS_PORT" arrive-open)" = "Open slowly" ] || cannot_judge "the -arrive screen never showed"
with_deadline 120 "$SMIX_RUN" --device "$UDID" --runner-port "$IOS_PORT" --no-launch "$WORK/arrive.yaml" \
  >"$WORK/arrive.log" 2>&1 || fail "1: $(grep -m1 -E 'FAIL|error' "$WORK/arrive.log")"
# the content finishes sliding in 1.5 s after the push
sleep 2
count="$(name_of "$UDID" "$IOS_PORT" arrive-count)"
[ "$count" = "pressed 1" ] || fail "1: the button the new screen brings says '$count', not 'pressed 1'"

log "2. iOS: a point touch straight after launchApp, and the runner after it"
printf 'appId: %s\n---\n- launchApp\n- tapOn: { point: "50%%,90%%" }\n' "$IOS_APP" >"$WORK/launch.yaml"
with_deadline 120 "$SMIX_RUN" --device "$UDID" --runner-port "$IOS_PORT" "$WORK/launch.yaml" \
  >"$WORK/launch.log" 2>&1 || fail "2: $(grep -m1 -E 'FAIL|error' "$WORK/launch.log")"
code="$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$IOS_PORT/health" || true)"
[ "$code" = 200 ] || fail "2: the runner answers /health with '$code' after the touch"

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

log "3. Android: a point touch on a card opens it"
adb -s "$SERIAL" shell am start -S -n "$AND_APP/.NavActivity" >/dev/null 2>&1 || fail "could not open the nav fixture"
for _ in $(seq 1 20); do
  [ -n "$(name_of "$SERIAL" "$AND_PORT" nav_card_3)$(name_of "$SERIAL" "$AND_PORT" nav_list)" ] && break
  sleep 0.5
done
# the middle of card 3's row, read from the screen rather than assumed
at="$("$SMIX" describe --json --device "$SERIAL" --port "$AND_PORT" 2>/dev/null | python3 -c "
import json, sys
b = {e.get('id'): e['bounds'] for e in json.load(sys.stdin)['elements'] if e.get('id')}
s, c = b['nav_scaffold'], b['nav_card_3']
print(f\"{(c['x'] + c['w'] / 2) / s['w'] * 100:.1f}%,{(c['y'] + c['h'] / 2) / s['h'] * 100:.1f}%\")
")" || cannot_judge "the nav fixture's cards are not on screen"
"$SMIX" tap "point:$at" --device "$SERIAL" --port "$AND_PORT" >"$WORK/atap.log" 2>&1 \
  || fail "3: $(tail -3 "$WORK/atap.log")"
sleep 1
detail="$(name_of "$SERIAL" "$AND_PORT" nav_detail_title)"
[ "$detail" = "detail 3" ] || fail "3: the detail shows '$detail', not 'detail 3'"

log "PASS"
