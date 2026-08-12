#!/usr/bin/env bash
# A device you can register and drive, you can also load.
#
# 4.0 made a physical Android device registrable, addressable and
# drivable, and left no way to put an app on it: the install verb routed
# to simctl alone while `smix-adb` had carried the call that does it the
# whole time. The device guard refuses the bare form and names smix as
# the way through, so the two pointed at each other. A consumer moved
# all eight copies of that guard aside to get a build onto a phone, and
# writing this change ran into the same wall three times.
#
# Nothing here needs a device. Every case runs against a throwaway
# machine directory with fabricated registrations, and the two that
# matter are proved by *which refusal* comes back rather than by
# success: a serial nothing is listening on still tells you which tool
# was reached for.
#
# Usage: bash scripts/dev/v4.1-c1-android-payload-e2e.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
PASS=0
FAIL=0

step() { echo; echo "=== $* ==="; }
ok()   { echo "  PASS: $*"; PASS=$((PASS + 1)); }
bad()  { echo "  FAIL: $*"; FAIL=$((FAIL + 1)); }

# Captured, then matched. `cmd | grep -q` reads as "not found" when it
# means "grep closed the pipe and the writer took SIGPIPE".
has()   { case "$1" in *"$2"*) return 0 ;; *) return 1 ;; esac }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
M="$WORK/machine"
W="$WORK/ws"
mkdir -p "$M" "$W/.smix"
unset SMIX_SIMS_JSON

# Every call goes at the throwaway machine directory. Without this the
# script would register into the real one — on a machine that may have
# somebody's phone plugged in.
run() {
    set +e
    OUT="$(cd "$W" && SMIX_MACHINE_DIR="$M" "$SMIX" "$@" 2>&1)"
    RC=$?
    set -e
    OUT="$(printf '%s\n' "$OUT" | grep -v '^kevy:' || true)"
}

step "0. build, and the routing table's own test"
cargo build -p smix-cli --manifest-path "$ROOT/Cargo.toml" >/dev/null 2>&1 \
    || { echo "cannot build smix-cli"; exit 2; }
[ -x "$SMIX" ] || { echo "no smix at $SMIX"; exit 2; }
set +e
UNIT="$(cd "$ROOT" && cargo test -p smix-cli --bin smix payload_verbs_reach_android 2>&1)"
URC=$?
set -e
# "ok. 0 passed" is what a filter that matched nothing prints, and it
# reads exactly like success.
if [ "$URC" = 0 ] && has "$UNIT" "1 passed"; then
    ok "the routing table says android, and the test that says so ran"
else
    bad "unit exit $URC; expected 1 passed"
fi

step "1. fabricate the devices — nothing is listening on any of them"
"$SMIX" --version >/dev/null 2>&1
# The emulator is addressed by its bare serial rather than registered:
# registering one is checked against adb, by design, so a serial nothing
# is listening on cannot be registered at all. `emulator-NNNN` shape is
# addressable without a record — `is_emulator_serial` in the resolver —
# which is also how somebody would really type it.
SMIX_MACHINE_DIR="$M" "$SMIX" sim register phone \
    --udid FAKESERIAL0001 --kind physical-android >/dev/null 2>&1 \
    || { echo "cannot register the fixture phone"; exit 2; }
# The proof they are fabrications: adb has never heard of them. Without
# this the cases below could be passing against something real.
ADB_LIST="$(adb devices 2>/dev/null || true)"
if has "$ADB_LIST" "emulator-9999" || has "$ADB_LIST" "FAKESERIAL0001"; then
    echo "a fixture serial is actually attached — pick different ones"; exit 2
fi
ok "a phone registered and an emulator serial named; neither attached"

step "2. LOAD-BEARING — install reaches for adb, not simctl"
# The old refusal was the transfer gate's: "this command runs through
# simctl, and <device> is a physical Android device". Its absence is the
# whole change; the presence of adb's own words is the confirmation.
run sim install emulator-9999 "$WORK/none.apk"
if has "$OUT" "runs through simctl"; then
    bad "still refused by the simctl transfer gate: $OUT"
elif has "$OUT" "adb" && has "$OUT" "emulator-9999"; then
    ok "adb was reached, and said what it says about a device that is not there"
else
    bad "neither refusal nor adb: $OUT"
fi

step "3. uninstall on a phone hits the opt-in gate"
# Not load-bearing, and the difference is worth stating: the destructive
# gate runs *before* the one that refuses a verb a device has no path
# for. So this step passes whether or not uninstall reaches Android —
# proved by putting it back to Apple-only and watching this stay green.
# What it does check is that the gate is in front, which is the order
# that matters when the answer is "no".
run sim uninstall phone com.example.app
if [ "$RC" = 0 ]; then
    bad "an unopted-in phone was uninstalled from"
elif has "$OUT" "allow-destructive"; then
    ok "refused, and named the one-time opt-in"
else
    bad "refused for some other reason: $OUT"
fi

step "4. LOAD-BEARING — past the gate, it is adb that answers"
# This is where the routing shows. With uninstall back at Apple-only,
# the opened gate hands the call to the next refusal — the transfer gate
# saying simctl cannot reach an Android device — instead of to adb.
run sim allow-destructive phone
run sim uninstall phone com.example.app
if has "$OUT" "allow-destructive"; then
    bad "still gated after opting in: $OUT"
elif has "$OUT" "runs through simctl"; then
    bad "past the gate and still routed at simctl — uninstall never reached Android: $OUT"
elif has "$OUT" "adb"; then
    ok "the gate opened and adb took the call"
else
    bad "unexpected: $OUT"
fi

step "5. a physical iPhone is refused, not attempted"
# No devicectl install path is wired. §9 #1 ③: say so rather than
# degrade into something that looks like it worked.
SMIX_MACHINE_DIR="$M" "$SMIX" sim register iphone \
    --udid 00008120-000000000000000E --kind physical-ios >/dev/null 2>&1 || true
run sim install iphone "$WORK/none.app"
if [ "$RC" = 0 ]; then
    bad "claimed to install on a physical iPhone"
elif has "$OUT" "simctl" || has "$OUT" "physical iPhone"; then
    ok "refused, naming what it is and what cannot reach it"
else
    bad "refused for some other reason: $OUT"
fi

step "6. the simulator path is untouched"
run sim install 00000000-1111-2222-3333-444444444444 "$WORK/none.app"
if has "$OUT" "adb"; then
    bad "a simulator was sent to adb: $OUT"
else
    ok "simulators still go through simctl"
fi

echo
echo "=== $PASS passed, $FAIL failed ==="
[ "$FAIL" = 0 ] && echo "V41-C1-ANDROID-PAYLOAD-PASS"
[ "$FAIL" = 0 ]
