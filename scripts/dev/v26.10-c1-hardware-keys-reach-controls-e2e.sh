#!/usr/bin/env bash
# v26.10-C1: a simulator's hardware keyboard walks controls and activates them.
#
# A consumer had to know whether their buttons can be reached and pressed
# with a keyboard alone. `press-key tab` types a tab character, and keys
# sent through the simulator's own keyboard looked dead after the first:
# a runner was up, and a UI-test session keeps Full Keyboard Access from
# taking any key after the one that shows its outline.
#
# The fixture's keyboard screen has a button, a plain view with a tap
# gesture, and another button, each counting what it received. With no
# runner up: Tab, up to the top, then Space on each on the way down, and
# the app's own counts say one each — read afterwards through a runner that
# does not relaunch the app. With that runner up, pressing is refused.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
IOS_ALIAS="${SMIX_V2610_C1_IOS:-$E2E_IOS}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port IOS_PORT
APPID="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
PROJECT="$ROOT/swift-bridge/SmixRunner.xcodeproj"
WORK="$(mktemp -d)"

log()  { printf '[v26.10-c1] %s\n' "$*" >&2; }
fail() { printf '[v26.10-c1] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v26.10-c1] SKIP: %s\n' "$*" >&2; exit 2; }

UDID="" WE_BOOTED=0 WE_UPPED=0 WAS=""
cleanup() {
  if [ "$WE_UPPED" = 1 ]; then
    "$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" >/dev/null 2>&1 \
      || printf '[v26.10-c1] warning: the runner was not stopped\n' >&2
  fi
  if [ "$WAS" = off ]; then "$SMIX" sim keyboard "$UDID" --full-access off >/dev/null 2>&1 || true; fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "${WORK:?}"
}
trap cleanup EXIT

command -v xcrun >/dev/null 2>&1 || cannot_judge "no xcrun"
UDID="$("$SMIX" sim resolve "$IOS_ALIAS" 2>/dev/null | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$IOS_ALIAS'"
[ -d "$FIXTURE" ] || fail "no iOS fixture at $FIXTURE (bash scripts/dev/build-fixture-app.sh)"

if [ "$(simulator_state "$UDID")" != Booted ]; then
  "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi
case "$("$SMIX" sim keyboard "$UDID")" in
  *"access on") : ;;
  *)
    [ "$WE_BOOTED" = 1 ] || cannot_judge "$UDID was already booted with full keyboard access off; switching it on needs a reboot"
    WAS=off
    "$SMIX" sim keyboard "$UDID" --full-access on --reboot >"$WORK/on.log" 2>&1 || fail "full access: $(tail -3 "$WORK/on.log")"
    ;;
esac

"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
xcrun simctl terminate "$UDID" "$APPID" >/dev/null 2>&1 || true
xcrun simctl launch "$UDID" "$APPID" -keyboard-focus >"$WORK/launch.log" 2>&1 || fail "launch: $(tail -3 "$WORK/launch.log")"
# nothing can be asked while no runner is up, so the screen is given time
sleep 5

"$SMIX" sim keyboard "$UDID" tab up up space down space down space shift+tab >"$WORK/press.log" 2>&1 \
  || fail "press: $(tail -5 "$WORK/press.log")"

SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" runner up "$UDID" --bundle "$APPID" --no-launch --runner-port "$IOS_PORT" \
  --runner-project "$PROJECT" >"$WORK/up.log" 2>&1 || fail "runner up: $(tail -5 "$WORK/up.log")"
WE_UPPED=1
got="$("$SMIX" tree --json --port "$IOS_PORT" --device "$UDID" --platform ios 2>/dev/null | python3 -c '
import json, sys
def walk(n):
    if n.get("identifier") == "kf_result":
        print(n.get("label") or "")
        return True
    return any(walk(c) for c in n.get("children", []))
walk(json.load(sys.stdin)["root"]) or print("no kf_result on screen")')"
[ "$got" = "first 1 shared 1 last 1" ] \
  || fail "Space on each control left the app saying '$got', not 'first 1 shared 1 last 1'"
log "keys: the app counted one activation of each, the plain view with a tap gesture among them"

rc=0
"$SMIX" sim keyboard "$UDID" tab >"$WORK/refused.log" 2>&1 || rc=$?
[ "$rc" -ne 0 ] || fail "a key press beside a runner was reported as pressed"
grep -q "runner up on port $IOS_PORT" "$WORK/refused.log" || fail "the refusal does not name the runner: $(cat "$WORK/refused.log")"
log "beside a runner: refused, naming it"

log "V26.10-C1-E2E-PASS on $UDID"
