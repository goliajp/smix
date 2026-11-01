#!/usr/bin/env bash
# v20.0-C3: an app in an Android desktop window (a Chromebook build) is
# driven with a mouse.
#
# A consumer's Chromebook audit checked its hover styles by eye, five or six
# rounds of them: `adb shell input` has no hover, and every smix gesture was
# a touch. The fixture's desktop screen writes what the pointer did into
# labels — the row's hover, the button's hover and taps — so each judgement
# is the app's own word, not the runner's answer:
#   1. `hoverOn` the row: the row reads "hover in 1"
#   2. `hoverOn` the button: the row the pointer left reads "hover out 1",
#      the button "in 1 taps 0"
#   3. a `tapOn` after the hover still lands: "taps 1"
#   4. `hoverAway` with nothing hovered (the tap ended the hover) passes and
#      changes nothing; after another hover it leaves the app: the button
#      reads "out" again. The system drops a pointer's exit when it hovers
#      no window, which the runner once passed on as a failure
#   5. `smix hover` / `smix hover --away` do the same from the command line
#   6. iOS refuses `smix hover` by name, before any request is sent
#   7. `setWindowSize` 1024 -> 839 -> compact -> 1024 in one flow: the
#      screen reads the window's dp and the layout class (600 / 840 dp)
#      after each
#   8. a size below the system's smallest window, or larger than the
#      display, is refused naming the limit, and the window keeps its size
#   9. `smix window-size` does the same from the command line; iOS refuses
#      it by name
#  10. `takeScreenshot: { window: true }` writes exactly the window: the
#      PNG measures the task's bounds, its caption bar included
#  11. `pressKey: ctrl+s` and `ctrl+shift+z` reach the app with their
#      modifiers held: "keys ctrl+S ctrl+shift+Z"
#  12. `mouseDrag` by 96 dp moves the mouse 96 dp: "drag 96 0 mouse"
#  13. the caption bar's back arrow is an element: `tapOn: { id:
#      back_button }` closes the desktop screen
#
# A right click and a wheel turn are judged on an Android 16 tablet
# instead (v21.0-c1-mouse-buttons-e2e.sh): this desktop image routes them
# to the wrong view, a bug of its Android 14 build.
#
# Runs on smix's own desktop emulator (E2E_ANDROID_DESKTOP), booted with no
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
gate_free_port UNUSED_PORT
ALIAS="$E2E_ANDROID_DESKTOP"
APP="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"

log()  { printf '[v20.0-c3] %s\n' "$*" >&2; }
fail() { printf '[v20.0-c3] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v20.0-c3] cannot judge: %s\n' "$*" >&2; exit 2; }

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

label() { # the text of the element with this id, read from the tree
  "$SMIX" tree --device "$DEVICE" --runner-port "$PORT" --json 2>/dev/null | python3 -c '
import json, sys
want = sys.argv[1]
def walk(n):
    if n.get("identifier") == want:
        print(n.get("text") or n.get("label") or ""); sys.exit(0)
    for c in n.get("children", []): walk(c)
walk(json.load(sys.stdin)["root"])' "$1"
}

is() { # $1 id, $2 expected text (case does not matter: a Button shows capitals)
  local got
  got="$(label "$1")"
  [ "$(printf '%s' "$got" | tr '[:upper:]' '[:lower:]')" = "$2" ] || fail "$3: $1 reads '$got', expected '$2'"
}

open_desktop() {
  adb -s "$DEVICE" shell am start -W -S --windowingMode 5 -n "$APP/.DesktopActivity" >/dev/null 2>&1 \
    || fail "could not start the desktop screen"
  flow open '- extendedWaitUntil: { visible: { id: desk_hover_row }, timeout: 10000 }'
  [ "$(label desk_hover_state)" = "hover out 0" ] \
    || cannot_judge "the desktop screen did not open fresh (hover state: '$(label desk_hover_state)')"
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
adb -s "$DEVICE" shell pm list features 2>/dev/null | grep -q 'android.hardware.type.pc' \
  || cannot_judge "$ALIAS ($DEVICE) is not an Android desktop image"
adb -s "$DEVICE" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"
"$SMIX" runner up "$DEVICE" --platform android --runner-port "$PORT" >"$WORK/up.log" 2>&1 \
  || fail "runner up: $(tail -5 "$WORK/up.log")"
UP=1

open_desktop
log "1. hoverOn the row"
flow hover-row '- hoverOn: { id: desk_hover_row }'
is desk_hover_state "hover in 1" "1"

log "2. hoverOn the button"
flow hover-button '- hoverOn: { id: desk_button }'
is desk_hover_state "hover out 1" "2 (the row the pointer left)"
is desk_button "button in 1 taps 0" "2"

log "3. tapOn after the hover"
flow tap-button '- tapOn: { id: desk_button }'
is desk_button "button out 1 taps 1" "3 (the touch ends the hover)"

log "4. hoverAway"
printf 'appId: %s\n---\n- hoverAway\n' "$APP" >"$WORK/away-from-nothing.yaml"
# raw run: a refusal of this step is what this script judges, not a step it could not judge
"$SMIX" run --device "$DEVICE" --runner-port "$PORT" "$WORK/away-from-nothing.yaml" \
  >"$WORK/away-from-nothing.log" 2>&1 \
  || fail "4: hoverAway with nothing hovered was refused: $(grep -m1 'FAIL' "$WORK/away-from-nothing.log")"
is desk_button "button out 1 taps 1" "4 (nothing was hovered)"
flow hover-again '- hoverOn: { id: desk_button }'
is desk_button "button in 2 taps 1" "4 (hovered again)"
flow away '- hoverAway'
is desk_button "button out 2 taps 1" "4"

log "5. smix hover / smix hover --away"
"$SMIX" hover id:desk_hover_row --device "$DEVICE" --runner-port "$PORT" >"$WORK/cli.log" 2>&1 \
  || fail "5: smix hover: $(tail -3 "$WORK/cli.log")"
is desk_hover_state "hover in 2" "5"
"$SMIX" hover --away --device "$DEVICE" --runner-port "$PORT" >"$WORK/cli-away.log" 2>&1 \
  || fail "5: smix hover --away: $(tail -3 "$WORK/cli-away.log")"
is desk_hover_state "hover out 2" "5 (--away)"

log "6. iOS refuses by name"
# nothing listens on UNUSED_PORT: a refusal that came from a request would
# be a connection error instead
if "$SMIX" hover --away --platform ios --runner-port "$UNUSED_PORT" >"$WORK/ios.log" 2>&1; then
  fail "6: smix hover on iOS succeeded"
fi
grep -q "Android-only" "$WORK/ios.log" || fail "6: the iOS refusal does not say why: $(cat "$WORK/ios.log")"

log "7. setWindowSize there and back"
flow resize \
  '- setWindowSize: 1024x768' \
  '- assertVisible: "window 1024 768 expanded"' \
  '- setWindowSize: { width: 839, height: 768 }' \
  '- assertVisible: "window 839 768 medium"' \
  '- setWindowSize: compact' \
  '- assertVisible: "window 412 800 compact"' \
  '- setWindowSize: 1024x768'
is desk_window "window 1024 768 expanded" "7"

log "8. sizes the display cannot give"
refused() { # $1 name, $2 size, $3 what the refusal must say
  printf 'appId: %s\n---\n- setWindowSize: %s\n' "$APP" "$2" >"$WORK/$1.yaml"
  # raw run: this step's refusal is what is judged
  if "$SMIX" run --device "$DEVICE" --runner-port "$PORT" "$WORK/$1.yaml" >"$WORK/$1.log" 2>&1; then
    fail "8: setWindowSize $2 passed"
  fi
  grep -q "$3" "$WORK/$1.log" || fail "8: the refusal of $2 does not say '$3': $(grep -m2 -i 'fail\|error' "$WORK/$1.log")"
  is desk_window "window 1024 768 expanded" "8 (after $2)"
}
refused too-small 100x100 "220 dp a side"
refused too-large 5000x5000 "does not fit this display"

log "9. smix window-size"
"$SMIX" window-size 839x768 --app "$APP" --device "$DEVICE" --runner-port "$PORT" >"$WORK/cli-size.log" 2>&1 \
  || fail "9: smix window-size: $(tail -3 "$WORK/cli-size.log")"
grep -q "reads back at 839x768 dp" "$WORK/cli-size.log" || fail "9: smix window-size said: $(cat "$WORK/cli-size.log")"
is desk_window "window 839 768 medium" "9"
if "$SMIX" window-size 839x768 --platform ios --runner-port "$UNUSED_PORT" >"$WORK/ios-size.log" 2>&1; then
  fail "9: smix window-size on iOS succeeded"
fi
grep -q "Android-only" "$WORK/ios-size.log" || fail "9: the iOS refusal does not say why: $(cat "$WORK/ios-size.log")"

log "10. a screenshot of the window"
printf 'appId: %s\n---\n- takeScreenshot:\n    path: %s\n    window: true\n' "$APP" "$WORK/window.png" >"$WORK/shot.yaml"
"$SMIX_RUN" --device "$DEVICE" --runner-port "$PORT" "$WORK/shot.yaml" >"$WORK/shot.log" 2>&1 \
  || { tail -10 "$WORK/shot.log" >&2; fail "10: takeScreenshot window failed"; }
bounds="$(adb -s "$DEVICE" shell am stack list | grep "$APP/" | grep -oE 'bounds=\[[0-9]+,[0-9]+\]\[[0-9]+,[0-9]+\]' | head -1)"
[ -n "$bounds" ] || cannot_judge "10: the fixture's task is not in am stack list"
want="$(printf '%s' "$bounds" | python3 -c 'import re,sys; l,t,r,b=map(int,re.findall(r"-?\d+",sys.stdin.read())); print(f"{r-l}x{b-t}")')"
got="$(python3 -c 'import struct,sys; d=open(sys.argv[1],"rb").read(24); w,h=struct.unpack(">II",d[16:24]); print(f"{w}x{h}")' "$WORK/window.png")"
[ "$got" = "$want" ] || fail "10: the window screenshot is $got, the window is $want ($bounds)"

log "11. key combinations"
flow keys '- pressKey: ctrl+s' '- pressKey: ctrl+shift+z'
is desk_keys "keys ctrl+s ctrl+shift+z" "11"

log "12. a drag by dp"
flow drag '- mouseDrag: { from: { id: desk_drag }, by: { x: 96, y: 0 } }'
is desk_drag "drag 96 0 mouse" "12"

log "13. the caption bar's back arrow"
flow back '- tapOn: { id: back_button }' \
  '- assertNotVisible: { id: desk_hover_row }'

log "V20.0-C3-DESKTOP-WINDOW-PASS"
