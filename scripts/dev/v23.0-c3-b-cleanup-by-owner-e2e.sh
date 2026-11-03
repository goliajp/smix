#!/usr/bin/env bash
# v23.0-C3: `smix cleanup --owner` takes down one task's resources and
# leaves another's standing.
#
# A task that ends is meant to leave nothing; four tasks each deleting
# from their own list is how one took a link another still used. Now the
# list is the ledger's:
#   1. task A boots the simulator; task B brings a runner up on it and
#      boots the emulator. `--dry-run` for B lists and changes nothing
#   2. cleanup for B: its runner and its emulator go, A's simulator stays
#      and is reported as A's
#   3. cleanup for A: the simulator goes
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port IOS_PORT
IOS_APP="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
WORK="$(mktemp -d)"
A="c3-task-a-$$"
B="c3-task-b-$$"

log()  { printf '[v23.0-c3-b] %s\n' "$*" >&2; }
fail() { printf '[v23.0-c3-b] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v23.0-c3-b] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" SERIAL=""
cleanup() {
  "$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" >/dev/null 2>&1 || true
  [ -n "$UDID" ] && "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true
  [ -n "$SERIAL" ] && "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

booted() { "$SMIX" status --json | python3 -c '
import json, sys
print(any(r["id"] == sys.argv[1] and r["running"] for r in json.load(sys.stdin)))' "$1"; }
runner_answers() { curl -s -m 3 -o /dev/null -w '%{http_code}' "http://127.0.0.1:$IOS_PORT/health" | grep -q 200; }

UDID="$("$SMIX" sim resolve "$E2E_IOS" 2>/dev/null | tail -1 | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$E2E_IOS'"
case "$(simulator_name "$UDID")" in
  sim-smix-*) ;;
  *) cannot_judge "'$E2E_IOS' resolves to $UDID, which is not one of smix's own simulators" ;;
esac
[ "$(simulator_state "$UDID")" = Booted ] && cannot_judge "$UDID is already on; this needs to boot it"
"$SMIX" sim resolve "$E2E_ANDROID" >/dev/null 2>&1 && cannot_judge "$E2E_ANDROID is already on; this needs to boot it"

log "1. A boots the simulator; B brings a runner up on it and boots the emulator"
with_deadline 300 "$SMIX" --owner "$A" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
SMIX_OWNER="$B" "$SMIX" runner up "$UDID" --bundle "$IOS_APP" --runner-port "$IOS_PORT" >"$WORK/up.log" 2>&1 \
  || fail "runner up: $(tail -5 "$WORK/up.log")"
SMIX_OWNER="$B" with_deadline 300 "$SMIX" sim boot "$E2E_ANDROID" --no-window >"$WORK/aboot.log" 2>&1 \
  || fail "boot: $(tail -3 "$WORK/aboot.log")"
SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tail -1)"
"$SMIX" --owner "$B" cleanup --dry-run >"$WORK/dry.log" 2>&1 || fail "dry run: $(tail -3 "$WORK/dry.log")"
grep -q "$UDID: would close runner" "$WORK/dry.log" || fail "1: the dry run does not list B's runner: $(cat "$WORK/dry.log")"
grep -q "$SERIAL: would close booted" "$WORK/dry.log" || fail "1: the dry run does not list B's emulator: $(cat "$WORK/dry.log")"
runner_answers || fail "1: the dry run took the runner down"
[ "$(booted "$SERIAL")" = True ] || fail "1: the dry run took the emulator down"

log "2. cleanup for B"
"$SMIX" --owner "$B" cleanup >"$WORK/b.log" 2>&1 || fail "cleanup B: $(tail -5 "$WORK/b.log")"
runner_answers && fail "2: B's runner still answers on $IOS_PORT: $(cat "$WORK/b.log")"
[ "$(booted "$SERIAL")" = False ] || fail "2: B's emulator is still running: $(cat "$WORK/b.log")"
[ "$(booted "$UDID")" = True ] || fail "2: A's simulator went with B's cleanup: $(cat "$WORK/b.log")"
grep -q "$UDID: left booted — $A's" "$WORK/b.log" || fail "2: A's simulator is not reported as A's: $(cat "$WORK/b.log")"
SERIAL=""

log "3. cleanup for A"
"$SMIX" --owner "$A" cleanup >"$WORK/a.log" 2>&1 || fail "cleanup A: $(tail -5 "$WORK/a.log")"
[ "$(booted "$UDID")" = False ] || fail "3: A's simulator is still running: $(cat "$WORK/a.log")"
UDID=""

log "V23.0-C3-B-CLEANUP-BY-OWNER-PASS"
