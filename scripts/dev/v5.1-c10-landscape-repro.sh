#!/usr/bin/env bash
# v5.1-C10: does a tap that reports success actually move anything?
#
# A consumer drove a landscape screen and watched `smix tap` answer
# `landed inside: <the button they aimed at>` while a pixel-by-pixel diff
# of before and after showed nothing had changed at all. Portrait, the
# same calls worked all day.
#
# Reading the runner explains where that sentence comes from and why it
# is not evidence. `HitChain.at()` walks the snapshot and returns every
# element whose `frame.contains(point)` — pure geometry, in whatever
# space the snapshot is describing. The touch itself goes through
# `app.coordinate(withNormalizedOffset:)`, normalised against
# `XCUIApplication`'s frame. Two sides, two spaces, and nothing checks
# that they are the same one. `landed inside` proves the aim, never the
# effect.
#
# This script does not fix that. It reproduces it, and it is built so
# that its red means something:
#
#   --orientation portrait   the control. Must pass. If a tap on a
#                            portrait button does not move pixels here,
#                            the instrument is broken and the landscape
#                            run says nothing about the app.
#   --orientation landscape  the subject. Expected to fail today, and to
#                            fail with a sentence carrying both halves —
#                            what smix reported, and what actually moved.
#
# Three readings per tap, because any one of them can lie on its own:
#   1. what smix said        (`landed inside: …`)
#   2. pixels changed        (ImageChops.difference().getbbox())
#   3. the counter's text    (through /tree — an independent witness to
#                            the pixels, in case the screenshots are of
#                            the wrong thing)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
UDID="${SMIX_LANDSCAPE_E2E_UDID:-}"
. "$ROOT/scripts/lib/gate-port.sh"
PORT="${SMIX_LANDSCAPE_E2E_PORT:-$SMIX_RUNNER_PORT}"
BUNDLE="$(grep -m1 '^BUNDLE_ID=' "$ROOT/scripts/dev/build-fixture-app.sh" | cut -d'"' -f2)"
WORK="$(mktemp -d)"

ORIENTATION=""
while [ $# -gt 0 ]; do
  case "$1" in
    --orientation) ORIENTATION="${2:-}"; shift 2 ;;
    *) printf 'unknown argument: %s\n' "$1" >&2; exit 2 ;;
  esac
done
case "$ORIENTATION" in
  portrait|landscape) ;;
  *) printf 'usage: %s --orientation portrait|landscape\n' "$0" >&2; exit 2 ;;
esac

TAG="c10-$ORIENTATION"
log()  { printf '[%s] %s\n' "$TAG" "$*" >&2; }
step() { printf '[%s] --- %s\n' "$TAG" "$*" >&2; }
fail() { printf '[%s] FAIL: %s\n' "$TAG" "$*" >&2; exit 1; }
skip() { printf '[%s] SKIP: %s\n' "$TAG" "$*" >&2; exit 0; }

started_runner=0
cleanup() {
  if [ "$started_runner" = 1 ]; then
    if ! "$SMIX" runner down --runner-port "$PORT" >"$WORK/down.log" 2>&1; then
      printf '[%s] the runner on %s was not stopped:\n' "$TAG" "$PORT" >&2
      tail -3 "$WORK/down.log" >&2
    fi
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

# Only for calls whose exit code decides nothing — it pipes, and a pipe
# with `|| true` on the end cannot fail. The first draft used it for the
# assertions too, so `smix tap id:landscape-enter || fail` reported a
# landscape screen that did not exist: the verdict had been handed to
# `grep`, which was perfectly happy. Anything that decides calls "$SMIX"
# directly.
smix_quiet() { "$SMIX" "$@" 2>&1 | grep -v '^kevy:' || true; }

[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
command -v xcrun >/dev/null 2>&1 || skip "no xcrun — this needs a simulator"
python3 -c 'import PIL' 2>/dev/null || fail "Pillow is the pixel judge here — python3 -m pip install pillow"

if [ -z "$UDID" ]; then
  # Not "the first sim whose name matches": that proxy handed a release
  # gate somebody else's device once already. pick-dev-sim asks the
  # machine ledger whether smix booted it.
  UDID="$(bash "$ROOT/scripts/dev/pick-dev-sim.sh" 2>"$WORK/pick.log")" || {
    cat "$WORK/pick.log" >&2
    skip "no dev sim this machine's ledger says smix booted"
  }
fi
[ -n "$UDID" ] || skip "pick-dev-sim named no device"
log "device $UDID, runner port $PORT"

step "1. build and install the fixture"
bash "$ROOT/scripts/dev/build-fixture-app.sh" > "$WORK/build.log" 2>&1 \
  || { tail -20 "$WORK/build.log"; fail "the fixture app did not build"; }
APP="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
[ -d "$APP" ] || fail "no fixture app at $APP after a successful build"
"$SMIX" sim boot "$UDID" >/dev/null 2>&1 || fail "could not boot $UDID"
"$SMIX" sim install "$UDID" "$APP" >/dev/null 2>&1 || fail "could not install the fixture"

step "2. runner up"
"$SMIX" runner up "$UDID" --bundle "$BUNDLE" --runner-port "$PORT" > "$WORK/up.log" 2>&1 \
  || { tail -20 "$WORK/up.log"; fail "runner did not come up"; }
started_runner=1

PREFIX="$ORIENTATION"

read_counter() {
  # Not the `smix` pipe helper: this value decides the verdict, and a
  # stray diagnostic line folded into stdout would read as a broken tree.
  "$SMIX" tree --port "$PORT" --json 2>/dev/null | grep -v '^kevy:' | python3 -c "
import json,sys
want=sys.argv[1]
def walk(n):
    if (n.get('identifier') or '') == want:
        print(n.get('label') or ''); raise SystemExit
    for c in n.get('children', []): walk(c)
t=json.load(sys.stdin); walk(t if isinstance(t,dict) else t[0])
" "$PREFIX-counter"
}

shot() { xcrun simctl io "$UDID" screenshot "$1" >/dev/null 2>&1; }

# One tap, three readings, one sentence.
#
# The sentence carries all three because each can lie alone: smix's own
# report is the thing under suspicion, a pixel diff cannot tell "nothing
# happened" from "I photographed the wrong screen", and the counter is
# read back through the same runner whose aim is in question. Together
# they are hard to fool in the same direction.
tap_and_judge() {
  local id="$1" before after reported bbox
  before="$(read_counter)"
  shot "$WORK/before.png"
  reported="$("$SMIX" tap "id:$id" --port "$PORT" 2>&1 | grep -v '^kevy:' | tr '\n' ' ' | sed 's/  */ /g')"
  sleep 1
  shot "$WORK/after.png"
  after="$(read_counter)"
  bbox="$(python3 -c "
from PIL import Image, ImageChops
a=Image.open('$WORK/before.png').convert('RGB')
b=Image.open('$WORK/after.png').convert('RGB')
print(ImageChops.difference(a,b).getbbox() or 'None')
")"
  printf '[%s] %s\n' "$TAG" "reported: ${reported} | pixels changed: ${bbox} | counter: ${before:-<none>} → ${after:-<none>}" >&2
  [ "$bbox" != "None" ] || return 1
  [ "$before" != "$after" ] || return 1
  return 0
}

step "3. open the counter screen"
"$SMIX" tap "id:$PREFIX-enter" --port "$PORT" > "$WORK/enter.log" 2>&1 \
  || { cat "$WORK/enter.log"; fail "could not reach the $ORIENTATION counter screen"; }
# `wait-for`, not `find`: presenting the controller and the rotation
# that follows it are not instantaneous, and a one-shot probe would make
# this script's verdict depend on how fast the machine is.
"$SMIX" wait-for "id:$PREFIX-counter" --port "$PORT" --timeout 10 > "$WORK/there.log" 2>&1 \
  || { cat "$WORK/there.log"; fail "the $ORIENTATION counter screen did not come up"; }

# The space the tree is describing, recorded either way — it is the
# number the next checkpoint compares against, and it costs nothing to
# take here.
ROOT_BOUNDS="$("$SMIX" tree --port "$PORT" --json 2>/dev/null | grep -v '^kevy:' | python3 -c "
import json,sys
t=json.load(sys.stdin); n=t if isinstance(t,dict) else t[0]
b=n.get('bounds') or {}
print(f\"{b.get('w')}x{b.get('h')}\")
")"
log "tree root: $ROOT_BOUNDS"

step "4. the large target"
large_ok=0
tap_and_judge "$PREFIX-increment" && large_ok=1

step "5. the small target, 44x40 in the corner"
# Tapped after the large one so a failure here cannot be read as "the
# screen was never interactive": if the large target moved pixels and
# this one does not, the difference is the target, not the screen.
small_ok=0
tap_and_judge "$PREFIX-exit" && small_ok=1

if [ "$large_ok" = 1 ] && [ "$small_ok" = 1 ]; then
  log "both targets moved the screen"
  exit 0
fi

fail "a tap smix reported as landed changed nothing on screen \
(large target: $([ $large_ok = 1 ] && echo moved || echo 'nothing moved'), \
small target: $([ $small_ok = 1 ] && echo moved || echo 'nothing moved'))"
