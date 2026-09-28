#!/usr/bin/env bash
# v11.0-C9p: on iOS a tap lands where it was aimed in both landscapes.
#
# A touch is delivered in the device's space, so the runner rotates the
# point it aims at. It read the rotation off the app's frame, and a
# frame only says the app is wide, not which way it was turned: measured
# 2026-09-28 on iOS 27, aimed at (214,56) in an 874x402 app turned
# landscapeLeft, the touch arrived at (660,346) — the opposite corner —
# and the step reported success. landscapeRight landed.
#
# Judged by the app: its own press count, and the point it says it was
# touched at, against the centre of the target it was aimed at. The
# target sits in the top-leading corner because a mirrored tap on a
# centred one still lands. Then a screen that supports only
# landscapeRight is driven with the device turned the other way: the
# device's orientation is not the interface's there, and only the app's
# own reading gets it right.
#
# Three codes, per the contract: 0 judged and right, 1 judged and wrong
# or the setup this run owns did not come up, 2 could not judge.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-devices.sh
source "$ROOT/scripts/lib/e2e-devices.sh"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/judged-run.sh
source "$ROOT/scripts/lib/judged-run.sh"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
UDID="${1:-${SMIX_E2E_UDID:-}}"
APP="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"

log()  { printf '[c9p-landscape] %s\n' "$*" >&2; }
fail() { printf '[c9p-landscape] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c9p-landscape] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

[ -n "$UDID" ] || cannot_judge "usage: $0 <simulator udid> (or SMIX_E2E_UDID)"
[ "$(simulator_state "$UDID")" = Booted ] \
  || cannot_judge "$UDID is not a booted simulator"
[ -d "$FIXTURE" ] || fail "no fixture app — run: bash scripts/dev/build-fixture-app.sh"

WORK="$(mktemp -d)"
WE_UPPED=0
cleanup() {
  if [ "$WE_UPPED" = 1 ]; then
    printf 'appId: %s\n---\n- setOrientation: portrait\n' "$APP" > "$WORK/restore.yaml"
    # raw run: best effort on the way out; a failure to turn back is not this script's verdict
    "$SMIX" run "$WORK/restore.yaml" --device "$UDID" --platform ios --runner-port "$PORT" >/dev/null 2>&1 || true
    local said
    if ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$PORT" 2>&1)"; then
      printf '[c9p-landscape] warning: the runner on %s:%s was not stopped:\n%s\n' \
        "$UDID" "$PORT" "$(printf '%s' "$said" | tail -3)" >&2
    fi
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

# One flow step on this device, by UDID: the device need not be
# registered, so the platform is named.
step() { # step <yaml step lines>
  printf 'appId: %s\n---\n%s\n' "$APP" "$1" > "$WORK/step.yaml"
  "$SMIX_RUN" "$WORK/step.yaml" --device "$UDID" --platform ios --runner-port "$PORT" \
    >"$WORK/step.out" 2>&1 \
    || fail "step '$1' failed: $(tail -3 "$WORK/step.out" | tr '\n' ' ')"
}

# The labels and the target's bounds, read from the runner's own tree.
read_probe() {
  curl -s -m 15 "http://127.0.0.1:$PORT/tree" | python3 -c '
import json, sys
d = json.load(sys.stdin)
out = {}
def walk(n):
    i = n.get("identifier") or ""
    if i in ("rotating-presses", "rotating-last-touch", "landscape-counter"):
        out[i] = n.get("label") or ""
    if i == "rotating-press":
        b = n.get("bounds") or {}
        out[i] = "%d,%d" % (b.get("x", 0) + b.get("w", 0) / 2, b.get("y", 0) + b.get("h", 0) / 2)
    for c in n.get("children") or []:
        walk(c)
walk(d)
print(json.dumps(out))'
}
field() { python3 -c 'import json,sys; print(json.loads(sys.argv[1]).get(sys.argv[2], ""))' "$1" "$2"; }

xcrun simctl install "$UDID" "$FIXTURE" || fail "could not install the fixture"
"$SMIX" runner up "$UDID" --bundle "$APP" --runner-port "$PORT" --force >/dev/null 2>&1 \
  || fail "the runner would not start on $UDID:$PORT"
WE_UPPED=1

step "- setOrientation: portrait
- launchApp: { appId: $APP, stopApp: true }
- tapOn: { id: rotating-enter }"

for o in portrait landscapeLeft landscapeRight; do
  step "- setOrientation: $o"
  before="$(read_probe)"
  aimed="$(field "$before" rotating-press)"
  [ -n "$aimed" ] || cannot_judge "$o: the target is not on the screen"
  n0="$(field "$before" rotating-presses | tr -dc 0-9)"
  step "- tapOn: { id: rotating-press }"
  after="$(read_probe)"
  n1="$(field "$after" rotating-presses | tr -dc 0-9)"
  touched="$(field "$after" rotating-last-touch)"
  python3 - "$aimed" "$touched" <<'PY' || fail "$o: aimed at $aimed, the app was touched at $touched"
import sys
ax, ay = map(int, sys.argv[1].split(","))
try:
    tx, ty = map(int, sys.argv[2].split(","))
except ValueError:
    sys.exit(1)
sys.exit(0 if abs(ax - tx) <= 2 and abs(ay - ty) <= 2 else 1)
PY
  [ "$n1" = $((n0 + 1)) ] || fail "$o: the app counted $n0 then $n1 presses — the tap did not press it"
  log "$o: aimed at $aimed, touched at $touched, pressed"
done

# The device turned one way, a screen that only turns the other.
step "- setOrientation: portrait
- tapOn: { id: rotating-exit }
- tapOn: { id: landscape-enter }
- setOrientation: landscapeRight
- tapOn: { id: landscape-increment }
- tapOn: { id: landscape-increment }"
count="$(field "$(read_probe)" landscape-counter)"
[ "$count" = 2 ] || fail "a landscapeRight-only screen with the device turned landscapeRight counted $count, not 2"
step "- tapOn: { id: landscape-exit }
- setOrientation: portrait"
log "a landscapeRight-only screen, device turned the other way: pressed twice"

log "PASS"
