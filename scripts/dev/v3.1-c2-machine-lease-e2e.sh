#!/usr/bin/env bash
# One machine, one set of device ledgers.
#
# A lease says who holds a device, what they opened on it and on which
# port. Every field of that is about the machine, and until 2026-08-11
# they were written into whichever `.smix/` was above the working
# directory. That night a runner was found holding port 22087 with no
# record of it: the rule says find a runner's owner before touching it,
# the check came back empty, so it could neither be confirmed an orphan
# nor killed. It was on the books the whole time — in another
# workspace's books.
#
# The hard part of proving this is that "four checkouts agree" is also
# true of things that were never shared. `smix sim list` asks simctl, so
# it answers the same from every tree whether or not anything moved; the
# first version of the sibling checkpoint's acceptance measured exactly
# that and would have passed before the work started. So the load-bearing
# step here is step 7: a device that is booted *right now* and has no
# ledger must be reported as nobody's. Only a ledger can say that. simctl
# would say "Booted".
#
# Usage: bash scripts/dev/v3.1-c2-machine-lease-e2e.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/e2e-devices.sh
source "$ROOT/scripts/lib/e2e-devices.sh"
PASS=0
FAIL=0

step() { echo; echo "=== $* ==="; }
ok()   { echo "  PASS: $*"; PASS=$((PASS + 1)); }
bad()  { echo "  FAIL: $*"; FAIL=$((FAIL + 1)); }

step "0. build"
cargo build -p smix-cli --manifest-path "$ROOT/Cargo.toml" >/dev/null 2>&1 \
    || { echo "cannot build smix-cli"; exit 2; }
[ -x "$SMIX" ] || { echo "no smix at $SMIX"; exit 2; }

LEASES="$("$SMIX" lease list --help >/dev/null 2>&1 && echo ok)"
[ -n "$LEASES" ] || { echo "this smix has no \`lease list\` — wrong binary"; exit 2; }

step "1. every ledger this machine has"
"$SMIX" lease list 2>/dev/null || true

# Devices with a ledger are off limits: they belong to whoever is holding
# them, and this script is not it.
LEDGERED="$("$SMIX" lease list 2>/dev/null | awk '{print $1}' | tr -d ':' || true)"
has_ledger() { printf '%s\n' "$LEDGERED" | grep -qx "$1"; }

step "2. pick two devices, neither of them anybody's"
# The one this script boots and shuts down is one of the suite's own
# simulators. Any shut-down simulator without a ledger used to qualify, and
# on a shared machine that booted another project's device — its not
# having a ledger says nobody is holding it now, not that it is ours.
OWN_IDLE=" $E2E_IOS_SECOND $E2E_IOS_THIRD "
DEVICES="$(xcrun simctl list devices -j)"
# BUSY first: booted, no ledger. IDLE second: not booted, no ledger.
# In that order, so the two can never be the same device.
BUSY=""
IDLE=""
while read -r udid state; do
    has_ledger "$udid" && continue
    if [ "$state" = "Booted" ] && [ -z "$BUSY" ]; then BUSY="$udid"; fi
    if [ "$state" = "Shutdown" ] && [ -z "$IDLE" ] && [[ "$OWN_IDLE" == *" $udid "* ]]; then IDLE="$udid"; fi
done <<EOF
$(printf '%s' "$DEVICES" | python3 -c '
import json, sys
for runtime, devs in json.load(sys.stdin)["devices"].items():
    for d in devs:
        if d.get("isAvailable"):
            print(d["udid"], d["state"])
')
EOF

if [ -z "$IDLE" ]; then
    echo "none of the suite's own simulators ($E2E_IOS_SECOND, $E2E_IOS_THIRD) is shut down without a ledger — cannot run"
    exit 2
fi
echo "  IDLE (this script will boot and shut down): $IDLE"
echo "  BUSY (booted, nobody's, never touched):     ${BUSY:-none available}"

# Teardown restores what this script changed and nothing else. The device
# was down when we arrived; that is the state it goes back to.
WE_BOOTED=no
cleanup() {
    if [ "$WE_BOOTED" = yes ]; then
        echo "  restoring $IDLE to shut down"
        xcrun simctl shutdown "$IDLE" >/dev/null 2>&1 || true
        "$SMIX" lease prune >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

step "3. boot it from this checkout"
"$SMIX" sim boot "$IDLE" >/dev/null 2>&1 || { echo "boot failed"; exit 2; }
WE_BOOTED=yes
ok "booted $IDLE"

step "4. every checkout on this machine, one answer"
# Found rather than listed. A hard-coded set of paths would name
# somebody's working tree in a repository that ships, and would make
# this script answer "one checkout agrees with itself" anywhere else.
# `|| true`: find exits non-zero on the first directory it may not
# read, and under `set -e` that ends the script — which it did, one step
# after reporting a pass, leaving a booted simulator to the trap.
TREES="$( { find "$HOME" -maxdepth 4 -type d -name .smix 2>/dev/null || true; } | sed 's|/.smix$||' | head -8)"
TREES="$(printf '%s\n%s\n' "$ROOT" "$TREES" | sort -u)"
# Read from a frozen copy of the machine's ledgers, not the live ones:
# other sessions boot and lease devices on this machine while the script
# runs, and four reads a few seconds apart then disagree because the
# machine changed, not because the checkout does. A holder's liveness can
# still change under the copy, so every other checkout's read sits between
# two reads from this one; if those two differ, the round saw a change and
# says nothing about checkouts.
MACHINE="${SMIX_MACHINE_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/smix}"
FROZEN="$(mktemp -d)"
cp -R "$MACHINE/." "$FROZEN/" 2>/dev/null || true
read_in() { (cd "$1" && SMIX_MACHINE_DIR="$FROZEN" "$SMIX" lease list 2>/dev/null | sort | shasum | awk '{print $1}'); }
SEEN=0
DISAGREE=""
UNSTABLE=""
while read -r w; do
    [ -n "$w" ] && [ -d "$w" ] || continue
    SEEN=$((SEEN + 1))
    [ "$w" = "$ROOT" ] && continue
    judged=no
    for round in 1 2 3; do
        before="$(read_in "$ROOT")"; theirs="$(read_in "$w")"; after="$(read_in "$ROOT")"
        [ "$before" = "$after" ] || continue
        judged=yes
        echo "  $(basename "$w"): $theirs (this checkout: $before)"
        [ "$theirs" = "$before" ] || DISAGREE="$DISAGREE $(basename "$w")"
        break
    done
    [ "$judged" = yes ] || UNSTABLE="$UNSTABLE $(basename "$w")"
done <<< "$TREES"
rm -rf "$FROZEN"
if [ "$SEEN" -lt 2 ]; then
    bad "only $SEEN checkout found — one tree agreeing with itself proves nothing"
elif [ -n "$DISAGREE" ]; then
    bad "checkouts disagree about what this machine holds:$DISAGREE"
elif [ -n "$UNSTABLE" ]; then
    echo "  the ledgers changed under every round for:$UNSTABLE — cannot judge them"
    echo "C2-MACHINE-LEASE-SKIP"; exit 2
else
    ok "all $SEEN checkouts read the same ledgers"
fi

step "5. the device this script booted is on the books"
# Captured, then searched. `lease list | grep -q` reads as "not found"
# when it means "grep closed the pipe on its first match, the writer took
# SIGPIPE, and `pipefail` called the pipeline failed" — which is what the
# first run of this script reported, one step before `lease owner` found
# the very ledger it had just said was missing.
LIST="$("$SMIX" lease list 2>/dev/null)"
case "$LIST" in
    *"$IDLE"*) ok "$IDLE has a ledger" ;;
    *) bad "$IDLE was booted through smix and has no ledger" ;;
esac

step "6. from a checkout that never held it, ask who booted it"
# Any tree that is not this one. Named by discovery for the same reason
# as step 4.
OTHER="$(printf '%s\n' "$TREES" | grep -v "^$ROOT\$" | head -1)"
if [ -n "$OTHER" ] && [ -d "$OTHER" ]; then
    ( cd "$OTHER" && "$SMIX" lease owner "$IDLE" ) && rc=0 || rc=$?
    [ "$rc" = 0 ] && ok "answered from another tree (exit 0)" \
                  || bad "another tree cannot see who booted it (exit $rc)"
else
    echo "  SKIP: no second checkout on this machine"
fi

step "7. a booted device nobody recorded is nobody's"
# The step that separates "the ledgers are shared" from "simctl answers
# the same everywhere". BUSY is running right now; only a ledger can say
# it is not smix's.
if [ -n "$BUSY" ]; then
    ( cd "$OTHER" 2>/dev/null || cd "$ROOT"; "$SMIX" lease owner "$BUSY" ) && rc=0 || rc=$?
    [ "$rc" = 3 ] && ok "a booted device with no ledger reports as nobody's (exit 3)" \
                  || bad "expected exit 3 for an unrecorded booted device, got $rc"
else
    echo "  SKIP: no booted device without a ledger to ask about"
fi

step "8. the list comes from the ledgers, not from the devices"
EMPTY="$(mktemp -d)"
OUT="$(SMIX_MACHINE_DIR="$EMPTY" "$SMIX" lease list 2>/dev/null)"
rmdir "$EMPTY" 2>/dev/null || true
case "$OUT" in
    *"no device ledgers"*) ok "pointed at an empty machine dir, it lists nothing" ;;
    *) bad "an empty machine dir still listed something — this is reading the devices: $OUT" ;;
esac

step "9. a live holder is not something to reclaim"
LIST="$("$SMIX" lease list 2>/dev/null)"
# Every ledger on this machine whose holder is alive must read as held,
# not as abandoned. A `reconcile` acting on the other verdict would tear
# down somebody's session, and until the ledgers were shared only their
# own tree could make that mistake.
ABANDONED="$(printf '%s\n' "$LIST" | grep abandoned || true)"
if [ -z "$ABANDONED" ]; then
    ok "nothing on this machine is called abandoned"
else
    LIVE=""
    CHECKED=0
    UNASKED=""
    while read -r line; do
        [ -n "$line" ] || continue
        id="${line%%:*}"
        # `lease status` refuses a device nothing here has registered — a
        # ledger can outlive the device it was about. Under `set -e` that
        # refusal ended the script mid-section; it is a device this cannot
        # ask about, and is said as one.
        pid="$("$SMIX" lease status "$id" 2>/dev/null | sed -n 's/.*pid \([0-9][0-9]*\).*/\1/p' | head -1)" || pid=""
        if [ -z "$pid" ]; then UNASKED="$UNASKED $id"; continue; fi
        CHECKED=$((CHECKED + 1))
        state="$(ps -p "$pid" -o state= 2>/dev/null || true)"
        case "$state" in "") ;; Z*) ;; *) LIVE="$LIVE $id" ;; esac
    done <<< "$ABANDONED"
    [ -z "$UNASKED" ] || echo "  could not ask about:$UNASKED"
    if [ -n "$LIVE" ]; then
        bad "called abandoned while its holder is alive:$LIVE"
    elif [ "$CHECKED" = 0 ]; then
        ok "nothing called abandoned could be asked about, so this checked none of them"
    else
        ok "all $CHECKED asked about have a dead holder"
    fi
fi

echo
echo "=== $PASS passed, $FAIL failed ==="
[ "$FAIL" = 0 ]
