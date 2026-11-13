#!/usr/bin/env bash
# v26.1: a device that came up outside smix is named, and smix will not
# drive it until somebody takes it on with a reason; an emulator smix boots
# has no window unless asked.
#
# 2026-11-13: an agent started an emulator with a bare `emulator -avd`. It
# came up as a window on the owner's desktop, stopped on its PIN screen,
# and nothing had a record of it. A hook cannot see a script that spawns a
# device, so the judgement here is the runtime one: with a device started
# by hand, `sim list` and `doctor` name it, `runner up` and `run` refuse it,
# and a `lease claim --reason` (logged) is what lets them through.
#
# The ledger is this script's own (SMIX_MACHINE_DIR). The simulator must be
# shut down when this starts: it boots it by hand and shuts it down again.
#   SMIX_E2E_FENCE_SIM=<UDID>  required, a simulator nobody is using
#   SMIX_E2E_FENCE_AVD=<name>  default: the third smix AVD
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/e2e-devices.sh
source "$ROOT/scripts/lib/e2e-devices.sh"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"

UDID="${SMIX_E2E_FENCE_SIM:-}"
AVD="${SMIX_E2E_FENCE_AVD:-$E2E_ANDROID_THIRD}"
ALIAS="fence-e2e-avd"
FENCE_TEXT="was not started by smix and nobody has claimed it"
WORK="$(mktemp -d)"

log()  { printf '[fence] %s\n' "$*"; }
step() { printf '[fence] --- %s\n' "$*"; }
fail() { printf '[fence] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[fence] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

# set to yes only once this script has booted the simulator itself; it
# refuses to start on one that was already up
WE_BOOTED=no
RUNNER_UP=no
SERIAL=""
gate_free_port RUNNER_PORT
cleanup() {
  if [ "$RUNNER_UP" = yes ] && ! "$SMIX" runner down --runner-port "$RUNNER_PORT" > "$WORK/cleanup-down.txt" 2>&1; then
    printf '[fence] teardown: runner down on %s failed:\n' "$RUNNER_PORT" >&2
    cat "$WORK/cleanup-down.txt" >&2
  fi
  [ "$WE_BOOTED" = yes ] && xcrun simctl shutdown "$UDID" >/dev/null 2>&1
  [ -n "$SERIAL" ] && e2e_stop_emulator "$SERIAL"
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -n "$UDID" ] || cannot_judge "set SMIX_E2E_FENCE_SIM to a simulator nobody is using"
[ "$(simulator_state "$UDID")" = Shutdown ] || cannot_judge "$UDID is not shut down"
command -v adb >/dev/null 2>&1 || cannot_judge "no adb"
pgrep -f -- "-avd $AVD( |$)" >/dev/null 2>&1 && cannot_judge "$AVD is already running"

e2e_isolate_machine "$WORK"
mkdir -p "$WORK/ws/.smix"
cd "$WORK/ws"
printf 'appId: com.apple.Preferences\n---\n- assertVisible: "General"\n' > "$WORK/flow.yaml"
EXCEPTIONS="$SMIX_MACHINE_DIR/fence-exceptions.jsonl"

step "iOS 1. a simulator booted by hand"
xcrun simctl boot "$UDID" || fail "could not boot $UDID by hand"
WE_BOOTED=yes
xcrun simctl bootstatus "$UDID" -b >/dev/null || fail "$UDID did not finish booting"

step "iOS 2. sim list and doctor name it"
"$SMIX" sim list > "$WORK/list.txt" 2>&1
grep -q "not started by smix" "$WORK/list.txt" && grep -q "$UDID" <(sed -n '/not started by smix/,$p' "$WORK/list.txt") \
  || { cat "$WORK/list.txt"; fail "sim list did not name $UDID as started outside smix"; }
"$SMIX" sim list --json | python3 -c '
import json, sys
rows = [r for r in json.load(sys.stdin) if r.get("udid", "").upper() == sys.argv[1].upper()]
sys.exit(0 if rows and rows[0].get("startedOutsideSmix") is True else 1)' "$UDID" \
  || fail "sim list --json does not say startedOutsideSmix for $UDID"
"$SMIX" doctor > "$WORK/doctor.txt" 2>&1 || { cat "$WORK/doctor.txt"; fail "doctor failed"; }
grep -q "$UDID" "$WORK/doctor.txt" || { cat "$WORK/doctor.txt"; fail "doctor did not name $UDID"; }

step "iOS 3. runner up and run refuse it, before building anything"
start=$(date +%s)
"$SMIX" runner up "$UDID" --bundle com.apple.Preferences --runner-port "$RUNNER_PORT" > "$WORK/up.txt" 2>&1
rc=$?
took=$(( $(date +%s) - start ))
[ "$rc" != 0 ] && grep -q "$FENCE_TEXT" "$WORK/up.txt" \
  || { cat "$WORK/up.txt"; fail "runner up was not refused by the fence (rc=$rc)"; }
[ "$took" -lt 30 ] || fail "the refusal took ${took}s: it should come before any build"
# raw run: what is judged is the fence's refusal, which comes before any flow runs
"$SMIX" run --device "$UDID" "$WORK/flow.yaml" > "$WORK/run.txt" 2>&1
rc=$?
[ "$rc" != 0 ] && grep -q "$FENCE_TEXT" "$WORK/run.txt" \
  || { cat "$WORK/run.txt"; fail "run was not refused by the fence (rc=$rc)"; }

step "iOS 4. a claim needs a reason, and is logged"
"$SMIX" lease claim "$UDID" > "$WORK/claim0.txt" 2>&1 && fail "a claim with no --reason was accepted"
"$SMIX" lease claim "$UDID" --reason "fence e2e takes it on" > "$WORK/claim.txt" 2>&1 \
  || { cat "$WORK/claim.txt"; fail "the claim was refused"; }
[ "$(grep -c '"reason":"fence e2e takes it on"' "$EXCEPTIONS" 2>/dev/null)" = 1 ] \
  || fail "the claim was not logged once in $EXCEPTIONS"
"$SMIX" lease status "$UDID" 2>&1 | grep -q "fence e2e takes it on" || fail "lease status does not show the reason"
"$SMIX" sim list 2>&1 | sed -n '/not started by smix/,$p' | grep -q "$UDID" && fail "sim list still names a claimed device"

step "iOS 5. runner up goes through once it is claimed"
RUNNER_UP=yes
"$SMIX" runner up "$UDID" --bundle com.apple.Preferences --runner-port "$RUNNER_PORT" > "$WORK/up2.txt" 2>&1 \
  || { tail -20 "$WORK/up2.txt"; fail "runner up on a claimed device failed"; }
"$SMIX" runner down --runner-port "$RUNNER_PORT" > "$WORK/down1.txt" 2>&1 \
  || { cat "$WORK/down1.txt"; fail "runner down failed"; }
RUNNER_UP=no
xcrun simctl shutdown "$UDID" || fail "could not shut $UDID down"
WE_BOOTED=no

step "Android 1. an emulator started by hand"
listed() { adb devices 2>/dev/null | awk -v s="$1" '$1 == s { f = 1 } END { exit !f }'; }
free_console_port() {
  local p=5640
  while [ "$p" -lt 5680 ]; do
    listed "emulator-$p" || { lsof -nP -iTCP:"$p" -sTCP:LISTEN >/dev/null 2>&1 || { echo "$p"; return 0; }; }
    p=$((p + 2))
  done
  return 1
}
PORT="$(free_console_port)" || cannot_judge "no free emulator console port"
SERIAL="emulator-$PORT"
e2e_start_emulator "$WORK/emu.log" -avd "$AVD" -port "$PORT" -no-boot-anim -no-snapshot-save
booted=no
for _ in $(seq 1 120); do
  [ "$(adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && { booted=yes; break; }
  sleep 2
done
[ "$booted" = yes ] || fail "$AVD did not boot by hand — see $WORK/emu.log"

step "Android 2. named, and run refuses it"
"$SMIX" sim list 2>&1 | sed -n '/not started by smix/,$p' | grep -q "$SERIAL" \
  || fail "sim list did not name $SERIAL as started outside smix"
# raw run: what is judged is the fence's refusal, which comes before any flow runs
"$SMIX" run --platform android --device "$SERIAL" "$WORK/flow.yaml" > "$WORK/arun.txt" 2>&1
grep -q "$FENCE_TEXT" "$WORK/arun.txt" || { cat "$WORK/arun.txt"; fail "run on $SERIAL was not refused by the fence"; }
"$SMIX" sim register "$ALIAS" --udid "$SERIAL" --kind emulator > "$WORK/reg.txt" 2>&1 \
  || { cat "$WORK/reg.txt"; fail "could not register $AVD"; }
e2e_stop_emulator "$SERIAL" || fail "the hand-started $SERIAL did not quit"
SERIAL=""

step "Android 3. smix sim boot, no flags: no window, and drivable"
unset SMIX_EMULATOR_NO_WINDOW
"$SMIX" sim boot "$ALIAS" --wait 600 > "$WORK/boot.txt" 2>&1 || { cat "$WORK/boot.txt"; fail "smix sim boot $ALIAS failed"; }
args="$(ps -axo args | grep -E -- "-avd $AVD( |$)" | grep -v grep | head -1)"
[ -n "$args" ] || fail "no emulator process for $AVD after smix sim boot"
SERIAL="emulator-$(printf '%s\n' "$args" | sed -nE 's/.*-port ([0-9]+).*/\1/p')"
[ "$SERIAL" != "emulator-" ] || fail "no -port in: $args"
case " $args " in *" -no-window "*) ;; *) fail "smix sim boot started a window: $args" ;; esac
"$SMIX" sim list 2>&1 | sed -n '/not started by smix/,$p' | grep -q "$SERIAL" && fail "sim list names an emulator smix booted"
# raw run: what is judged is only that the fence lets it past
"$SMIX" run --platform android --device "$SERIAL" "$WORK/flow.yaml" > "$WORK/arun2.txt" 2>&1
grep -q "$FENCE_TEXT" "$WORK/arun2.txt" && { cat "$WORK/arun2.txt"; fail "run refused an emulator smix booted"; }
"$SMIX" sim shutdown "$ALIAS" > "$WORK/down.txt" 2>&1 || { cat "$WORK/down.txt"; fail "smix sim shutdown $ALIAS failed"; }
SERIAL=""

log "PASS"
