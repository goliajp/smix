#!/usr/bin/env bash
# v6.1-C5: two devices up, one of them not ours, and every path that
# stops or drives a device only reaches the one that is.
#
# Every ownership verdict in this version is trivially true on a machine
# with one device: there is nobody to refuse. This is the check that has
# two, and it is the only place "smix does not touch what it did not
# start" is a claim about anything rather than a sentence.
#
# Per platform, the same two rows: an emulator / simulator smix booted
# (recorded in the ledger) and one started by hand on this machine (no
# record). Then, for each stopping and each choosing path:
#   - the ledger-booted one is reachable / stoppable, and IS stopped
#   - the hand-started one is refused, by name, and is still running after
#
# Both halves per `.claude/rule/empty-predicate.md`. Proving refusal
# alone proves nothing about whether smix still works; proving reach
# alone proves nothing about whether it refuses.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/e2e-devices.sh
source "$ROOT/scripts/lib/e2e-devices.sh"
WORK="$(mktemp -d)"
OURS_ALIAS=c5-ours
OURS_AVD="${SMIX_C5_OURS_AVD:-$E2E_ANDROID_SECOND}"
THEIRS_AVD="${SMIX_C5_THEIRS_AVD:-$E2E_ANDROID}"

log()  { printf '[c5] %s\n' "$*" >&2; }
step() { printf '[c5] --- %s\n' "$*" >&2; }
fail() { printf '[c5] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c5] SKIP: %s\n' "$*" >&2; exit 2; }

# The whole check runs in a ledger of its own. `smix down` stops every
# registered device whose boot row says smix started it and whose holder
# is gone — on the machine's real ledger that is the release's own
# devices and anybody else's that smix booted. Until 2026-09-25 this ran
# `down` on the real ledger, registered `c5-theirs` into it, and took its
# device from the alias `smix-android`, which by then named a consumer's
# emulator.
e2e_isolate_machine "$WORK"

# What this script starts by hand it stops by hand; what it starts via
# smix it stops via smix. Nothing else.
THEIRS_ANDROID=""
THEIRS_IOS=""
OURS_SERIAL=""
cleanup() {
  [ -n "$THEIRS_ANDROID" ] && adb -s "$THEIRS_ANDROID" emu kill >/dev/null 2>&1 || true
  [ -n "$THEIRS_IOS" ] && xcrun simctl shutdown "$THEIRS_IOS" >/dev/null 2>&1 || true
  [ -n "$OURS_SERIAL" ] && "$SMIX" sim shutdown "$OURS_ALIAS" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX"
command -v adb >/dev/null 2>&1 || cannot_judge "no adb"
command -v xcrun >/dev/null 2>&1 || cannot_judge "no xcrun"
EMULATOR="${ANDROID_HOME:-$HOME/Library/Android/sdk}/emulator/emulator"

# Our AVD must be off on arrival: this script starts it, so what it stops
# at the end is exactly what it started.
for s in $(adb devices 2>/dev/null | awk '$1 ~ /^emulator-/ && $2=="device" {print $1}'); do
  [ "$(adb -s "$s" emu avd name 2>/dev/null | head -1 | tr -d '\r')" = "$OURS_AVD" ] \
    && cannot_judge "$OURS_AVD is already running as $s — this check starts it from nothing"
done

# "Theirs" is a second instance of another smix AVD, started read-only by
# hand. The emulator refuses that while the same AVD runs normally
# anywhere ("run all emulators with -read-only"), and in a release the
# first smix AVD is the one the release itself is driving — so there is
# no AVD to stand in for somebody else's, and this says so before
# starting anything rather than failing halfway. A third smix AVD would
# let it run beside a release.
for s in $(adb devices 2>/dev/null | awk '$1 ~ /^emulator-/ && $2=="device" {print $1}'); do
  [ "$(adb -s "$s" emu avd name 2>/dev/null | head -1 | tr -d '\r')" = "$THEIRS_AVD" ] \
    && cannot_judge "$THEIRS_AVD is running as $s, so a read-only second instance of it cannot start to stand in for somebody else's emulator — run this with $THEIRS_AVD off, or point SMIX_C5_THEIRS_AVD at a third smix AVD"
done

# Two console ports nothing answers on. Chosen, not derived from a
# registered serial: a serial is a port, and ports are shared.
free_port() {
  local p
  for p in $(seq "$1" 2 5680); do
    adb devices 2>/dev/null | grep -q "^emulator-$p[[:space:]]" && continue
    lsof -nP -iTCP:"$p" -sTCP:LISTEN >/dev/null 2>&1 && continue
    echo "$p"; return 0
  done
  return 1
}
OURS_PORT="$(free_port 5600)" || cannot_judge "no free emulator console port"
THEIRS_PORT="$(free_port $((OURS_PORT + 2)))" || cannot_judge "no second free console port"
THEIRS_ANDROID="emulator-$THEIRS_PORT"
WE_BOOTED_IT=yes

ready_android() {
  for _ in $(seq 1 60); do
    [ "$(adb -s "$1" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ] && return 0
    sleep 5
  done
  return 1
}
gone_android() {
  for _ in $(seq 1 30); do
    adb devices 2>/dev/null | grep -q "^$1[[:space:]]" || return 0
    sleep 2
  done
  return 1
}

# This ledger has to learn our AVD the way a user's does: registered once
# while it runs (that is when its name is recorded), then booted by smix,
# which is what writes the boot row `down` reads.
step "A0. our AVD, registered in this ledger and then booted by smix"
"$EMULATOR" -avd "$OURS_AVD" -port "$OURS_PORT" -no-boot-anim > "$WORK/ours-hand.log" 2>&1 &
ready_android "emulator-$OURS_PORT" || { tail -5 "$WORK/ours-hand.log" >&2; fail "$OURS_AVD did not come up to be registered"; }
"$SMIX" sim register "$OURS_ALIAS" --udid "emulator-$OURS_PORT" --kind emulator > "$WORK/reg.log" 2>&1 \
  || { cat "$WORK/reg.log" >&2; fail "could not register $OURS_AVD in this script's ledger"; }
adb -s "emulator-$OURS_PORT" emu kill >/dev/null 2>&1 || true
gone_android "emulator-$OURS_PORT" || fail "the registration boot of $OURS_AVD did not stop"

# ---------------------------------------------------------------- Android
step "A1. ours through smix, theirs by hand"
"$SMIX" sim boot "$OURS_ALIAS" > "$WORK/boot.log" 2>&1 || { cat "$WORK/boot.log" >&2; fail "smix could not boot $OURS_ALIAS"; }
OURS_SERIAL="$("$SMIX" sim resolve "$OURS_ALIAS" 2>/dev/null | tr -d '[:space:]')"
[ -n "$OURS_SERIAL" ] || fail "smix booted $OURS_ALIAS and cannot say which serial it is on"
ready_android "$OURS_SERIAL" || fail "$OURS_SERIAL did not finish booting"
"$EMULATOR" -avd "$THEIRS_AVD" \
  -port "$THEIRS_PORT" -no-boot-anim -read-only > "$WORK/theirs-android.log" 2>&1 &
ready_android "$THEIRS_ANDROID" || { tail -5 "$WORK/theirs-android.log" >&2; fail "the hand-started emulator did not come up"; }
log "up: ours=$OURS_SERIAL theirs=$THEIRS_ANDROID"

step "A2. choosing: pick-dev-emulator names ours and only ours"
PICKED="$(bash "$ROOT/scripts/dev/pick-dev-emulator.sh" 2>"$WORK/pick.err")" \
  || { cat "$WORK/pick.err" >&2; fail "with ours up, the picker refused"; }
[ "$PICKED" = "$OURS_SERIAL" ] || fail "the picker chose $PICKED, not ours ($OURS_SERIAL)"
log "picked $PICKED"

step "A3. stopping theirs through smix is refused, and theirs stays up"
"$SMIX" sim register c5-theirs --udid "$THEIRS_ANDROID" --kind emulator >/dev/null 2>&1 || true
if "$SMIX" sim shutdown c5-theirs > "$WORK/refuse-a.log" 2>&1; then
  cat "$WORK/refuse-a.log" >&2; fail "smix stopped an emulator it did not boot"
fi
grep -q "$THEIRS_ANDROID" "$WORK/refuse-a.log" || fail "the refusal does not name $THEIRS_ANDROID"
adb devices | grep -q "^$THEIRS_ANDROID" || fail "smix refused in words and $THEIRS_ANDROID is gone anyway"
log "refused, theirs still up"

step "A4. down leaves theirs alone and takes ours"
# A port of this script's own for the runner step of `down`; the default
# is 22087, which is somebody's.
. "$ROOT/scripts/lib/gate-port.sh"
"$SMIX" down > "$WORK/down.log" 2>&1 || true
grep -q "c5-theirs ($THEIRS_ANDROID) is up but not ours" "$WORK/down.log" \
  || fail "down did not say it left $THEIRS_ANDROID alone:
$(grep -E 'c5-theirs|c5-ours' "$WORK/down.log")"
adb devices | grep -q "^$THEIRS_ANDROID" || fail "down said it left $THEIRS_ANDROID alone and it is gone"
sleep 4
adb devices | grep -q "^$OURS_SERIAL" && fail "down left ours ($OURS_SERIAL) running — it did not tear down its own device"
log "down: theirs alone, ours stopped"

"$SMIX" sim unregister c5-theirs >/dev/null 2>&1 || true

# ------------------------------------------------------------------- iOS
step "I1. a simulator started by hand (no ledger) — smix must not stop it"
THEIRS_IOS="$(xcrun simctl list devices -j | python3 -c '
import json,sys
for rt in json.load(sys.stdin)["devices"].values():
    for d in rt:
        if d.get("name","").startswith("sim-smix-") and d.get("state")=="Shutdown" and d.get("isAvailable"):
            print(d["udid"]); raise SystemExit
')"
[ -n "$THEIRS_IOS" ] || cannot_judge "no shut-down sim-smix-* to stand in for somebody else's"
xcrun simctl boot "$THEIRS_IOS" >/dev/null 2>&1 || fail "could not hand-boot $THEIRS_IOS"
sleep 15
if "$SMIX" sim shutdown "$THEIRS_IOS" > "$WORK/refuse-i.log" 2>&1; then
  cat "$WORK/refuse-i.log" >&2; fail "smix stopped a simulator it did not boot"
fi
grep -q "$THEIRS_IOS" "$WORK/refuse-i.log" || fail "the iOS refusal does not name the device"
xcrun simctl list devices | grep "$THEIRS_IOS" | grep -q Booted \
  || fail "smix refused in words and $THEIRS_IOS is shut down anyway"
log "iOS refused, theirs still up"

step "I2. pick-dev-sim does not hand out the hand-booted one"
if P="$(bash "$ROOT/scripts/dev/pick-dev-sim.sh" 2>/dev/null)"; then
  [ "$P" != "$THEIRS_IOS" ] || fail "pick-dev-sim handed out a simulator no ledger says smix booted"
fi
log "picker does not offer theirs"

log "both platforms: ours reachable, theirs refused and left running"
