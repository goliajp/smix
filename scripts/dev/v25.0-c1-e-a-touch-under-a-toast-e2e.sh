#!/usr/bin/env bash
# v25.0-C1: a touch at an element something is drawn over.
#
# A consumer's menu button stayed where it was while a toast added after it
# lay across it, the toast's close button over the button's centre. The tap
# went to the close button and was refused afterwards as missed, with a hint
# that said the element had moved. Now the Android runner reads, before the
# touch, what a touch would reach: it touches the part of the target that
# shows, and touches nothing when none does. On a real emulator,
# CoverActivity:
#   1. a tap on the menu button, the card over its lower part, presses the
#      menu button and leaves the card's close button alone
#   2. a tap on a button a panel lies wholly over is refused NOT_VISIBLE,
#      naming the panel, and neither the button nor the panel is touched
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

# $1 id, $2 wanted name
wait_name() {
  for _ in $(seq 1 12); do
    [ "$(name_of "$1")" = "$2" ] && return 0
    sleep 0.25
  done
  return 1
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

log "1. a tap on a button partly under a card presses the part that shows"
"$SMIX" tap --id cover_menu --device "$SERIAL" --port "$AND_PORT" >"$WORK/tap.log" 2>&1 \
  || fail "1: $(cat "$WORK/tap.log")"
wait_name cover_count "pressed 1" || fail "1: the menu button says '$(name_of cover_count)'"
[ "$(name_of cover_closes)" = "closed 0" ] || fail "1: the close button says '$(name_of cover_closes)'"

log "2. a tap on a button a panel lies wholly over is refused, naming the panel"
rc=0; "$SMIX" tap --id cover_hidden --device "$SERIAL" --port "$AND_PORT" >"$WORK/hidden.log" 2>&1 || rc=$?
[ "$rc" != 0 ] || fail "2: a tap on a covered button passed: $(cat "$WORK/hidden.log")"
grep -q "NOT_VISIBLE" "$WORK/hidden.log" || fail "2: the refusal is not NOT_VISIBLE: $(cat "$WORK/hidden.log")"
grep -q "cover_sheet" "$WORK/hidden.log" || fail "2: the refusal does not name the panel: $(cat "$WORK/hidden.log")"
grep -q "nothing was touched" "$WORK/hidden.log" || fail "2: the refusal does not say nothing was touched"
[ "$(name_of cover_hidden_count)" = "hidden 0" ] || fail "2: the hidden button says '$(name_of cover_hidden_count)'"
[ "$(name_of cover_sheet_taps)" = "sheet 0" ] || fail "2: the panel says '$(name_of cover_sheet_taps)': it was touched"

log "PASS"
