#!/usr/bin/env bash
# v13.1-C1: a look at an Android app whose main thread is busy does not
# wait for it.
#
# A flow's look at a probe-carrying app is two reads side by side: the
# probe's tree, and the accessibility tree with the app's own windows left
# hollow. The hollow read still fetched each app window's root to learn
# whose window it was, and a root is answered by the app's main thread, so
# every look waited for the app to be idle — a consumer's home screen, which
# copies video frames, took about six seconds a look.
#
# Measured here while the fixture holds its main thread for four seconds:
# with 13.0.0 the hollow read took 3.5 s. The whole accessibility tree,
# which has to read the app's nodes, is the control — it must be slow, or
# the main thread was not busy and nothing below was measured.
#
# There is no SKIP path: a gate that cannot judge must be red.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
ALIAS="${SMIX_V131_C1_ANDROID:-$E2E_ANDROID}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
# A look that waits for the main thread waits most of the four seconds.
QUICK=1.0
SLOW=2.0

log()  { printf '[v13.1-c1] %s\n' "$*" >&2; }
fail() { printf '[v13.1-c1] FAIL: %s\n' "$*" >&2; exit 1; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0 WORK=""
cleanup() {
  [ -n "$WORK" ] && rm -rf "$WORK"
  if [ "$WE_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
      >/dev/null 2>&1 || printf '[v13.1-c1] warning: the runner was not stopped\n' >&2
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

WORK="$(mktemp -d)"
# What a flow asks for: the app named on the request.
read_route() {
  curl -s -m 30 -H "App-Bundle-Id: $APPID" -o "$WORK/$2.json" -w '%{time_total}' \
    "http://localhost:$PORT$1"
}
faster_than() { python3 -c 'import sys; sys.exit(0 if float(sys.argv[1]) < float(sys.argv[2]) else 1)' "$1" "$2"; }

adb -s "$SERIAL" shell am start -W -S -n "$APPID/.CodeActivity" >/dev/null 2>&1 \
  || fail "could not start the code screen"
busy_y=""
for _ in $(seq 1 40); do
  read_route "/probe/tree" probe >/dev/null || true
  busy_y="$(python3 - "$WORK/probe.json" <<'PY'
import json, sys
def walk(n):
    if n.get("resourceId") == "fixture_code_busy":
        left, top, right, bottom = n.get("visibleBounds") or n["bounds"]
        return (top + bottom) // 2
    for c in n.get("children") or []:
        r = walk(c)
        if r is not None:
            return r
try:
    doc = json.load(open(sys.argv[1]))
except Exception:
    sys.exit(0)
for r in doc.get("roots") or []:
    y = walk(r)
    if y is not None:
        print(y)
        break
PY
)"
  [ -n "$busy_y" ] && break
  sleep 0.5
done
[ -n "$busy_y" ] || fail "the probe never reported the button that holds the main thread"

idle="$(read_route "/tree?hollow=$APPID" idle)"
log "idle: the hollow read took ${idle}s"

# Hold the app's main thread, then look while it is held.
adb -s "$SERIAL" shell input tap 540 "$busy_y"
sleep 0.5
hollow="$(read_route "/tree?hollow=$APPID" hollow)"
probe="$(read_route "/probe/tree" probe-busy)"
sleep 4.5

adb -s "$SERIAL" shell input tap 540 "$busy_y"
sleep 0.5
control="$(read_route "/tree" whole)"
sleep 4.5
log "busy: hollow ${hollow}s, probe ${probe}s; the whole tree, as the control, ${control}s"

faster_than "$SLOW" "$control" \
  || fail "the control read the whole tree in ${control}s — the app's main thread was not held, so nothing was measured"
faster_than "$hollow" "$QUICK" \
  || fail "the hollow read waited for the app's main thread: ${hollow}s"
faster_than "$probe" "$QUICK" \
  || fail "the probe's tree waited for the app's main thread: ${probe}s"

# Fast is worth nothing if the window it skipped is no longer reported as the
# app's: the host finds the app's place in the stack, its focus and where it
# takes touches by this entry.
python3 - "$WORK/hollow.json" "$APPID" <<'PY' || fail "the hollow read no longer reports the app's window as the app's"
import json, sys
doc = json.load(open(sys.argv[1]))
app = sys.argv[2]
wins = [c.get("window") or {} for c in doc.get("children") or []]
mine = [w for w in wins if w.get("kind") == "application" and w.get("package") == app]
others = [w for w in wins if w.get("package") not in (None, app)]
assert mine, f"no application window of {app} among {wins}"
assert any(w.get("focused") for w in mine), f"the app's window is not the focused one: {mine}"
assert all("touchable" in w and "layer" in w for w in mine), f"layer or touchable missing: {mine}"
assert others, f"no other window was read at all: {wins}"
PY
log "the app's window is reported as the app's, focused, with its layer and touchable area"

log "V13.1-C1-E2E-PASS on $SERIAL"
