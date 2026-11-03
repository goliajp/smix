#!/usr/bin/env bash
# v23.0-C3: a boot first asks whether the machine has room, and when it
# has not, names what is running and whose it is.
#
# Two tasks once booted eleven devices on one Mac and nearly stopped it.
# The budget is set outright here, so other work on the machine cannot
# decide the outcome:
#   1. task A boots an iPhone simulator (budget wide open)
#   2. task B's Apple TV is refused: one runs alone, and the refusal names
#      A's iPhone and A
#   3. task B's emulator under a 1 GiB budget, `--wait 12`: it waits, then
#      is refused naming the same, and is not booted
#   4. the same emulator under the open budget boots — the budget was the
#      only thing in the way
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
WORK="$(mktemp -d)"
A="c3-task-a-$$"
B="c3-task-b-$$"
TV="sim-smix-tv"
OPEN=100000

log()  { printf '[v23.0-c3-c] %s\n' "$*" >&2; }
fail() { printf '[v23.0-c3-c] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v23.0-c3-c] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" TV_UDID="" SERIAL=""
cleanup() {
  [ -n "$UDID" ] && "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true
  [ -n "$TV_UDID" ] && "$SMIX" sim shutdown "$TV_UDID" >/dev/null 2>&1 || true
  [ -n "$SERIAL" ] && "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

UDID="$("$SMIX" sim resolve "$E2E_IOS" 2>/dev/null | tail -1 | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$E2E_IOS'"
TV_UDID="$("$SMIX" sim resolve "$TV" 2>/dev/null | tail -1 | tr -d '[:space:]')" || cannot_judge "no $TV registered"
for u in "$UDID" "$TV_UDID"; do
  case "$(simulator_name "$u")" in sim-smix-*) ;; *) cannot_judge "$u is not one of smix's own simulators" ;; esac
  [ "$(simulator_state "$u")" = Booted ] && cannot_judge "$u is already on; this needs to boot it"
done
"$SMIX" sim resolve "$E2E_ANDROID" >/dev/null 2>&1 && cannot_judge "$E2E_ANDROID is already on; this needs to boot it"

log "1. task A boots an iPhone"
SMIX_DEVICE_BUDGET_GIB=$OPEN with_deadline 300 "$SMIX" --owner "$A" sim boot "$UDID" >"$WORK/boot.log" 2>&1 \
  || fail "boot: $(tail -3 "$WORK/boot.log")"

log "2. task B's Apple TV is refused"
# raw run: this refusal is what is judged
if SMIX_DEVICE_BUDGET_GIB=$OPEN "$SMIX" --owner "$B" sim boot "$TV_UDID" >"$WORK/tv.log" 2>&1; then
  fail "2: an Apple TV booted beside a running iPhone"
fi
grep -q "runs alone" "$WORK/tv.log" || fail "2: refused for another reason: $(cat "$WORK/tv.log")"
grep -q "$UDID (owner $A" "$WORK/tv.log" || fail "2: the refusal does not name A's iPhone: $(cat "$WORK/tv.log")"
[ "$(simulator_state "$TV_UDID")" = Booted ] && fail "2: the Apple TV booted anyway"

log "3. task B's emulator under a 1 GiB budget waits, then is refused"
start=$(date +%s)
if SMIX_DEVICE_BUDGET_GIB=1 SMIX_OWNER="$B" "$SMIX" sim boot "$E2E_ANDROID" --no-window --wait 12 >"$WORK/em.log" 2>&1; then
  fail "3: the emulator booted past a 1 GiB budget"
fi
waited=$(( $(date +%s) - start ))
[ "$waited" -ge 10 ] || fail "3: refused after ${waited}s, not after waiting"
grep -q "GiB budget" "$WORK/em.log" || fail "3: refused for another reason: $(cat "$WORK/em.log")"
grep -q "$UDID (owner $A" "$WORK/em.log" || fail "3: the refusal does not name A's iPhone: $(cat "$WORK/em.log")"
"$SMIX" sim resolve "$E2E_ANDROID" >/dev/null 2>&1 && fail "3: the emulator is running anyway"

log "4. the same emulator under the open budget boots"
SMIX_DEVICE_BUDGET_GIB=$OPEN SMIX_OWNER="$B" with_deadline 300 "$SMIX" sim boot "$E2E_ANDROID" --no-window \
  >"$WORK/em2.log" 2>&1 || fail "4: $(tail -3 "$WORK/em2.log")"
SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tail -1)"

log "V23.0-C3-C-ROOM-TO-BOOT-PASS"
