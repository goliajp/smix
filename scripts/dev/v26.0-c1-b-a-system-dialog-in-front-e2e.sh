#!/usr/bin/env bash
# v26.0-C1: the system's permission dialog in front, on an Android emulator.
#
# A consumer's app (carrying the probe) raised the notification permission
# dialog. `smix tree` opened with a note about the app's Compose dialogs
# and told them to add the probe they already had, with the dialog's
# buttons at the bottom of an outline whose first thirty lines were the
# status bar; and `smix tap "text:Don't allow"` matched nothing, because
# the button reads `Don’t allow` with a typographic apostrophe. With the
# fixture's camera request (the same dialog, owned by the permission
# controller):
#   1. `smix tree` opens by naming the system's permission dialog and its
#      buttons, and says nothing about Compose or the probe
#   2. `smix tap "text:Don't allow"`, typed with a plain apostrophe, presses
#      it: the dialog's window goes, and the permission is marked as the
#      person's answer (USER_SET, cleared before asking) and not held
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

log()  { printf '[v26.0-c1-b] %s\n' "$*" >&2; }
fail() { printf '[v26.0-c1-b] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v26.0-c1-b] cannot judge: %s\n' "$*" >&2; exit 2; }

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

# Not held, so asking raises the dialog; the flags go too, or a
# permission denied twice is refused without a dialog from then on.
adb -s "$SERIAL" shell am force-stop com.android.permissioncontroller >/dev/null 2>&1 || true
adb -s "$SERIAL" shell pm revoke "$APPID" android.permission.CAMERA >/dev/null 2>&1 || true
adb -s "$SERIAL" shell pm clear-permission-flags "$APPID" android.permission.CAMERA user-set user-fixed \
  >/dev/null 2>&1 || true
printf 'appId: %s\n---\n- launchApp\n- tapOn:\n    id: fixture_ask_camera\n' "$APPID" >"$WORK/ask.yaml"
SMIX_RUNNER_PORT="$PORT" "$SMIX_RUN" --device "$SERIAL" "$WORK/ask.yaml" >"$WORK/ask.log" 2>&1 \
  || fail "asking for the camera: $(tail -5 "$WORK/ask.log")"

dialog_up() {
  curl -s -m 10 "http://localhost:$PORT/windows" | python3 -c 'import json,sys
w=json.load(sys.stdin)["windows"]
sys.exit(0 if any("permissioncontroller" in (r.get("package") or "") for r in w) else 1)'
}
i=0
until dialog_up; do
  i=$((i + 1))
  [ "$i" -lt 40 ] || cannot_judge "the permission dialog did not come up within 10 s of asking"
  sleep 0.25
done

"$SMIX" tree --device "$SERIAL" --port "$PORT" >"$WORK/tree.txt" 2>&1 || fail "smix tree: $(tail -3 "$WORK/tree.txt")"
head -1 "$WORK/tree.txt" >&2
head -1 "$WORK/tree.txt" | grep -q "the system's permission dialog" \
  || fail "the tree's first line does not name the system's permission dialog"
head -1 "$WORK/tree.txt" | grep -q "allow" || fail "the tree's first line does not list the dialog's buttons"
if grep -qi "compose\|smix-probe" "$WORK/tree.txt"; then
  fail "the tree still tells the reader about Compose or the probe"
fi
log "ok    the tree opens on the system's permission dialog and its buttons"

"$SMIX" tap "text:Don't allow" --device "$SERIAL" --port "$PORT" >"$WORK/tap.log" 2>&1 \
  || fail "tap with a plain apostrophe: $(tail -5 "$WORK/tap.log")"
i=0
while dialog_up; do
  i=$((i + 1))
  [ "$i" -lt 40 ] || fail "the tap reported success and the dialog is still there"
  sleep 0.25
done
held="$(adb -s "$SERIAL" shell dumpsys package "$APPID" | grep -m1 'android.permission.CAMERA: granted=' || true)"
log "  $held"
case "$held" in
  *granted=false*USER_SET*) ;;
  *) fail "after pressing Don't allow the camera reads: ${held:-nothing}" ;;
esac
log "ok    tap \"text:Don't allow\" pressed the typeset button"
log "PASS"
