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
# `|| true` on the grep: a perfectly clean tree at $BASE feeds it
# nothing, and grep answers no-input with exit 1 — fatal under
# pipefail, and silently so under `set -e`, before a single line of
# output. The readers loop below already knew this about grep; this
# pipeline learned it the day the branch was merged and the tree was
# clean for the first time in the script's life.
CHANGED_DOCS=$(
    {
        git diff --name-only "$BASE"...HEAD
        git diff --name-only
        git diff --name-only --cached
        git ls-files --others --exclude-standard
    } | { grep -v '^crates/' || true; } | sort -u
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

# Crates that embed a copy of a whole tree from outside `crates/`.
#
# The rule above finds a crate that reads a changed *file* by name.
# `smix-runner-sources` embeds a tarball of `swift-bridge/` and
# `android-runner/` and compares it against them, and it names only the
# two directories — so bumping the version, which edits a README and a
# Kotlin constant inside those trees, matched nothing and the test that
# exists to catch a stale tarball did not run. Preflight said clean; the
# ship found both tarballs stale an hour later. A consumer would have
# had `smix runner up` build the old Swift and report success.
#
# The directory is matched as the tail of a string literal, so
# `join("../../swift-bridge")` selects it and `"docs/ai-guide/x.md"`
# does not select every crate that says docs.
for d in $CHANGED_DOCS; do
    case "$d" in */*) tree=${d%%/*} ;; *) continue ;; esac
    # `|| true`: grep answers no-match with exit 1, fatal under pipefail.
    embedders=$(grep -rl "\"[^\"]*/$tree\"" crates/*/src crates/*/tests 2>/dev/null |
        cut -d/ -f2 | sort -u || true)
    if [ -n "$embedders" ]; then
        CRATES=$(printf '%s\n%s\n' "$CRATES" "$embedders" | sort -u | sed '/^$/d')
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

# The Swift suite, when the Swift sources moved.
#
# CI runs it and this did not, so changing a handler's return type in
# `SmixRunnerServer` and not updating the one test that stubs that
# handler passed preflight clean and would have failed CI. The header of
# this file says it runs the checks CI will run; for two steps it was
# not true.
#
# Narrowed by changed files like the crate steps above, and for the same
# reason — a cold `swift test` is too slow to be a habit — using the
# same four sources, so an uncommitted edit counts.
if printf '%s\n' "$CHANGED_DOCS" | grep -q '^swift-bridge/'; then
    echo "--- swift test"
    (cd swift-bridge && swift test)
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
# One per line, in the order they run.
#
# This was a single line of 1043 characters. A gate added to it
# needed horizontal scrolling to see, and a gate removed from it
# left almost no trace in a diff — a poor place to keep the list of
# everything that has to be true.
SOURCE_GATES=(
  hygiene-scan
  route-conformance
  every-verb-reads-a-locale-map
  fact-scan
  workflow-scan
  android-gate-scan
  audit-ledger-scan
  scope-promise-scan
  release-record-scan
  guide-claims-scan
  gate-subject-diversity
  route-context-scan
  gate-port-scan
  preflight-parity-scan
  mcp-cli-parity-scan
  known-unstable-scan
  corpus-portability-scan
  portable-tier-parity
  yield-is-not-failure-scan
  teardown-restores-scan
  android-subject-scan
  device-facts-are-machine-scoped
  leases-are-machine-scoped
  no-second-ledger-path
  retired-claims-scan
  contract-scan
  selector-surface-scan
  health-is-not-a-session-check
  probes-name-the-app
  tap-then-capture-is-one-path
  every-flag-says-what-it-does
  an-authorised-hatch-reaches-every-surface
  a-tap-proves-aim-not-arrival
  v5.1-c10-ground-truth-is-complete
  no-script-picks-a-device-by-accident
  generated-artifacts-are-load-bearing
  project-pointer-holds-no-facts
  publish-dag-is-complete
  publish-dag-is-complete.test
  actions-are-pinned
  actions-are-pinned.test
  jobs-have-a-ceiling
  jobs-have-a-ceiling.test
  a-selftest-nobody-runs
  a-selftest-nobody-runs.test
  verify-published-reads-registries.test
  cheap-gates-come-first.test
  a-published-crate-can-run-its-tests
  a-verb-does-not-assume-a-platform
  a-hand-copied-table-says-a-number
)

for gate in "${SOURCE_GATES[@]}"; do
    python3 "scripts/dev/$gate.py"
done

# Takes a flag, so it cannot ride the loop above. Its own docstring calls
# `--check` "the gate", and until 2026-08-06 nothing ran it — a check
# that never runs reads as coverage while providing none, which is worse
# than not having written it: the guides could drift from the corpus the
# tests actually execute, and the drift would be invisible precisely
# because a gate appeared to be watching.
python3 scripts/dev/guide-corpus-sync.py --check

# The device guards decide what may touch a simulator or a phone. Their
# own judgement is source too.
for harness in scripts/dev/*-guard.test.sh; do
    bash "$harness"
done

# So does the picker: it decides which simulator a release gate is
# handed, and it answered from a naming convention until that convention
# handed away a sim somebody was using.
bash scripts/dev/pick-dev-sim.test.sh
# And what the guards are allowed to judge in the first place: a body
# being written down is not a command being run.
python3 scripts/dev/hook-command.test.py
# And that the retired-wording scan can still go red. Its subject is a
# sentence nobody will notice: `llms.txt` opened with "never a physical
# device" for two majors after §9 #1 stopped saying so, past every gate
# there was, because no gate knew a rule had a date.
python3 scripts/dev/retired-claims-scan.test.py
# And that the contract gate can tell a claimed gap from a lost one. Its
# whole point is one file's worth of difference between the two, so a
# harness is the only thing that can show it still reads that difference.
python3 scripts/dev/contract-scan.test.py
# And that the selector-surface gate can still go red. Its subject is an
# axis nothing watched: `point` worked in flows and four SDKs and was
# missing from MCP and the CLI for two majors, with every gate green.
python3 scripts/dev/selector-surface-scan.test.py
# And that the health/session gate can still go red. Its subject is a
# question two commands answered with a different question: /health says
# the server answers and nothing about the app binding.
python3 scripts/dev/health-is-not-a-session-check.test.py
# And that the probe-naming gate can still go red. Its subject is the
# half of the session question C5 left open: asking without naming an app
# answers about whichever one the runner was bound to at startup.
python3 scripts/dev/probes-name-the-app.test.py
# And that the one-path gate can still go red. Its subject is two
# sentences no behaviour test can see: the combined action has one
# implementation, and the frame comes from the process that tapped.
python3 scripts/dev/tap-then-capture-is-one-path.test.py
# And that the flag-description gate can still go red. A consumer read a
# help text where one flag carried another's sentence and the second
# carried nothing; fifteen more were the same and nobody was looking.
python3 scripts/dev/every-flag-says-what-it-does.test.py
python3 scripts/dev/an-authorised-hatch-reaches-every-surface.test.py
python3 scripts/dev/a-tap-proves-aim-not-arrival.test.py
python3 scripts/dev/v5.1-c10-ground-truth-is-complete.test.py
python3 scripts/dev/no-script-picks-a-device-by-accident.test.py
python3 scripts/dev/generated-artifacts-are-load-bearing.test.py
python3 scripts/dev/project-pointer-holds-no-facts.test.py

# The AI tier is a judgement and the resolver is not; nothing about
# that separation is enforced by the type system. This ran in one
# checkpoint's acceptance block and then in nothing for the rest of the
# cycle.
bash scripts/dev/fence-check.sh
python3 scripts/dev/gen-llms.py --check

# The napi loader (index.d.ts / index.js) is auto-generated and committed,
# so it drifts from the crate between ships. Before 6.0.0, swipeAtCoord
# reached the Rust bindings but not the .d.ts. Regenerate and diff here —
# host-side, no device, unlike the uniffi bindings gate which is ship-only.
bash scripts/dev/napi-dts-fresh.sh

# The stress/smoke tier selector decides which corpus flows a tier runs;
# its subset invariant (smoke ⊆ all) is what keeps stress-gate from
# hand-maintaining a second list.
python3 scripts/release/stress-select.py --test
bash scripts/release/stress-gate.sh --selftest

# The corpus gate's verdict — the part with a decision in it. FLAKE is
# not green, and a rule reachable only by booting a simulator is a rule
# nobody checks.
python3 scripts/dev/flake-classify.test.py
bash scripts/release/corpus-gate.sh --selftest
bash scripts/dev/v3.0-c3-determinism.sh --selftest
bash scripts/release/portable-tier.sh --selftest
# The site check's own judgement: it has to recognise the SPA fallback,
# because that is what a missing file answers with.
bash scripts/release/site-is-current.sh --selftest

# The Claude Code plugin. Both are device-free and take under a second;
# what they cover is a plugin that parses perfectly and does nothing —
# a manifest pointing at a directory that moved, a hook that is not
# executable, or components filed inside `.claude-plugin/` where they are
# silently not loaded.
python3 scripts/dev/plugin-structure.test.py
bash scripts/dev/plugin-readiness.test.sh
# The invariant the plugin is built on: it adds initiative, not
# capability. A skill naming a command the CLI does not have would be
# documenting something that only exists inside Claude Code.
python3 scripts/dev/plugin-capability-parity.test.py
bash scripts/dev/plugin-monitor.test.sh

# The device e2e scripts (record/propose/federation) each need a booted
# sim or emulator, so preflight — device-free and run dozens of times a
# day — only runs them when SMIX_DEVICE_E2E is set. The glob wires every
# `*-e2e.sh` into a gate so workflow-scan reads them as run rather than
# orphaned, without a hand-kept list that would drift as checkpoints add
# scripts.
if [[ -n "${SMIX_DEVICE_E2E:-}" ]]; then
    for e2e in scripts/dev/*-e2e.sh; do
        bash "$e2e"
    done
fi

echo "preflight: clean"
