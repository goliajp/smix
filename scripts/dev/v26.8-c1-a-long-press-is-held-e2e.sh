#!/usr/bin/env bash
# v26.8-C1: an Android long press is one press held, and the app says so.
#
# A consumer's `smix long-press --duration-ms 1500` failed on every element
# of their app with `/long-press-at-norm-coord answered ok:false`, each call
# taking 10 s. The runner held a press by `UiDevice.swipe(x, y, x, y, steps)`
# — a move every 5 ms, each waiting for the app to finish it — so 1.5 s was
# 300 moves, 10.8 s on the fixture's idle screen, and an app slow to finish
# them failed it outright. Nothing had ever checked that an Android long
# press is received as one: every gate read the runner's own answer.
#
# Then, from the same consumer: a press held 1.5 s and released on a control
# with only a click handler did not click it, where the same press on iOS
# does. Testing whether a tap survives the control being rebuilt mid-press
# needs exactly that press.
#
# The fixture's hold screen counts long presses and taps apart, and clicks
# of a button that has no long-press handler, read from the app. On the
# idle screen and on one that spends most of each frame on the main
# thread: two 1.5 s presses count two long presses and no tap, a 100 ms
# press counts a tap and no long press, a 1.5 s press on the plain button
# clicks it once after the finger was down 1450–2000 ms by the touch's own
# times, and each 1.5 s press returns within 4 s (it was 10 s and more).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
ALIAS="${SMIX_V268_C1_ANDROID:-$E2E_ANDROID}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"

log()  { printf '[v26.8-c1] %s\n' "$*" >&2; }
fail() { printf '[v26.8-c1] FAIL: %s\n' "$*" >&2; exit 1; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0
cleanup() {
  if [ "$WE_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
      >/dev/null || printf '[v26.8-c1] warning: the runner was not stopped\n' >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || fail "no adb on PATH — this judges an Android runner and cannot"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
[ -f "$APK" ] || fail "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"

if ! SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" \
   || [ -z "$SERIAL" ] || ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  log "booting $ALIAS"
  with_deadline 300 "$SMIX" sim boot "$ALIAS" >/dev/null 2>&1 || fail "could not boot $ALIAS"
  WE_BOOTED=1
  SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" || fail "$ALIAS was booted and does not resolve"
fi
case "$SERIAL" in
  emulator-*) : ;;
  # This installs a fixture app; a registered phone belongs to somebody.
  *) fail "$ALIAS resolves to '$SERIAL', which is not an emulator — refusing" ;;
esac
with_deadline 120 adb -s "$SERIAL" wait-for-device || fail "$SERIAL did not come up"
for _ in $(seq 1 60); do
  adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done

adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"
if ! curl -s -m 5 "http://localhost:$PORT/health" >/dev/null 2>&1; then
  "$SMIX" runner up "$ALIAS" --platform android --runner-port "$PORT" >/dev/null 2>&1 \
    || fail "the runner would not start on $SERIAL:$PORT"
  WE_UPPED=1
fi

read_id() { # $1 resource id; prints the text the app shows there
  "$SMIX" tree --device "$SERIAL" --platform android --port "$PORT" --json | python3 -c '
import json, sys
want = sys.argv[1]
def walk(n):
    if n.get("identifier") == want:
        print(n.get("text") or n.get("label") or "")
        return True
    return any(walk(c) for c in n.get("children", []))
walk(json.load(sys.stdin)["root"]) or print("no " + want + " on screen")' "$1"
}

press() { # $1 ms, $2 target; fails the gate when it errors or a 1.5 s press is slow
  local t0 t1 out
  t0=$(python3 -c 'import time; print(time.monotonic())')
  out="$("$SMIX" long-press "id:$2" --device "$SERIAL" --platform android --port "$PORT" \
    --duration-ms "$1" 2>&1)" || { printf '%s\n' "$out" >&2; fail "a $1 ms long press failed"; }
  t1=$(python3 -c 'import time; print(time.monotonic())')
  python3 -c "import sys; e=$t1-$t0; print(f'{e:.2f}'); sys.exit(0 if $1 < 1500 or e < 4 else 1)" >&2 \
    || fail "a $1 ms long press took longer than 4 s"
}

for screen in idle loaded; do
  loaded=false; [ "$screen" = loaded ] && loaded=true
  adb -s "$SERIAL" shell am start -W -S -n "$APPID/.HoldActivity" --ez busy "$loaded" >/dev/null 2>&1 \
    || fail "could not start the hold screen"
  press 1500 hold_target
  press 1500 hold_target
  press 100 hold_target
  press 1500 hold_plain
  down="$(read_id hold_last)"
  ms="${down#down }"; ms="${ms% ms}"
  case "$ms" in ''|*[!0-9]*) fail "$screen screen: the plain button's last press reads '$down'" ;; esac
  [ "$ms" -ge 1450 ] && [ "$ms" -le 2000 ] \
    || fail "$screen screen: a 1500 ms press was down $ms ms in the app, not 1450–2000"
  got="$(read_id hold_result)"
  [ "$got" = "held 2 tapped 1 clicked 1" ] \
    || fail "$screen screen: two 1.5 s presses and a 100 ms one on the long-press button and a 1.5 s one on the plain button left the app saying '$got', not 'held 2 tapped 1 clicked 1'"
  log "$screen screen: the app counted two long presses, one tap, and one click of the plain button, held $ms ms"
done

log "V26.8-C1-E2E-PASS on $SERIAL"
