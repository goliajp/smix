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
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
OURS_ALIAS="${SMIX_C5_OURS_ALIAS:-smix-android}"
WORK="$(mktemp -d)"

log()  { printf '[c5] %s\n' "$*" >&2; }
step() { printf '[c5] --- %s\n' "$*" >&2; }
fail() { printf '[c5] FAIL: %s\n' "$*" >&2; exit 1; }
skip() { printf '[c5] SKIP: %s\n' "$*" >&2; exit 0; }

# What this script starts by hand it stops by hand; what it starts via
# smix it stops via smix. Nothing else.
THEIRS_ANDROID=""
THEIRS_IOS=""
cleanup() {
  [ -n "$THEIRS_ANDROID" ] && adb -s "$THEIRS_ANDROID" emu kill >/dev/null 2>&1 || true
  [ -n "$THEIRS_IOS" ] && xcrun simctl shutdown "$THEIRS_IOS" >/dev/null 2>&1 || true
  "$SMIX" sim shutdown "$OURS_ALIAS" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX"
command -v adb >/dev/null 2>&1 || skip "no adb"
command -v xcrun >/dev/null 2>&1 || skip "no xcrun"

OURS_SERIAL="$("$SMIX" sim resolve "$OURS_ALIAS" 2>/dev/null | grep -v '^kevy:' | tr -d '[:space:]')" || true
[ -n "$OURS_SERIAL" ] || skip "no emulator registered as '$OURS_ALIAS'"
OURS_PORT="${OURS_SERIAL##*-}"
THEIRS_PORT=$((OURS_PORT + 2))
THEIRS_ANDROID="emulator-$THEIRS_PORT"

# The hand-started rows must be on ports / UDIDs nobody has a ledger for.
# Both devices are off on arrival — this script refuses to run otherwise —
# so what it started is exactly what it stops, and the machine is restored
# to the empty state it began in.
WE_BOOTED_IT=yes
adb devices 2>/dev/null | grep -q "^emulator-$OURS_PORT" && { WE_BOOTED_IT=no; fail "$OURS_SERIAL is already up — start from nothing"; }
adb devices 2>/dev/null | grep -q "^$THEIRS_ANDROID" && { WE_BOOTED_IT=no; fail "$THEIRS_ANDROID is already up — start from nothing"; }

ready_android() {
  for _ in $(seq 1 60); do
    [ "$(adb -s "$1" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ] && return 0
    sleep 5
  done
  return 1
}

# ---------------------------------------------------------------- Android
step "A1. ours through smix, theirs by hand"
"$SMIX" sim boot "$OURS_ALIAS" >/dev/null 2>&1 || fail "smix could not boot $OURS_ALIAS"
"${ANDROID_HOME:-$HOME/Library/Android/sdk}/emulator/emulator" -avd sim-smix-android-01 \
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
SMIX_RUNNER_PORT=22097 "$SMIX" down > "$WORK/down.log" 2>&1 || true
grep -q "c5-theirs ($THEIRS_ANDROID) is up but not ours" "$WORK/down.log" \
  || fail "down did not say it left $THEIRS_ANDROID alone:
$(grep -E 'c5-theirs|smix-android' "$WORK/down.log")"
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
[ -n "$THEIRS_IOS" ] || skip "no shut-down sim-smix-* to stand in for somebody else's"
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
