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

# Crates whose tests read a changed file from outside `crates/`.
#
# Several gates compile a document into themselves with `include_str!`
# and then check what it says. Narrowing by changed crates alone made
# those invisible to the exact edit they guard: touch only
# `docs/ai-guide/04-actions.md` and the gate that executes its examples
# never ran.
#
# Every tracked path, not just `docs/*`: the first version of this
# globbed `docs/` and the release-record gate reads `CHANGELOG.md` at
# the repo root, so editing only the changelog skipped the one check
# that reads it. A gate this list cannot see is a gate that runs
# everywhere except where it matters.
CHANGED_DOCS=$(
    {
        git diff --name-only "$BASE"...HEAD
        git diff --name-only
        git diff --name-only --cached
        git ls-files --others --exclude-standard
    } | grep -v '^crates/' | sort -u
)
for d in $CHANGED_DOCS; do
    # `|| true`: no crate reads most docs, and grep's "no match" exit 1
    # is fatal under `set -o pipefail`.
    readers=$(grep -rl "include_str!(\"[^\"]*$d\")" crates/*/src crates/*/tests 2>/dev/null |
        cut -d/ -f2 | sort -u || true)
    if [ -n "$readers" ]; then
        CRATES=$(printf '%s\n%s\n' "$CRATES" "$readers" | sort -u | sed '/^$/d')
    fi
done

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

echo "--- android: unit tests + androidTest compile"
# Unconditional, unlike the crate steps above. Those narrow by git diff
# because a whole-workspace clippy is too slow to be a habit; this is a
# second or two against a warm daemon, and narrowing it by changes under
# android-runner/ would reproduce the hole it was added to close. The
# three defects this gate exists for were cross-language contract breaks
# — a header sent from Rust that the Kotlin side never read — so the
# edit that breaks the Android assertion lands in crates/.
#
# assembleDebugAndroidTest compiles both androidTest source sets without
# running either. :app's is the runner body that ships to users, and no
# gate compiled it until now — the same species iOS logged twice as "the
# ship gate never compiles the runner it distributes".
#
# Compile only, and that is a DOWNGRADE with a name: instrumentation
# needs a device, preflight runs dozens of times a day, and booting an
# emulator here would contend with whatever the developer is doing. The
# device layer lives in ship.sh via android-instrumentation-gate.sh.
#
# Bare task names: a module added later is inside the gate on arrival.
( cd android-runner && ./gradlew testDebugUnitTest assembleDebugAndroidTest --console=plain )

echo "--- source gates"
for gate in hygiene-scan route-conformance fact-scan workflow-scan android-gate-scan audit-ledger-scan scope-promise-scan; do
    python3 "scripts/dev/$gate.py"
done

# The device guards decide what may touch a simulator or a phone. Their
# own judgement is source too.
for harness in scripts/dev/*-guard.test.sh; do
    bash "$harness"
done

# The AI tier is a judgement and the resolver is not; nothing about
# that separation is enforced by the type system. This ran in one
# checkpoint's acceptance block and then in nothing for the rest of the
# cycle.
bash scripts/dev/fence-check.sh
python3 scripts/dev/gen-llms.py --check

# The stress/smoke tier selector decides which corpus flows a tier runs;
# its subset invariant (smoke ⊆ all) is what keeps stress-gate from
# hand-maintaining a second list.
python3 scripts/release/stress-select.py --test

echo "preflight: clean"
