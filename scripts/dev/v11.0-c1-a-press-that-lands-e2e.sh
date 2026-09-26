#!/usr/bin/env bash
# v11.0-C1: a tap on a dialog's button presses it — or says it did not.
#
# A consumer tapped an Android dialog's confirm button three ways
# (`text:Delete`, `text:DELETE`, `id:button1`), smix printed `tapped`
# every time, and the button's listener never ran. The touch had landed
# below the dialog and dismissed it; the next step saw the dialog gone
# and read that as the action having happened.
#
# The cause was two denominators. The host turned the button's centre
# into a share of the accessibility tree's ROOT, and the runner turned
# that share back into pixels with the DISPLAY. The root was the union
# of whatever windows could be read, and with gesture navigation and a
# dialog in front nothing reached the bottom of the screen: measured on
# this fixture, root 1080x1396 on a 1080x2340 display, the centre
# (874.5, 1266.5) sent as ny=0.907 and pressed at y=2122, below a dialog
# that ends at 1341. With three-button navigation the navigation bar is
# a window of its own touching the bottom, so the union happened to be
# the display and the same tap landed. That is why this runs both.
#
# A second half hid behind the first: the semantics probe only saw
# Compose roots, so an app's own native dialog was not in the tree a
# flow reads at all — `smix find` over the accessibility tree said it
# was there and `tapOn` over the probe's said it was not.
#
# Every judgement here reads something smix did not write. The fixture's
# dialog counts the confirmations its listener received; the system's
# uninstall dialog is judged by whether the package is still installed.
# A dismissed dialog and a confirmed one are both gone, so "the dialog
# went away" is not evidence of anything — and it was what let this
# defect read as a pass.
#
# Three codes, per the contract: 0 judged and right, 1 judged and wrong
# or the setup this run owns did not come up, 2 could not judge.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/e2e-devices.sh"
ALIAS="${SMIX_C1_ANDROID:-$E2E_ANDROID}"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"

log()  { printf '[c1-press] %s\n' "$*" >&2; }
fail() { printf '[c1-press] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c1-press] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

SERIAL="" WE_UPPED=0 WE_BOOTED=0 NAV_BEFORE=""
cleanup() {
  # The navigation mode first: it is a setting of a device this script
  # borrowed, and it outlives the run.
  if [ -n "$SERIAL" ] && [ -n "$NAV_BEFORE" ]; then
    set_nav "$NAV_BEFORE" || \
      printf '[c1-press] warning: navigation mode NOT restored to %s on %s\n' "$NAV_BEFORE" "$SERIAL" >&2
  fi
  # The a11y leg uninstalls the fixture on purpose; put it back even
  # when the run failed in between.
  if [ -n "$SERIAL" ] && ! adb -s "$SERIAL" shell pm path "$APPID" 2>/dev/null | grep -q package:; then
    adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || \
      printf '[c1-press] warning: the fixture was NOT reinstalled on %s\n' "$SERIAL" >&2
  fi
  if [ "$WE_UPPED" = 1 ]; then
    local said
    if ! said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" 2>&1)"; then
      printf '[c1-press] warning: the runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
    fi
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
}
trap cleanup EXIT

# navigation_mode: 0 three-button, 1 two-button, 2 gesture. Setting it
# goes through the overlay that owns it; reading it back is the check.
nav_overlay() {
  case "$1" in
    0) echo com.android.internal.systemui.navbar.threebutton ;;
    1) echo com.android.internal.systemui.navbar.twobutton ;;
    2) echo com.android.internal.systemui.navbar.gestural ;;
    *) return 1 ;;
  esac
}
set_nav() {
  local overlay
  overlay="$(nav_overlay "$1")" || return 1
  adb -s "$SERIAL" shell cmd overlay enable-exclusive --category "$overlay" >/dev/null 2>&1 || return 1
  local i
  for i in 1 2 3 4 5 6 7 8 9 10; do
    [ "$(adb -s "$SERIAL" shell settings get secure navigation_mode 2>/dev/null | tr -d '\r')" = "$1" ] && { sleep 3; return 0; }
    sleep 1
  done
  return 1
}

command -v adb >/dev/null 2>&1 || cannot_judge "no adb on PATH"
SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)"
[ -n "$SERIAL" ] || cannot_judge "no device registered as $ALIAS"
case "$SERIAL" in
  emulator-*) : ;;
  # It uninstalls an app and changes the navigation mode. A registered
  # phone belongs to somebody (§9 #1).
  *) cannot_judge "$ALIAS resolves to $SERIAL, which is not an emulator — refusing" ;;
esac
if ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  log "booting $SERIAL"
  "$SMIX" sim boot "$SERIAL" >/dev/null 2>&1 || fail "could not boot $SERIAL"
  WE_BOOTED=1
  adb -s "$SERIAL" wait-for-device
fi
[ -f "$APK" ] || fail "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"
NAV_BEFORE="$(adb -s "$SERIAL" shell settings get secure navigation_mode 2>/dev/null | tr -d '\r')"
nav_overlay "$NAV_BEFORE" >/dev/null || cannot_judge "navigation_mode reads '$NAV_BEFORE', which this script does not know how to put back"

adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"
if ! curl -s -m 5 "http://localhost:$PORT/health" >/dev/null 2>&1; then
  "$SMIX" runner up "$SERIAL" --platform android --runner-port "$PORT" >/dev/null 2>&1 \
    || fail "the runner would not start on $SERIAL:$PORT"
  WE_UPPED=1
fi

# What the fixture's own listener has counted, read off the screen by
# the accessibility reader — a reading of the app, not of smix's tap.
confirmed_count() {
  local tree
  tree="$("$SMIX" tree --device "$SERIAL" --port "$PORT" --reader a11y --json 2>/dev/null)" \
    || return 1
  TREE_JSON="$tree" python3 - <<'PY'
import json, os, re, sys
d = json.loads(os.environ["TREE_JSON"])
def walk(n):
    if (n.get("identifier") or "").split("/")[-1] == "native_dialog_count":
        m = re.search(r"(\d+)", n.get("text") or n.get("label") or "")
        if m:
            print(m.group(1)); sys.exit(0)
    for c in n.get("children", []):
        walk(c)
walk(d["root"])
sys.exit(1)
PY
}

# `$SMIX tap`, with its words and its exit code both kept: the defect was
# a tap that printed success, so what it said is part of the evidence.
tap() { # $1 selector → sets TAP_RC, TAP_SAID
  TAP_RC=0
  TAP_SAID="$("$SMIX" tap "$1" --device "$SERIAL" --port "$PORT" 2>&1)" || TAP_RC=$?
}

# The press happened (the app says so); smix must say the same thing.
# Exit 0, and no "not verified": that line is how the old answer to every
# Android tap read, pressed or not.
judged_as_landed() { # $1 mode name, $2 what was pressed
  [ "$TAP_RC" = 0 ] || fail "$1: $2 was pressed and smix reported a failure (exit $TAP_RC): $TAP_SAID"
  case "$TAP_SAID" in
    *"not verified"*) fail "$1: $2 was pressed and smix could not say so: $TAP_SAID" ;;
  esac
}

display_size() {
  adb -s "$SERIAL" shell wm size 2>/dev/null | tr -d '\r' | sed -n 's/^Physical size: //p' | tail -1
}

one_mode() { # $1 navigation_mode, $2 name
  local mode="$1" name="$2" before after tree probe
  set_nav "$mode" || fail "$name: could not set navigation_mode=$mode"
  log "--- $name navigation"

  # 1. The fixture's own native dialog, through the reader a flow uses.
  adb -s "$SERIAL" shell am start -W -n "$APPID/.NativeDialogActivity" >/dev/null 2>&1 \
    || fail "$name: could not start the dialog screen"
  sleep 2
  before="$(confirmed_count)" || fail "$name: could not read the fixture's count before the tap"
  tap "id:native_dialog_open"
  [ "$TAP_RC" = 0 ] || fail "$name: could not open the dialog: $TAP_SAID"
  sleep 2

  # The invariant, read while the dialog is up — the only moment it was
  # ever false. Both readers' rectangles against the display.
  local display; display="$(display_size)"
  tree="$(curl -s -m 10 "http://localhost:$PORT/tree")"
  probe="$(curl -s -m 10 "http://localhost:$PORT/probe/tree?app=$APPID")"
  local invariant_rc=0
  TREE_JSON="$tree" PROBE_JSON="$probe" DISPLAY_WH="$display" NAME="$name" python3 - <<'PY' || invariant_rc=$?
import json, os, sys
w, h = (int(x) for x in os.environ["DISPLAY_WH"].split("x"))
t = json.loads(os.environ["TREE_JSON"])["bounds"]
p = json.loads(os.environ["PROBE_JSON"])
ps = p.get("screen") if isinstance(p, dict) else None
name = os.environ["NAME"]
ok = True
if (t["w"], t["h"]) != (w, h):
    print(f"[c1-press] FAIL: {name}: the accessibility tree's root is {t['w']}x{t['h']} and the display is {w}x{h} — a tap normalised against one and pressed against the other lands in the wrong place", file=sys.stderr)
    ok = False
if ps is None or tuple(ps) != (w, h):
    print(f"[c1-press] FAIL: {name}: the probe tree's screen is {ps} and the display is {w}x{h}", file=sys.stderr)
    ok = False
def has_button1(n):
    if (n.get("testTag") or n.get("resourceId") or "").split("/")[-1] == "button1":
        return True
    return any(has_button1(c) for c in n.get("children", []))
roots = p.get("roots", []) if isinstance(p, dict) else []
seen = any(has_button1(r) for r in roots)
print(f"[c1-press]   {name}: root-is-display={'yes' if (t['w'], t['h']) == (w, h) else 'no'} probe-sees-dialog={'yes' if seen else 'no'}", file=sys.stderr)
if not seen:
    print(f"[c1-press] FAIL: {name}: the app's own dialog is not in the probe's tree", file=sys.stderr)
    ok = False
sys.exit(0 if ok else 1)
PY

  tap "id:button1"
  sleep 2
  after="$(confirmed_count)" || fail "$name: could not read the fixture's count after the tap"
  if [ "$after" != $((before + 1)) ]; then
    fail "$name: the fixture's listener counted $before then $after — the confirm was not pressed. smix said (exit $TAP_RC): $TAP_SAID"
  fi
  judged_as_landed "$name" "the fixture's confirm"
  log "  $name: count=$((after - before)) (the app's own listener)"
  [ "$invariant_rc" = 0 ] || fail "$name: the confirm landed, and the rectangles above still disagree"

  # 2. A dialog that is not the app's: the system's uninstall
  #    confirmation, which has no probe. Judged by the package manager.
  adb -s "$SERIAL" shell am start -a android.intent.action.DELETE -d "package:$APPID" >/dev/null 2>&1 \
    || fail "$name: could not raise the uninstall dialog"
  sleep 3
  tap "id:button1"
  sleep 3
  if adb -s "$SERIAL" shell pm path "$APPID" 2>/dev/null | grep -q package:; then
    fail "$name: the uninstall dialog's confirm was not pressed — $APPID is still installed. smix said (exit $TAP_RC): $TAP_SAID"
  fi
  judged_as_landed "$name" "the system dialog's confirm"
  log "  $name: system-dialog-confirmed=yes (the package is gone)"
  adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || fail "$name: could not reinstall the fixture"
}

one_mode 2 gesture
one_mode 0 three-button

log "C1-PRESS-E2E-PASS on $SERIAL (count=1 in both navigation modes, probe-sees-dialog=yes, the system dialog confirmed in both)"
