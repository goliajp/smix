#!/usr/bin/env bash
# Print the UDID of a booted dev sim this machine's ledger says smix
# opened, or refuse.
#
# Scripts that need a device used to take the first booted sim they
# found. On a machine that also has a consumer's sim up, that is a coin
# toss over whose device the release gate drives — and it picked the
# wrong one, which is how this file came to exist.
#
# The first answer was a naming convention: `sim-smix-*` is ours,
# anything else is somebody's. That is a proxy for ownership, and on
# 2026-08-11 the proxy failed the same way the thing it replaced did.
# `sim-smix-02` was booted, matched the name, and was handed to the ship
# smoke gate, which started a runner on it and terminated the Safari
# somebody had running there. The name was right. The sim was not free.
#
# The header of this file used to say "there is no ownership registry to
# consult". There is one now — it is what v4.0 moved to the machine — so
# this asks it: a sim is eligible when a ledger records smix booting it.
# A sim somebody brought up by hand, or from Xcode, or from a tool that
# writes no ledger, has no such record and is left alone.
#
# That is stricter than the naming rule and deliberately so. The cost of
# refusing is a message telling you to boot it through smix; the cost of
# claiming is somebody's session.
#
# Usage:  UDID="$(bash scripts/dev/pick-dev-sim.sh)" || exit 1
# Exit:   0 with the UDID on stdout; 1 with a reason on stderr.
set -euo pipefail

PREFIX="${SMIX_DEV_SIM_PREFIX:-sim-smix-}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# Which smix answers the ownership question.
#
# `SMIX_BIN` is authoritative when set: falling through from a binary
# somebody named to a different one would answer a question they did not
# ask, and the answer decides whose simulator a gate drives.
#
# Otherwise the workspace builds before the PATH — a gate running in this
# tree should ask the smix it is testing, and an older one on the PATH
# may not have `lease owner` at all, which would read as "no sim is
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
    echo "pick-dev-sim: no smix here can answer who booted a device — \
\`lease owner\` arrived in 4.0. Build this workspace, or set SMIX_BIN." >&2
    echo "pick-dev-sim: refusing rather than falling back to the name, which \
is the proxy that handed a busy sim to a release gate." >&2
    exit 1
fi

BOOTED="$(xcrun simctl list devices -j | python3 -c '
import json, sys
prefix = sys.argv[1]
for runtime in json.load(sys.stdin)["devices"].values():
    for d in runtime:
        if d.get("state") == "Booted" and d.get("name", "").startswith(prefix):
            print(d["udid"], d["name"])
' "$PREFIX")"

if [ -z "$BOOTED" ]; then
    echo "pick-dev-sim: no booted sim named ${PREFIX}* — boot one with \
\`smix sim boot <UDID>\`, or pass the UDID explicitly" >&2
    exit 1
fi

# Of those, the ones a ledger says smix booted.
#
# `lease owner` exits 0 when a ledger records the boot, 3 when nothing
# does, 1 when the question could not be asked. Only 0 is eligible: 3 is
# precisely the state `sim-smix-02` was in, and 1 means this script does
# not know, which is not a licence.
ELIGIBLE=""
UNCLAIMED=""
while read -r udid name; do
    [ -n "$udid" ] || continue
    set +e
    "$SMIX" lease owner "$udid" >/dev/null 2>&1
    rc=$?
    set -e
    if [ "$rc" = 0 ]; then
        ELIGIBLE="$ELIGIBLE$udid $name
"
    else
        UNCLAIMED="$UNCLAIMED  $udid $name (lease owner exit $rc)
"
    fi
done <<EOF
$BOOTED
EOF

COUNT="$(printf '%s' "$ELIGIBLE" | grep -c . || true)"

if [ "$COUNT" -eq 0 ]; then
    echo "pick-dev-sim: booted ${PREFIX}* sims exist, and no ledger says smix \
booted any of them:" >&2
    printf '%s' "$UNCLAIMED" >&2
    echo "pick-dev-sim: a sim brought up by hand, by Xcode, or by anything \
that writes no ledger is somebody's — driving it is what took away a \
running Safari on 2026-08-11. Boot one through \`smix sim boot\`, or pass \
the UDID explicitly." >&2
    exit 1
fi
if [ "$COUNT" -gt 1 ]; then
    echo "pick-dev-sim: several booted ${PREFIX}* sims are smix's — name the \
one you mean:" >&2
    printf '%s' "$ELIGIBLE" >&2
    exit 1
fi

printf '%s\n' "$(printf '%s' "$ELIGIBLE" | head -1 | cut -d' ' -f1)"
