#!/usr/bin/env bash
# Print the UDID of this repo's own booted dev sim, or refuse.
#
# Scripts that need a device used to take the first booted sim they
# found. On a machine that also has a consumer's sim up, that is a
# coin toss over whose device the release gate drives — and it picked
# the wrong one, which is how this file came to exist.
#
# There is no ownership registry to consult, so the rule is the naming
# convention this repo creates its sims under: `sim-smix-*`. Anything
# else booted on the machine belongs to someone else. Ambiguity is an
# error rather than a choice: two of ours booted means the caller has
# to say which.
#
# Usage:  UDID="$(bash scripts/dev/pick-dev-sim.sh)" || exit 1
# Exit:   0 with the UDID on stdout; 1 with a reason on stderr.
set -euo pipefail

PREFIX="${SMIX_DEV_SIM_PREFIX:-sim-smix-}"

MATCHES="$(xcrun simctl list devices -j | python3 -c '
import json, sys
prefix = sys.argv[1]
devices = json.load(sys.stdin)["devices"]
for runtime in devices.values():
    for d in runtime:
        if d.get("state") == "Booted" and d.get("name", "").startswith(prefix):
            print(d["udid"], d["name"])
' "$PREFIX")"

COUNT="$(printf '%s' "$MATCHES" | grep -c . || true)"

if [[ "$COUNT" -eq 0 ]]; then
  echo "pick-dev-sim: no booted sim named ${PREFIX}* — boot one, or pass the UDID explicitly" >&2
  exit 1
fi
if [[ "$COUNT" -gt 1 ]]; then
  echo "pick-dev-sim: several booted ${PREFIX}* sims — name the one you mean:" >&2
  printf '%s\n' "$MATCHES" >&2
  exit 1
fi

printf '%s\n' "${MATCHES%% *}"
