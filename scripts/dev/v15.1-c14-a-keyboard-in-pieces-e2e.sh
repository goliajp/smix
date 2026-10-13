#!/usr/bin/env bash
# v15.1-C14: a keyboard that takes touches in pieces covers only the pieces.
#
# After a stylus stroke in a field, Gboard switches to handwriting: its
# window takes touches on a toolbar beside the field and on a strip at the
# bottom, and the rectangle around both spans the screen between them. A
# touch between the pieces goes to the app. smix took that rectangle for
# the keyboard's: the host refused a tap on a button between the pieces as
# under the keyboard, and the runner, picking the window by its bounds,
# read the touch as delivered outside every window. The script puts the
# keyboard in that state with a stylus stroke on the fixture's field, shows
# the precondition from the runner's own tree (two pieces, the button
# inside the rectangle around them and in neither piece), then judges:
#   - tapOn the button passes, and the code screen it opens is in front
# A finger tap on the field afterwards puts the keyboard back.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
ALIAS="${SMIX_C14_ANDROID:-$E2E_ANDROID}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"

log()  { printf '[v15.1-c14] %s\n' "$*" >&2; }
fail() { printf '[v15.1-c14] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v15.1-c14] cannot judge: %s\n' "$*" >&2; exit 2; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0 WORK="" FIELD_TAP=""
cleanup() {
  if [ -n "$FIELD_TAP" ]; then
    # a finger on the field takes the keyboard out of handwriting
    adb -s "$SERIAL" shell am start -W -S -n "$APPID/.MainActivity" >/dev/null 2>&1 || true
    adb -s "$SERIAL" shell input tap $FIELD_TAP >/dev/null 2>&1 || true
    adb -s "$SERIAL" shell input keyevent 111 >/dev/null 2>&1 || true
  fi
  [ -n "$WORK" ] && rm -rf "$WORK"
  if [ "$WE_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
      >/dev/null || printf '[v15.1-c14] warning: the runner was not stopped\n' >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || fail "no adb on PATH — this judges an Android runner and cannot"
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
  *) fail "$ALIAS resolves to '$SERIAL', which is not an emulator — refusing" ;;
esac
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
WORK="$(mktemp -d)"

tree() { "$SMIX" tree --device "$SERIAL" --runner-port "$PORT" --json >"$WORK/tree.json" 2>"$WORK/tree.err"; }

# bounds <id> — "left top right bottom" of the element with that id, nothing when absent
bounds() {
  python3 - "$WORK/tree.json" "$1" <<'PY'
import json, sys
stack = [json.load(open(sys.argv[1]))["root"]]
while stack:
    n = stack.pop()
    if n.get("identifier") == sys.argv[2]:
        b = n["bounds"]
        print(int(b["x"]), int(b["y"]), int(b["x"] + b["w"]), int(b["y"] + b["h"]))
        raise SystemExit
    stack.extend(n.get("children") or [])
PY
}

adb -s "$SERIAL" shell am start -W -S -n "$APPID/.MainActivity" >/dev/null 2>&1 \
  || fail "could not start the fixture's main screen"
tree || fail "tree: $(tail -3 "$WORK/tree.err")"
read -r fl ft fr fb <<<"$(bounds fixture_input)"
[ -n "${fb:-}" ] || fail "the fixture's field is not in the tree"
# below the status bar, which an edge-to-edge window draws over the field's top
y=$(( fb - 17 ))
FIELD_TAP="$(( (fl + fr) / 2 )) $y"
# the precondition, from the runner's own tree: the keyboard in pieces, and
# the button in the rectangle around them and in neither piece. A stroke
# right after the screen starts is sometimes taken as nothing, so it is
# sent again, up to four times.
gap=""
for _ in 1 2 3 4; do
  adb -s "$SERIAL" shell input stylus swipe $(( fl + (fr - fl) / 5 )) "$y" $(( fl + (fr - fl) * 4 / 5 )) "$y" 400 \
    || fail "the stylus stroke was not sent"
  for _ in $(seq 1 12); do
    tree || fail "tree: $(tail -3 "$WORK/tree.err")"
    gap="$(python3 - "$WORK/tree.json" <<'PY'
import json, sys
root = json.load(open(sys.argv[1]))["root"]
ime = [c["window"] for c in root["children"] if (c.get("window") or {}).get("kind") == "inputMethod"]
stack, button = [root], None
while stack:
    n = stack.pop()
    if n.get("identifier") == "fixture_open_code":
        button = n["bounds"]
    stack.extend(n.get("children") or [])
if not ime or button is None or len(ime[0].get("touchableRects") or []) < 2:
    raise SystemExit
x, y = button["x"] + button["w"] / 2, button["y"] + button["h"] / 2
inside = lambda r: r["x"] <= x < r["x"] + r["w"] and r["y"] <= y < r["y"] + r["h"]
w = ime[0]
if inside(w["touchable"]) and not any(inside(r) for r in w["touchableRects"]):
    print(len(w["touchableRects"]), w["touchable"], w["touchableRects"])
PY
  )"
    [ -n "$gap" ] && break 2
    sleep 0.2
  done
done
[ -n "$gap" ] || cannot_judge "the keyboard did not come up in pieces around the code button after a stylus stroke (no handwriting on this keyboard?)"
log "the keyboard takes touches in pieces, the code button between them: $gap"

printf 'appId: %s\n---\n- tapOn: { id: fixture_open_code }\n' "$APPID" >"$WORK/tap.yaml"
rc=0
SMIX_RUNNER_PORT="$PORT" "$SMIX_RUN" --device "$SERIAL" "$WORK/tap.yaml" >"$WORK/tap.log" 2>&1 || rc=$?
[ "$rc" = 0 ] || { grep -v '^    - ' "$WORK/tap.log" | tail -8 >&2; fail "tapOn the button between the keyboard's pieces exited $rc"; }
opened=""
for _ in $(seq 1 25); do
  tree || fail "tree: $(tail -3 "$WORK/tree.err")"
  opened="$(bounds fixture_code_result)"
  [ -n "$opened" ] && break
  sleep 0.2
done
[ -n "$opened" ] || fail "tapOn passed and the code screen is not in front"
log "tapOn the button between the pieces passed, and the code screen opened"
log "V15.1-C14-KEYBOARD-IN-PIECES-E2E-PASS"
