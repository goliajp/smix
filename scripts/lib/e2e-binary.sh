#!/usr/bin/env bash
# The binary an end-to-end script drives, resolved in one place.
#
# Every script used to spell this itself, and they did not agree: most
# took `target/debug/smix`, two took `target/release/smix`. That is fine
# until a checkpoint's two halves disagree — C12 added `--reader` to the
# debug binary, the gate the script called reached for the release one,
# and the run ended with exit 1 and NO OUTPUT AT ALL, because `set -e`
# killed the script inside a command substitution before a single
# verdict printed. A leg that never ran and a leg that passed had looked
# the same for as long as the defaults differed.
#
# Source this, do not run it: it exports into the caller's environment.
#
# An inherited `SMIX_BIN` wins — a lane driving a release build, or a
# session pointing at an installed binary, said so deliberately.
#
# Missing is a FAILURE, not a reason to skip: the binary is something
# this run owns. A machine with no simulator cannot judge; a checkout
# nobody built is a checkout nobody built.

SMIX="${SMIX_BIN:-${SMIX_E2E_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}/target/debug/smix}"

if [[ ! -x "$SMIX" ]]; then
    printf 'e2e: no smix binary at %s — build it (cargo build), or set SMIX_BIN\n' \
        "$SMIX" >&2
    exit 1
fi

export SMIX
export SMIX_BIN="$SMIX"

# The MCP server, from the same build as the CLI beside it.
#
# One script drove a debug `smix` and a release `smix-mcp` in the same
# session — the two halves of one checkpoint, built at different times
# from different sources. Its own existence check stays where it is:
# most scripts never start an MCP server, and a missing one is only a
# failure for the scripts that do.
SMIX_MCP="${SMIX_MCP_BIN:-$(dirname "$SMIX")/smix-mcp}"
export SMIX_MCP
export SMIX_MCP_BIN="$SMIX_MCP"


# `smix run`, judged by the code smix reported.
# shellcheck source=scripts/lib/judged-run.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/judged-run.sh"
