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
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
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
DEVICES="$(xcrun simctl list devices -j)"
# BUSY first: booted, no ledger. IDLE second: not booted, no ledger.
# In that order, so the two can never be the same device.
BUSY=""
IDLE=""
while read -r udid state; do
    has_ledger "$udid" && continue
    if [ "$state" = "Booted" ] && [ -z "$BUSY" ]; then BUSY="$udid"; fi
    if [ "$state" = "Shutdown" ] && [ -z "$IDLE" ]; then IDLE="$udid"; fi
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
    echo "no unbooted simulator without a ledger — cannot run"
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
FIRST=""
SAME=yes
SEEN=0
while read -r w; do
    [ -n "$w" ] && [ -d "$w" ] || continue
    got="$(cd "$w" && "$SMIX" lease list 2>/dev/null | sort | shasum | awk '{print $1}')"
    echo "  $(basename "$w"): $got"
    SEEN=$((SEEN + 1))
    if [ -z "$FIRST" ]; then FIRST="$got"; elif [ "$got" != "$FIRST" ]; then SAME=no; fi
done <<< "$TREES"
if [ "$SEEN" -lt 2 ]; then
    bad "only $SEEN checkout found — one tree agreeing with itself proves nothing"
elif [ "$SAME" = yes ]; then
    ok "all $SEEN checkouts read the same ledgers"
else
    bad "checkouts disagree about what this machine holds"
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
    while read -r line; do
        [ -n "$line" ] || continue
        id="${line%%:*}"
        pid="$("$SMIX" lease status "$id" 2>/dev/null | sed -n 's/.*pid \([0-9][0-9]*\).*/\1/p' | head -1)"
        [ -n "$pid" ] || continue
        state="$(ps -p "$pid" -o state= 2>/dev/null || true)"
        case "$state" in "") ;; Z*) ;; *) LIVE="$LIVE $id" ;; esac
    done <<< "$ABANDONED"
    [ -z "$LIVE" ] && ok "everything called abandoned has a dead holder" \
                   || bad "called abandoned while its holder is alive:$LIVE"
fi

echo
echo "=== $PASS passed, $FAIL failed ==="
[ "$FAIL" = 0 ]
