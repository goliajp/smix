#!/usr/bin/env bash
# v21.0-C1: a right click and a wheel turn reach the element they name, in
# an app's desktop window.
#
# They are judged here, on an Android 16 tablet with freeform windows on,
# and not on the Android desktop image: that image's Android 14 build
# routes a wheel turn or a button press to the view under (x, x) instead of
# (x, y) — its ViewGroup reads the y of the event from the x — so whether
# the right view answers depends on where the window happens to sit.
#
# The fixture's desktop screen writes what reached it into labels:
#   1. `rightClickOn` the context row: "context clicks 1"
#   2. `mouseWheel` down 1, then down 2 with Ctrl: "wheel -3 ctrl yes"
#   3. `mouseWheel` up 3: "wheel 0 ctrl no"
#   4. `mouseDrag` from the drag row to the keys row, with the mouse:
#      "drag 0 <dy> mouse", dy the distance between the two centres
#
# Runs on smix's own tablet emulator (E2E_ANDROID_TABLET), booted with no
# window (--no-window) when it is off.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
ALIAS="$E2E_ANDROID_TABLET"
APP="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"

log()  { printf '[v21.0-c1] %s\n' "$*" >&2; }
fail() { printf '[v21.0-c1] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v21.0-c1] cannot judge: %s\n' "$*" >&2; exit 2; }

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

flow() { # $1 name, rest: steps
  local name="$1"; shift
  { printf 'appId: %s\n---\n' "$APP"; printf '%s\n' "$@"; } >"$WORK/$name.yaml"
  "$SMIX_RUN" --device "$DEVICE" --runner-port "$PORT" "$WORK/$name.yaml" >"$WORK/$name.log" 2>&1 \
    || { tail -15 "$WORK/$name.log" >&2; fail "flow '$name' failed"; }
}

tree_json() {
  "$SMIX" tree --device "$DEVICE" --runner-port "$PORT" --json 2>/dev/null
}

label() { # the text of the element with this id
  tree_json | python3 -c '
import json, sys
want = sys.argv[1]
def walk(n):
    if n.get("identifier") == want:
        print(n.get("text") or n.get("label") or ""); sys.exit(0)
    for c in n.get("children", []): walk(c)
walk(json.load(sys.stdin)["root"])' "$1"
}

is() { # $1 id, $2 expected text, $3 step
  local got
  got="$(label "$1")"
  [ "$got" = "$2" ] || fail "$3: $1 reads '$got', expected '$2'"
}

command -v adb >/dev/null 2>&1 || cannot_judge "no adb"
[ -f "$APK" ] || fail "no fixture apk (bash scripts/dev/build-android-fixture.sh)"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"
if ! DEVICE="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" || [ -z "$DEVICE" ]; then
  with_deadline 300 "$SMIX" sim boot "$ALIAS" --no-window >"$WORK/boot.log" 2>&1 \
    || cannot_judge "$ALIAS could not be booted: $(tail -3 "$WORK/boot.log")"
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
sdk="$(adb -s "$DEVICE" shell getprop ro.build.version.sdk 2>/dev/null | tr -d '\r')"
[ "${sdk:-0}" -ge 35 ] \
  || cannot_judge "$ALIAS ($DEVICE) is API ${sdk:-?}; a right click and a wheel turn are judged on 35 or later"
# freeform windows are off on a tablet until asked for; the setting is this emulator's own
adb -s "$DEVICE" shell settings put global enable_freeform_support 1
adb -s "$DEVICE" shell settings put global force_resizable_activities 1
adb -s "$DEVICE" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"
"$SMIX" runner up "$DEVICE" --platform android --runner-port "$PORT" >"$WORK/up.log" 2>&1 \
  || fail "runner up: $(tail -5 "$WORK/up.log")"
UP=1

adb -s "$DEVICE" shell am start -W -S --windowingMode 5 -n "$APP/.DesktopActivity" >/dev/null 2>&1 \
  || fail "could not start the desktop screen in a window"
flow open '- extendedWaitUntil: { visible: { id: desk_context }, timeout: 10000 }'
adb -s "$DEVICE" shell am stack list | grep -B3 "$APP/" | grep -q 'mWindowingMode=freeform' \
  || cannot_judge "the fixture did not open in a freeform window"
[ "$(label desk_context)" = "context clicks 0" ] || cannot_judge "the desktop screen did not open fresh"

log "1. rightClickOn"
flow right-click '- rightClickOn: { id: desk_context }'
is desk_context "context clicks 1" "1"

log "2. mouseWheel down, then down with Ctrl"
flow wheel-down \
  '- mouseWheel: { on: { id: desk_scroll }, direction: down }' \
  '- mouseWheel: { on: { id: desk_scroll }, direction: down, notches: 2, ctrl: true }'
is desk_scroll "wheel -3 ctrl yes" "2"

log "3. mouseWheel up"
flow wheel-up '- mouseWheel: { on: { id: desk_scroll }, direction: up, notches: 3 }'
is desk_scroll "wheel 0 ctrl no" "3"

log "4. mouseDrag from one element to another"
dpi="$(adb -s "$DEVICE" shell wm density | grep -oE '[0-9]+' | tail -1)"
dy="$(tree_json | python3 -c '
import json, sys
c = {}
def walk(n):
    if n.get("identifier") in ("desk_drag", "desk_keys"):
        b = n["bounds"]; c[n["identifier"]] = b["y"] + b["h"] / 2
    for k in n.get("children", []): walk(k)
walk(json.load(sys.stdin)["root"])
print(round((c["desk_keys"] - c["desk_drag"]) / (int(sys.argv[1]) / 160)))' "$dpi")"
flow drag '- mouseDrag: { from: { id: desk_drag }, to: { id: desk_keys } }'
got="$(label desk_drag)"
case "$got" in
  "drag 0 $dy mouse" | "drag 0 $((dy - 1)) mouse" | "drag 0 $((dy + 1)) mouse") : ;;
  *) fail "4: desk_drag reads '$got', expected 'drag 0 $dy mouse' (one dp either way)" ;;
esac

log "V21.0-C1-MOUSE-BUTTONS-PASS"
