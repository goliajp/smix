#!/usr/bin/env bash
# v22.0-C1: a tap inside a dialog shown as a card, over a scrim popup, is
# judged to land where it did.
#
# A consumer's app shows its sheets as a centred card at tablet width: the
# card is a dialog window, the scrim a full-screen popup attached to the
# activity and shown after the dialog. The probe stacked windows in the
# order they were added, so the scrim read as the window on top, and every
# tap in the card failed TAP_MISSED although it landed. The fixture's card
# screen counts what each touch reached:
#   1. `tapOn` the card's row and its button pass, and the app counted both
#   2. a button behind the card is not reachable while the card is open,
#      and nothing behind it, the scrim included, was touched
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
ALIAS="$E2E_ANDROID"
APP="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"

log()  { printf '[v22.0-c1-b] %s\n' "$*" >&2; }
fail() { printf '[v22.0-c1-b] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v22.0-c1-b] cannot judge: %s\n' "$*" >&2; exit 2; }

DEVICE="" UP=0 WE_BOOTED=0
cleanup() {
  local said
  if [ "$UP" = 1 ] \
    && ! said="$("$SMIX" runner down --platform android --device "$DEVICE" --runner-port "$PORT" 2>&1)"; then
    log "warning: the runner on $PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$DEVICE" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

counts() {
  "$SMIX" tree --device "$DEVICE" --runner-port "$PORT" 2>/dev/null \
    | grep -oE 'custom [0-9]+ applied [0-9]+ behind [0-9]+ scrim [0-9]+' | head -1
}

command -v adb >/dev/null 2>&1 || cannot_judge "no adb"
[ -f "$APK" ] || fail "no fixture apk (bash scripts/dev/build-android-fixture.sh)"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"
if ! DEVICE="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" || [ -z "$DEVICE" ]; then
  with_deadline 300 "$SMIX" sim boot "$ALIAS" --no-window >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
  DEVICE="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" || fail "$ALIAS was booted and does not resolve"
  WE_BOOTED=1
fi
case "$DEVICE" in
  emulator-*) : ;;
  *) fail "$ALIAS resolves to '$DEVICE', which is not an emulator — refusing" ;;
esac
for _ in $(seq 1 60); do
  adb -s "$DEVICE" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done
adb -s "$DEVICE" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"
"$SMIX" runner up "$DEVICE" --platform android --runner-port "$PORT" >"$WORK/up.log" 2>&1 \
  || fail "runner up: $(tail -5 "$WORK/up.log")"
UP=1

open() {
  adb -s "$DEVICE" shell am start -W -S -n "$APP/.CardActivity" >/dev/null 2>&1 || fail "could not start the card screen"
  printf 'appId: %s\n---\n- extendedWaitUntil: { visible: { id: card_open }, timeout: 10000 }\n' "$APP" >"$WORK/open.yaml"
  "$SMIX_RUN" --device "$DEVICE" --runner-port "$PORT" "$WORK/open.yaml" >"$WORK/open.log" 2>&1 \
    || cannot_judge "the card screen did not come up"
  [ "$(counts)" = "custom 0 applied 0 behind 0 scrim 0" ] || cannot_judge "the card screen did not open fresh: $(counts)"
}

log "1. taps inside the card land and are judged to"
open
printf 'appId: %s\n---\n- tapOn: { id: card_open }\n- tapOn: { id: card_custom }\n- tapOn: { id: card_apply }\n' "$APP" >"$WORK/in.yaml"
# raw run: a TAP_MISSED here is the verdict this script exists for
if ! "$SMIX" run --device "$DEVICE" --runner-port "$PORT" "$WORK/in.yaml" >"$WORK/in.log" 2>&1; then
  fail "1: a tap inside the card failed: $(grep -m1 -E 'FAIL|error' "$WORK/in.log")"
fi
grep -q "did not land" "$WORK/in.log" && fail "1: a tap inside the card was judged a miss: $(grep -m1 'did not land' "$WORK/in.log")"
adb -s "$DEVICE" shell input keyevent KEYCODE_BACK
got="$(counts)"
[ "$got" = "custom 1 applied 1 behind 0 scrim 0" ] || fail "1: the app counted '$got'"

log "2. a button behind the open card is not reached"
open
printf 'appId: %s\n---\n- tapOn: { id: card_open }\n- extendedWaitUntil: { visible: { id: card_apply }, timeout: 5000 }\n- tapOn: { id: card_behind }\n' "$APP" >"$WORK/behind.yaml"
# raw run: the refusal of this tap is what is judged
if "$SMIX" run --device "$DEVICE" --runner-port "$PORT" "$WORK/behind.yaml" >"$WORK/behind.log" 2>&1; then
  fail "2: a tap on a button behind the open card passed"
fi
adb -s "$DEVICE" shell input keyevent KEYCODE_BACK
got="$(counts)"
[ "$got" = "custom 0 applied 0 behind 0 scrim 0" ] || fail "2: something behind the card was touched: '$got'"

log "V22.0-C1-B-CARD-DIALOG-PASS"
