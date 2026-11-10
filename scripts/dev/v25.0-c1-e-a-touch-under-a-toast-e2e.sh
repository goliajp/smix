#!/usr/bin/env bash
# v25.0-C1: a touch that a toast drawn over its element took.
#
# A consumer's menu button stayed where it was while a toast added after it
# lay across it, the toast's close button over the button's centre. The tap
# was refused as missed — rightly — and the hint said the element had
# moved. Now a miss reads the screen again and says which it was. On a real
# emulator, CoverActivity (a flat button, a card over its lower part):
#   1. a tap on the button fails TAP_MISSED, and the hint says the button is
#      still where it was and names the card's close button over it, not
#      that the button moved
#   2. the button was not pressed; the close button was
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port AND_PORT
AND_APP="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"

log()  { printf '[v25.0-c1-e] %s\n' "$*" >&2; }
fail() { printf '[v25.0-c1-e] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v25.0-c1-e] cannot judge: %s\n' "$*" >&2; exit 2; }

SERIAL="" AND_UP=0 WE_BOOTED_AND=0
cleanup() {
  local said
  if [ "$AND_UP" = 1 ] \
    && ! said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$AND_PORT" 2>&1)"; then
    log "warning: the Android runner on $AND_PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  if [ "$WE_BOOTED_AND" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

# $1 id → the element's name, empty when absent
name_of() {
  "$SMIX" describe --json --device "$SERIAL" --port "$AND_PORT" 2>/dev/null | python3 -c "
import json, sys
for e in json.load(sys.stdin)['elements']:
    if e.get('id') == sys.argv[1]:
        print(e.get('name') or '')
" "$1"
}

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

adb -s "$SERIAL" shell am start -S -n "$AND_APP/.CoverActivity" >/dev/null 2>&1 || fail "could not open CoverActivity"
for _ in $(seq 1 20); do
  [ "$(name_of cover_count)" = "pressed 0" ] && break
  sleep 0.5
done
[ "$(name_of cover_count)" = "pressed 0" ] || cannot_judge "CoverActivity never showed"

log "1. a tap on the button under the card is refused, and says what is over it"
rc=0; "$SMIX" tap --id cover_menu --device "$SERIAL" --port "$AND_PORT" >"$WORK/tap.log" 2>&1 || rc=$?
[ "$rc" != 0 ] || fail "1: a tap whose touch the card took passed: $(cat "$WORK/tap.log")"
grep -q "TAP_MISSED" "$WORK/tap.log" || fail "1: the refusal is not TAP_MISSED: $(cat "$WORK/tap.log")"
grep -q "id=cover_menu is still where it was aimed at" "$WORK/tap.log" \
  || fail "1: the hint does not say the button stayed: $(cat "$WORK/tap.log")"
grep -q "id=cover_close is drawn over it" "$WORK/tap.log" \
  || fail "1: the hint does not name what is over it: $(cat "$WORK/tap.log")"
! grep -q "moved between the tree fetch" "$WORK/tap.log" \
  || fail "1: the hint still says the button moved: $(cat "$WORK/tap.log")"

log "2. the touch went to the card's close button, not the menu button"
[ "$(name_of cover_count)" = "pressed 0" ] || fail "2: the menu button says '$(name_of cover_count)'"
[ "$(name_of cover_closes)" = "closed 1" ] || fail "2: the close button says '$(name_of cover_closes)', not 'closed 1'"

log "PASS"
