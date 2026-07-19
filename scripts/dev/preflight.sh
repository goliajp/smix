#!/usr/bin/env bash
# Run, locally, the checks CI will run — over exactly the crates you
# touched.
#
# CI clippies the whole workspace; local runs are per-crate because the
# workspace takes too long to be a habit. That gap shipped a clippy
# failure to CI: two collapsible-if lints in a crate whose tests were
# green locally, because the local clippy named three OTHER crates.
# Deriving the crate list from `git diff` closes it — you cannot forget
# a crate you edited.
#
# Usage: scripts/dev/preflight.sh [base-ref]   (default: origin/develop)
set -euo pipefail
cd "$(dirname "$0")/../.."

BASE="${1:-origin/develop}"

# Crates with changed files, derived — not maintained by hand.
#
# All four sources matter, and the first version of this script used
# only the first: run against a just-pushed branch it announced "no
# crate changes" and reported clean, having skipped the uncommitted fix
# it was written to check. A preflight that cannot see your working
# tree passes by knowing nothing.
CRATES=$(
    {
        git diff --name-only "$BASE"...HEAD -- 'crates/*' # committed
        git diff --name-only -- 'crates/*'                # unstaged
        git diff --name-only --cached -- 'crates/*'       # staged
        git ls-files --others --exclude-standard 'crates/*'
    } | cut -d/ -f2 | sort -u
)

if [ -z "$CRATES" ]; then
    echo "preflight: no crate changes vs $BASE"
else
    ARGS=""
    for c in $CRATES; do
        [ -d "crates/$c" ] && ARGS="$ARGS -p $c"
    done
    echo "preflight: crates $(echo "$CRATES" | tr '\n' ' ')"
    echo "--- fmt"
    cargo fmt --all --check
    echo "--- clippy (the step that caught CI out)"
    # No `-- -D warnings`: [workspace.lints] already denies warnings and
    # clippy::all, and CI's step is a bare `cargo clippy`. A local flag
    # CI does not pass would make this script disagree with the thing it
    # exists to predict.
    # -j 4: this machine shares its cores with other builds; a full-core
    # cargo here loses more time to contention than it saves.
    # shellcheck disable=SC2086
    cargo clippy -j 4 $ARGS --all-targets
    echo "--- test"
    # shellcheck disable=SC2086
    cargo test -j 4 $ARGS
fi

echo "--- source gates"
for gate in hygiene-scan route-conformance fact-scan; do
    python3 "scripts/dev/$gate.py"
done
python3 scripts/dev/gen-llms.py --check

echo "preflight: clean"
