#!/usr/bin/env bash
# v24.0-C1 G6: an Android runner that went away, and what the next
# command says about it.
#
# A consumer's own instrumentation replaced smix's runner on the device,
# and the next `smix tap --device <serial>` said "start it with `smix
# runner up`" — without the serial, the platform or the app, the three
# things an Android runner needs. On a real emulator:
#   1. a runner brought up with --bundle records the app in the ledger
#   2. with that runner ended on the device, a tap names the whole
#      command: smix runner up <serial> --platform android --bundle <app>
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
APP="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"

log()  { printf '[v24.0-c1-c] %s\n' "$*" >&2; }
fail() { printf '[v24.0-c1-c] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v24.0-c1-c] cannot judge: %s\n' "$*" >&2; exit 2; }

SERIAL="" UP=0 WE_BOOTED=0
cleanup() {
  if [ "$UP" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" >/dev/null 2>&1 || true
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || cannot_judge "no adb"
if ! SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tail -1)" || [ -z "$SERIAL" ]; then
  with_deadline 300 "$SMIX" sim boot "$E2E_ANDROID" --no-window >"$WORK/boot.log" 2>&1 \
    || fail "boot: $(tail -3 "$WORK/boot.log")"
  SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tail -1)" || fail "$E2E_ANDROID does not resolve"
  WE_BOOTED=1
fi
case "$SERIAL" in emulator-*) : ;; *) fail "$E2E_ANDROID resolves to '$SERIAL' — refusing" ;; esac
"$SMIX" sim install "$SERIAL" "$APK" >/dev/null 2>&1 || fail "could not install the fixture"

log "1. runner up with --bundle records the app"
"$SMIX" runner up "$SERIAL" --platform android --bundle "$APP" --runner-port "$PORT" >"$WORK/up.log" 2>&1 \
  || fail "runner up: $(tail -5 "$WORK/up.log")"
UP=1
ledger="${XDG_DATA_HOME:-$HOME/.local/share}/smix/leases/$SERIAL.json"
recorded="$(python3 -c "
import json, sys
d = json.load(open(sys.argv[1]))
print(','.join(r.get('bundle', '') for r in d['resources'] if r.get('kind') == 'androidRunner'))
" "$ledger")"
[ "$recorded" = "$APP" ] || fail "1: the ledger's runner row records '$recorded', not '$APP'"

log "2. with the runner ended on the device, a tap names the whole command"
adb -s "$SERIAL" shell am force-stop dev.smix.runner.test
adb -s "$SERIAL" shell am force-stop dev.smix.runner
sleep 2
if "$SMIX" tap id:anything --device "$SERIAL" >"$WORK/tap.log" 2>&1; then
  fail "2: a tap with no runner on the device passed"
fi
want="start it with: smix runner up $SERIAL --platform android --bundle $APP"
grep -qF "$want" "$WORK/tap.log" || fail "2: the failure does not say '$want':
$(cat "$WORK/tap.log")"

log "PASS"
