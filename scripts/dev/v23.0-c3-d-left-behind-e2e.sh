#!/usr/bin/env bash
# v23.0-C3: a device smix booted that nobody holds and no command has
# touched for the idle limit is left behind, and is reclaimed.
#
# A task between two runs holds nothing either, so the time decides. The
# limit is 30 minutes by default; here it is 20 seconds:
#   1. task A boots the simulator; 25 s later task B boots the emulator
#   2. `cleanup --orphans --dry-run` lists A's simulator, idle, and not B's
#      fresh emulator, and closes nothing
#   3. `cleanup --orphans` shuts the simulator down; the emulator stays
#   4. 25 s on, a `sim boot` of the simulator first reclaims the emulator,
#      now idle too, then boots
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
WORK="$(mktemp -d)"
A="c3-task-a-$$"
B="c3-task-b-$$"

log()  { printf '[v23.0-c3-d] %s\n' "$*" >&2; }
fail() { printf '[v23.0-c3-d] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v23.0-c3-d] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" SERIAL=""
cleanup() {
  [ -n "$UDID" ] && "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true
  [ -n "$SERIAL" ] && "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

running() { "$SMIX" status --json | python3 -c '
import json, sys
print(any(r["id"] == sys.argv[1] and r["running"] for r in json.load(sys.stdin)))' "$1"; }

UDID="$("$SMIX" sim resolve "$E2E_IOS" 2>/dev/null | tail -1 | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$E2E_IOS'"
case "$(simulator_name "$UDID")" in sim-smix-*) ;; *) cannot_judge "$UDID is not one of smix's own simulators" ;; esac
[ "$(simulator_state "$UDID")" = Booted ] && cannot_judge "$UDID is already on; this needs to boot it"
"$SMIX" sim resolve "$E2E_ANDROID" >/dev/null 2>&1 && cannot_judge "$E2E_ANDROID is already on; this needs to boot it"
# nothing of anyone else's may be idle enough to be reclaimed by step 3
[ -z "$(SMIX_IDLE_DEVICE_SECS=20 "$SMIX" cleanup --orphans --dry-run)" ] \
  || cannot_judge "another device on this machine is already left behind: $(SMIX_IDLE_DEVICE_SECS=20 "$SMIX" cleanup --orphans --dry-run)"

log "1. A boots the simulator; 25 s later B boots the emulator"
with_deadline 300 "$SMIX" --owner "$A" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
sleep 25
SMIX_OWNER="$B" with_deadline 300 "$SMIX" sim boot "$E2E_ANDROID" --no-window >"$WORK/aboot.log" 2>&1 \
  || fail "boot: $(tail -3 "$WORK/aboot.log")"
SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tail -1)"
# an emulator boots for longer than the limit: B touches it once more, as
# its next command would, so it is fresh when the dry run looks
SMIX_OWNER="$B" "$SMIX" sim boot "$E2E_ANDROID" >"$WORK/touch.log" 2>&1 \
  || fail "touch: $(tail -3 "$WORK/touch.log")"

log "2. the dry run lists A's idle simulator, not B's fresh emulator"
SMIX_IDLE_DEVICE_SECS=20 "$SMIX" cleanup --orphans --dry-run >"$WORK/dry.log" 2>&1 || fail "dry run: $(cat "$WORK/dry.log")"
grep -q "would reclaim $UDID ($A)" "$WORK/dry.log" || fail "2: the simulator is not listed: $(cat "$WORK/dry.log")"
grep -q "$SERIAL" "$WORK/dry.log" && fail "2: the fresh emulator is listed: $(cat "$WORK/dry.log")"
[ "$(running "$UDID")" = True ] || fail "2: the dry run shut the simulator down"

log "3. reclaimed: the simulator goes, the emulator stays"
SMIX_IDLE_DEVICE_SECS=20 "$SMIX" cleanup --orphans >"$WORK/rec.log" 2>&1 || fail "reclaim: $(cat "$WORK/rec.log")"
[ "$(running "$UDID")" = False ] || fail "3: the simulator is still running: $(cat "$WORK/rec.log")"
[ "$(running "$SERIAL")" = True ] || fail "3: the emulator went too: $(cat "$WORK/rec.log")"

log "4. a boot first reclaims what is left behind"
sleep 25
SMIX_IDLE_DEVICE_SECS=20 with_deadline 300 "$SMIX" --owner "$A" sim boot "$UDID" >"$WORK/boot2.log" 2>&1 \
  || fail "4: $(tail -3 "$WORK/boot2.log")"
grep -q "reclaimed $SERIAL ($B)" "$WORK/boot2.log" || fail "4: the boot did not reclaim the emulator: $(cat "$WORK/boot2.log")"
[ "$(running "$SERIAL")" = False ] || fail "4: the emulator is still running"
SERIAL=""
[ "$(running "$UDID")" = True ] || fail "4: the simulator did not boot"

log "V23.0-C3-D-LEFT-BEHIND-PASS"
