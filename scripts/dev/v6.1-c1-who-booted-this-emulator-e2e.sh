#!/usr/bin/env bash
# v6.1-C1: whoever booted it stops it, on a real emulator.
#
# The unit tests pin the verdict; this pins that the verdict is reached
# on the way to a device, and that both of its answers are reachable.
#
# Three segments, and the third is the one that matters. Proving smix
# can stop what it started proves nothing about whether it will stop
# what somebody else started — that is a "must not happen" claim, and by
# `.claude/rule/empty-predicate.md` it has to be paired with a case that
# makes it happen. So the third segment starts an emulator deliberately
# outside smix and requires the refusal.
#
# Measured cost of not having this: over one day, another person driving
# smix on this machine stopped the release's emulator a dozen times, and
# two release gates picked their device instead of ours. Nobody did
# anything wrong; nothing could be asked.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/e2e-devices.sh
source "$ROOT/scripts/lib/e2e-devices.sh"
# Our second AVD, which nothing else here keeps running — this check needs
# a device it can start from nothing. It used to be the alias
# `smix-android`, which by 2026-09-25 named emulator-5560: a consumer's
# emulator, whose port step 3 starts an emulator on by hand.
ALIAS="${SMIX_C1_ALIAS:-$E2E_ANDROID_SECOND}"
WORK="$(mktemp -d)"

log()  { printf '[c1] %s\n' "$*" >&2; }
step() { printf '[c1] --- %s\n' "$*" >&2; }
fail() { printf '[c1] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c1] SKIP: %s\n' "$*" >&2; exit 2; }

cleanup() {
  # Only what this script started, and only through the console. An
  # emulator somebody else has since started on that port is not ours.
  if [ "${WE_STARTED_MANUAL:-0}" = 1 ]; then
    e2e_stop_emulator "$SERIAL" || true
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
command -v adb >/dev/null 2>&1 || cannot_judge "no adb — this needs the Android SDK"

# An emulator alias names an AVD, and an AVD that is not running has no
# serial yet — resolving it says so. That is the arrival state this check
# wants; anything else is somebody's running device.
SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tr -d '[:space:]')" || true
log "device $ALIAS (on arrival: ${SERIAL:-not running})"

ready() {
  for _ in $(seq 1 60); do
    [ "$(adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ] && return 0
    sleep 5
  done
  return 1
}

running() { adb devices 2>/dev/null | grep -q "^$SERIAL"; }

step "0. start from nothing on that port"
# What this script found on arrival, recorded rather than assumed. It
# takes the strong route — refuse to run at all unless the port is
# empty — so the answer is always the same one, and shutting the device
# down at the end therefore restores exactly what was here. Written out
# because a teardown that cannot say what it is restoring to is how one
# script leaves the next one without a device.
WE_BOOTED_IT=yes
if [ -n "$SERIAL" ]; then
  WE_BOOTED_IT=no
  # Whatever is there now was not started by this run, and this script
  # does not stop devices it did not start — the rule it is here to
  # check applies to it too.
  fail "$ALIAS is already running as $SERIAL. This check starts from nothing so that
'who booted it' has one answer. Stop it the way it was started, then re-run."
fi

step "1. smix boots it, and the ledger says so"
"$SMIX" sim boot "$ALIAS" > "$WORK/boot.log" 2>&1 \
  || { cat "$WORK/boot.log" >&2; fail "smix could not boot $ALIAS"; }
SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tr -d '[:space:]')" || true
[ -n "$SERIAL" ] || fail "smix booted $ALIAS and cannot say which serial it is on"
running || fail "smix reported a boot and adb does not list $SERIAL — a report of
something that did not happen is the defect this version is about"
"$SMIX" lease owner "$SERIAL" 2>/dev/null > "$WORK/owner.log" || true
grep -qi "booted by smix" "$WORK/owner.log" \
  || fail "the ledger cannot say who booted $SERIAL: $(cat "$WORK/owner.log")"
log "ledger: $(head -1 "$WORK/owner.log")"

step "2. smix stops what it started, and the row goes with it"
"$SMIX" sim shutdown "$ALIAS" > "$WORK/down.log" 2>&1 \
  || { cat "$WORK/down.log" >&2; fail "smix refused to stop a device it booted"; }
sleep 5
running && fail "smix reported a shutdown and $SERIAL is still listed"
"$SMIX" lease owner "$SERIAL" 2>/dev/null > "$WORK/owner2.log" || true
grep -qi "booted by smix" "$WORK/owner2.log" \
  && fail "the boot row outlived the device — a later teardown would read it and
stop whatever is on that port next: $(cat "$WORK/owner2.log")"
log "row cleared"

step "3. an emulator smix did not start is not smix's to stop"
# Start it outside smix, the way a person or another tool would: the same
# AVD, on the port smix just gave back.
e2e_start_emulator "$WORK/manual.log" -avd "${SMIX_C1_AVD:-$ALIAS}" \
  -port "${SERIAL##*-}" -no-boot-anim
WE_STARTED_MANUAL=1
ready || { tail -5 "$WORK/manual.log" >&2; fail "the hand-started emulator did not come up"; }

if "$SMIX" sim shutdown "$ALIAS" > "$WORK/refuse.log" 2>&1; then
  cat "$WORK/refuse.log" >&2
  fail "smix stopped an emulator it did not start"
fi
running || fail "smix refused in words and the device is gone anyway"
grep -q "$SERIAL" "$WORK/refuse.log" \
  || fail "the refusal does not name the device: $(cat "$WORK/refuse.log")"
grep -qi "boot" "$WORK/refuse.log" \
  || fail "the refusal does not state the rule it applies: $(cat "$WORK/refuse.log")"
log "refused, and the device is still running"

log "all three hold"
