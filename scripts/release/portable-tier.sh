#!/usr/bin/env bash
# The corpus flows that could run on a machine that is not the author's.
#
# Twenty of the twenty-four corpus flows name system-app identifiers that
# differ by iOS version and device model — `com.apple.settings.siri` and
# the like. On a CI runner those go red for reasons that say nothing
# about smix, and a gate whose red means nothing gets skipped. This runs
# the rest: the ones driving the fixture app, which ships in this
# repository and is compiled by the gate itself.
#
# The flow list is DERIVED, not written down here. Four separate places
# in this cycle kept their own copy of a list — which routes read a
# header, which modifiers block a fast path, which selector forms carry
# an index — and every one of them had drifted from what it was a copy
# of. `corpus-portability-scan.py --list` is the single answer to "which
# flows are portable", and this asks it.
#
# Usage:
#   SMIX_CORPUS_SIM=<UDID> SMIX_BIN=<path> bash scripts/release/portable-tier.sh
#   bash scripts/release/portable-tier.sh --selftest
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CORPUS="$ROOT/scripts/release/stress-corpus"

portable_flows() {
    python3 "$ROOT/scripts/dev/corpus-portability-scan.py" --list
}

if [ "${1:-}" = "--selftest" ]; then
    # No device: check that the derivation answers, and answers with
    # flows that exist. A list that came back empty would make this
    # script "pass" by running nothing, which is the failure mode the
    # rest of this cycle kept finding — silence read as success.
    flows="$(portable_flows)"
    count="$(printf '%s\n' "$flows" | grep -c . || true)"
    if [ "$count" -lt 1 ]; then
        echo "portable-tier selftest: FAIL — the scan named no portable flows" >&2
        exit 1
    fi
    missing=0
    for f in $flows; do
        [ -f "$CORPUS/$f.yaml" ] || { echo "portable-tier selftest: FAIL — $f has no yaml" >&2; missing=1; }
    done
    [ "$missing" = 0 ] || exit 1
    echo "portable-tier selftest: $count flow(s), all present: $(echo "$flows" | tr '\n' ' ')"
    exit 0
fi

: "${SMIX_CORPUS_SIM:?set SMIX_CORPUS_SIM to the simulator to drive}"

# This script had no binary of its own — it passed SMIX_BIN through to
# the corpus gate and never ran smix itself. The bootstrap below does,
# so it resolves one the same way the gate does.
SMIX_BIN="${SMIX_BIN:-$(command -v smix)}"
[ -n "$SMIX_BIN" ] || { echo "error: smix binary not on PATH (set SMIX_BIN)" >&2; exit 2; }

# A `.smix` workspace, if this checkout has none.
#
# `runner up` resolves device refs against the registry in `.smix/`, and
# that directory is runtime state — it is not in git, so a fresh
# checkout has none and the first CI run of this tier died with "no
# .smix/ workspace found upward from ...". Everything before it had
# worked: the simulator booted, the fixture compiled and installed.
#
# `smix init` is the bootstrap for exactly this, and it is idempotent
# enough to run when the directory is already there. Done here rather
# than in the workflow so that the local and CI paths stay the same one
# — the parity gate exists because they drift otherwise.
if [ ! -d "$ROOT/.smix" ]; then
    echo "portable tier: no .smix workspace — initialising for $SMIX_CORPUS_SIM"
    ( cd "$ROOT" && "$SMIX_BIN" init --device "$SMIX_CORPUS_SIM" ) \
        || { echo "error: smix init failed" >&2; exit 3; }
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

flows="$(portable_flows)"
[ -n "$flows" ] || { echo "error: no portable flows" >&2; exit 2; }
for f in $flows; do
    cp "$CORPUS/$f.yaml" "$WORK/"
done
# The excuse list travels with them: a flow excused in the full corpus is
# excused here too, or the same flake would mean different things
# depending on which tier ran it.
[ -f "$CORPUS/known-unstable.md" ] && cp "$CORPUS/known-unstable.md" "$WORK/"

echo "portable tier: $(echo "$flows" | tr '\n' ' ')"
# The runner boots on the fixture rather than Preferences: every flow
# here drives the fixture, and booting on an app none of them name would
# make each one rebind on its first request for no reason.
SMIX_CORPUS_BUNDLE="${SMIX_CORPUS_BUNDLE:-jp.golia.smix.fixture}" \
    bash "$ROOT/scripts/release/corpus-gate.sh" --corpus-dir="$WORK"
