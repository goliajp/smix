#!/usr/bin/env bash
# Print the serial of a running Android emulator this machine's ledger
# says smix booted, or refuse.
#
# The Android counterpart of pick-dev-sim.sh, and the same rule for the
# same reason. Two release gates and five e2e scripts here took "the
# first emulator-* adb lists", and one day another person driving smix
# on this machine had an emulator on that port — so the gates drove
# theirs, and six smoke scripts defaulting to emulator-5554 stopped it
# on teardown. Nobody did anything wrong. Nothing asked whose it was.
#
# Eligible = adb lists it as `device` AND `smix lease owner <serial>`
# exits 0, meaning a ledger says smix booted it OR somebody said out loud
# that this machine answers for it. An emulator started by hand, from
# Android Studio, or from a tool that writes no ledger has neither and is
# left alone. The cost of refusing is a message telling you what to run;
# the cost of claiming is somebody's session.
#
# The second half of that used to be missing, and the gap had a shape:
# this machine's own dedicated AVD, running, started by a hand that wrote
# no ledger, was drivable by nobody. The way through was
# SMIX_ANDROID_SERIAL, which records nothing and is gone when the command
# exits — so every release made the same decision again and none of them
# could be read afterwards. `smix lease claim <serial>` is that decision
# with somewhere to live. This script did not change to gain it.
#
# Usage:  SERIAL="$(bash scripts/dev/pick-dev-emulator.sh)" || exit 1
# Exit:   0 with the serial on stdout; 1 with a reason on stderr.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# Which smix answers the ownership question. `SMIX_BIN` is authoritative
# when set; otherwise the workspace builds before the PATH, because a
# gate in this tree should ask the smix it is testing, and an older one
# on the PATH may lack `lease owner` — which would read as "nothing is
# eligible" rather than "this binary cannot answer".
SMIX=""
if [ -n "${SMIX_BIN:-}" ]; then
    if [ -x "$SMIX_BIN" ] && "$SMIX_BIN" lease owner --help >/dev/null 2>&1; then
        SMIX="$SMIX_BIN"
    fi
else
    for candidate in "$ROOT/target/release/smix" "$ROOT/target/debug/smix" \
                     "$(command -v smix 2>/dev/null || true)"; do
        [ -n "$candidate" ] && [ -x "$candidate" ] || continue
        if "$candidate" lease owner --help >/dev/null 2>&1; then
            SMIX="$candidate"
            break
        fi
    done
fi

if [ -z "$SMIX" ]; then
    echo "pick-dev-emulator: no smix here can answer who booted a device — \
build this workspace, or set SMIX_BIN." >&2
    echo "pick-dev-emulator: refusing rather than taking the first emulator adb \
lists, which is the proxy that handed a release gate somebody else's device." >&2
    exit 1
fi

command -v adb >/dev/null 2>&1 || {
    echo "pick-dev-emulator: no adb on PATH — this needs the Android SDK" >&2
    exit 1
}

RUNNING="$(adb devices 2>/dev/null | awk '/^emulator-[0-9]+[[:space:]]+device$/ { print $1 }')"
if [ -z "$RUNNING" ]; then
    echo "pick-dev-emulator: adb lists no running emulator — boot one with \
\`smix sim boot <alias>\`, or pass the serial explicitly" >&2
    exit 1
fi

# Of those, the ones a ledger says smix booted. `lease owner` exits 0
# when a ledger records the boot, non-zero otherwise. Only 0 is eligible:
# "no ledger" is precisely the state a hand-started emulator is in, and
# "cannot answer" is not a licence.
ELIGIBLE=""
UNCLAIMED=""
while read -r serial; do
    [ -n "$serial" ] || continue
    set +e
    "$SMIX" lease owner "$serial" >/dev/null 2>&1
    rc=$?
    set -e
    if [ "$rc" = 0 ]; then
        ELIGIBLE="$ELIGIBLE$serial
"
    else
        UNCLAIMED="$UNCLAIMED  $serial (lease owner exit $rc)
"
    fi
done <<EOF2
$RUNNING
EOF2

COUNT="$(printf '%s' "$ELIGIBLE" | grep -c . || true)"
case "$COUNT" in
    1)
        printf '%s\n' "$ELIGIBLE" | head -1
        ;;
    0)
        echo "pick-dev-emulator: emulators are running and none has a ledger \
saying smix booted it:" >&2
        printf '%s' "$UNCLAIMED" >&2
        echo "pick-dev-emulator: an emulator started by hand is not smix's to \
drive. Either boot one through smix — \`smix sim boot <alias>\` — or, if one \
of the above is this machine's and nobody is using it, say so once and it \
stays said: \`smix lease claim <serial>\`. That grants driving and not \
shutting down, and the ledger ends it when the device goes off." >&2
        exit 1
        ;;
    *)
        echo "pick-dev-emulator: $COUNT emulators are smix-booted and this script \
does not choose between them — pass the serial explicitly:" >&2
        printf '%s' "$ELIGIBLE" | sed 's/^/  /' >&2
        exit 1
        ;;
esac
