#!/usr/bin/env bash
# v26.0-C1: a hint bar an app hangs on the window's decor view is read.
#
# A consumer reported (2026-10-16) that a hint bar their app added to the
# decor view — beside `android.R.id.content`, not in it — was not in the
# probe's tree. Since 18.0.0 the probe walks each window from its root
# view. On the fixture's DecorHintActivity, a Compose screen with a
# TextView added to the decor view:
#   1. every reader (auto, probe, a11y) lists the hint with its id and text
#   2. `find` and `tap` reach it by its text
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"

log()  { printf '[v26.0-c1-e] %s\n' "$*" >&2; }
fail() { printf '[v26.0-c1-e] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v26.0-c1-e] cannot judge: %s\n' "$*" >&2; exit 2; }

SERIAL="" WE_BOOTED=0 UP=0
cleanup() {
  local said
  if [ -n "$SERIAL" ]; then adb -s "$SERIAL" shell am force-stop "$APPID" >/dev/null 2>&1 || true; fi
  if [ "$UP" = 1 ] && ! said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" 2>&1)"; then
    log "warning: the runner on $PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -f "$APK" ] || fail "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"
SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tr -d '[:space:]')" || true
if [ -z "$SERIAL" ]; then
  "$SMIX" sim boot "$E2E_ANDROID" >"$WORK/boot.log" 2>&1 || fail "emulator boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
  SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" | tr -d '[:space:]')"
fi
case "$SERIAL" in
  emulator-*) ;;
  *) fail "'$E2E_ANDROID' resolves to $SERIAL, which is not an emulator" ;;
esac
"$SMIX" sim install "$SERIAL" "$APK" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
ANDROID_SERIAL="$SERIAL" "$SMIX" runner up "$SERIAL" --platform android --runner-port "$PORT" \
  >"$WORK/up.log" 2>&1 || fail "runner up: $(tail -5 "$WORK/up.log")"
UP=1

adb -s "$SERIAL" shell am start -W -n "$APPID/.DecorHintActivity" >"$WORK/start.log" 2>&1 \
  || fail "starting DecorHintActivity: $(tail -3 "$WORK/start.log")"
i=0
until "$SMIX" find id:decor_screen_title --device "$SERIAL" --port "$PORT" 2>/dev/null | grep -q "exists=true"; do
  i=$((i + 1))
  [ "$i" -lt 20 ] || cannot_judge "the decor-hint screen did not come up"
  sleep 0.5
done

for reader in auto probe a11y; do
  "$SMIX" tree --device "$SERIAL" --port "$PORT" --reader "$reader" >"$WORK/tree-$reader.txt" 2>&1 \
    || fail "smix tree --reader $reader: $(tail -3 "$WORK/tree-$reader.txt")"
  grep -q 'id="fixture_decor_hint" label="" text="Offline, retrying"' "$WORK/tree-$reader.txt" \
    || fail "the $reader reader does not list the hint on the decor view"
  log "ok    the $reader reader lists the hint"
done

"$SMIX" find "text:Offline, retrying" --device "$SERIAL" --port "$PORT" 2>&1 | grep -q "exists=true" \
  || fail "find by the hint's text found nothing"
"$SMIX" tap "text:Offline, retrying" --device "$SERIAL" --port "$PORT" >"$WORK/tap.log" 2>&1 \
  || fail "tap on the hint: $(tail -3 "$WORK/tap.log")"
grep -q "fixture_decor_hint" "$WORK/tap.log" || fail "the tap did not aim inside the hint: $(cat "$WORK/tap.log")"
log "ok    find and tap reach the hint by its text"
log "PASS"
