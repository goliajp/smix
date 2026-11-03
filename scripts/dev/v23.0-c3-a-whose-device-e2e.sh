#!/usr/bin/env bash
# v23.0-C3: what a task boots or brings up is recorded as that task's, and
# `smix status` shows it for the whole machine.
#
# Several tasks share one machine; the ledger named only the process that
# last held a device, which changes with every command. Now:
#   1. task A boots the iOS simulator (`--owner`), task B the Android
#      emulator (`SMIX_OWNER`): status names each device's owner, and says
#      smix booted both
#   2. task B brings a runner up on A's simulator: the runner is B's, the
#      boot stays A's
#   3. both are shut down: status no longer names an owner for either
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

log()  { printf '[v23.0-c3-a] %s\n' "$*" >&2; }
fail() { printf '[v23.0-c3-a] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v23.0-c3-a] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" SERIAL="" IOS_UP=0 WE_BOOTED_IOS=0 WE_BOOTED_AND=0
cleanup() {
  [ "$IOS_UP" = 1 ] && "$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" >/dev/null 2>&1 || true
  [ "$WE_BOOTED_IOS" = 1 ] && "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true
  [ "$WE_BOOTED_AND" = 1 ] && "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

# owner of one kind on one device, from `smix status --json`; empty for none
owner_of() { # $1 device id, $2 kind
  "$SMIX" status --json | python3 -c '
import json, sys
rows = {r["id"]: r for r in json.load(sys.stdin)}
print(rows.get(sys.argv[1], {}).get("owners", {}).get(sys.argv[2], ""))' "$1" "$2"
}
booted_by_smix() {
  "$SMIX" status --json | python3 -c '
import json, sys
rows = {r["id"]: r for r in json.load(sys.stdin)}
print(rows.get(sys.argv[1], {}).get("bootedBySmix", False))' "$1"
}

UDID="$("$SMIX" sim resolve "$E2E_IOS" 2>/dev/null | tail -1 | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$E2E_IOS'"
case "$(simulator_name "$UDID")" in
  sim-smix-*) ;;
  *) cannot_judge "'$E2E_IOS' resolves to $UDID, which is not one of smix's own simulators" ;;
esac
[ "$(simulator_state "$UDID")" = Booted ] && cannot_judge "$UDID is already on; this needs to boot it"
"$SMIX" sim resolve "$E2E_ANDROID" >/dev/null 2>&1 && cannot_judge "$E2E_ANDROID is already on; this needs to boot it"

log "1. task A boots the simulator, task B the emulator"
with_deadline 300 "$SMIX" --owner "$A" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
WE_BOOTED_IOS=1
SMIX_OWNER="$B" with_deadline 300 "$SMIX" sim boot "$E2E_ANDROID" --no-window >"$WORK/aboot.log" 2>&1 \
  || fail "boot: $(tail -3 "$WORK/aboot.log")"
SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tail -1)"
WE_BOOTED_AND=1
[ "$(owner_of "$UDID" booted)" = "$A" ] || fail "1: the simulator's boot is not $A's: '$(owner_of "$UDID" booted)'"
[ "$(owner_of "$SERIAL" booted)" = "$B" ] || fail "1: the emulator's boot is not $B's: '$(owner_of "$SERIAL" booted)'"
[ "$(booted_by_smix "$UDID")" = True ] || fail "1: status does not say smix booted the simulator"
"$SMIX" status | grep -q "owner $A" || fail "1: the text status does not name $A"

log "2. task B's runner on task A's simulator"
"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
SMIX_OWNER="$B" "$SMIX" runner up "$UDID" --bundle "$IOS_APP" --runner-port "$IOS_PORT" >"$WORK/up.log" 2>&1 \
  || fail "runner up: $(tail -5 "$WORK/up.log")"
IOS_UP=1
[ "$(owner_of "$UDID" runner)" = "$B" ] || fail "2: the runner is not $B's: '$(owner_of "$UDID" runner)'"
[ "$(owner_of "$UDID" booted)" = "$A" ] || fail "2: the boot moved to '$(owner_of "$UDID" booted)'"

log "3. shut down, the owners go with them"
"$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" >/dev/null 2>&1 || fail "runner down"
IOS_UP=0
"$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || fail "shutdown $UDID"
WE_BOOTED_IOS=0
"$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || fail "shutdown $SERIAL"
WE_BOOTED_AND=0
for d in "$UDID" "$SERIAL"; do
  for k in booted runner; do
    [ -z "$(owner_of "$d" "$k")" ] || fail "3: $d still names an owner for $k: $(owner_of "$d" "$k")"
  done
done

log "V23.0-C3-A-WHOSE-DEVICE-PASS"
