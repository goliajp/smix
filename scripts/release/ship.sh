#!/usr/bin/env bash
# smix release ship script.
#
# Runs `scripts/release/smoke-v1.smoke.sh` as a hard gate, then
# publishes the release across all four ecosystems in the tested DAG
# order. Refuses to publish if the smoke gate hasn't passed in the
# last hour.
#
# Usage:
#   scripts/release/ship.sh 1.0.5
#   scripts/release/ship.sh 1.0.5 --i-know-what-im-doing   # bypass smoke gate
#
# Requires (see individual publish steps):
#   - CARGO_REGISTRY_TOKEN or `cargo login` state
#   - `npm login`
#   - ~/.gradle/gradle.properties with mavenCentral* + GPG key
#   - git remote origin with push access

set -euo pipefail

VERSION="${1:-}"
BYPASS="${2:-}"

[[ -n "$VERSION" ]] || { echo "usage: ship.sh <version> [--i-know-what-im-doing]"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMOKE="$ROOT/scripts/release/smoke-v1.smoke.sh"
STAMP="$ROOT/.smoke-passed-at"

# Each gate's name, and how long the one before it took.
#
# 6.3.0 died four times on second-level judgements sitting at gate 50 of
# 53 — a stale version, an undefined variable, three platform packages
# left behind, an empty gpg key — each costing ninety minutes to reach.
# Moving them to the front fixed that release; nothing records the cost
# of the ones that remain, so the next gate added in the wrong place is
# invisible until it is paid for.
#
# The profile is written beside the log, one line per gate, so an
# ordering that has drifted can be read rather than remembered.
SHIP_T0="$(date +%s)"
SHIP_TPREV="$SHIP_T0"
SHIP_PROFILE="${SHIP_PROFILE:-/tmp/smix-ship-profile.tsv}"
: > "$SHIP_PROFILE"
SHIP_LAST=""
log() {
  local now elapsed since
  now="$(date +%s)"
  if [[ -n "$SHIP_LAST" ]]; then
    printf '%s\t%s\n' "$(( now - SHIP_TPREV ))" "$SHIP_LAST" >> "$SHIP_PROFILE"
  fi
  SHIP_TPREV="$now"
  SHIP_LAST="$*"
  elapsed=$(( now - SHIP_T0 ))
  printf '[ship] %3dm%02ds  %s\n' $(( elapsed / 60 )) $(( elapsed % 60 )) "$*"
}
# A line that reports rather than judges.
#
# It prints like a gate and must not be timed like one: a report costs
# nothing and, by definition, comes after the work it describes. The
# ordering check reads the profile and would otherwise see four
# zero-second entries sitting behind seventeen minutes and call each of
# them a gate in the wrong place — which is what it did on its first
# contact with a real profile.
note() {
  local elapsed=$(( $(date +%s) - SHIP_T0 ))
  printf '[ship] %3dm%02ds  %s\n' $(( elapsed / 60 )) $(( elapsed % 60 )) "$*"
}
# The last gate has no successor to close it out, so the summary does.
ship_profile_close() {
  [[ -n "$SHIP_LAST" ]] && printf '%s\t%s\n' "$(( $(date +%s) - SHIP_TPREV ))" "$SHIP_LAST" >> "$SHIP_PROFILE"
}
trap ship_profile_close EXIT
fail() { printf '[ship] FAIL: %s\n' "$*" >&2; exit 1; }

# --- pre-flight -------------------------------------------------------

if [[ "$BYPASS" != "--i-know-what-im-doing" ]]; then
  # Require smoke pass in the last hour.
  if [[ ! -f "$STAMP" ]] || \
     [[ $(( $(date +%s) - $(stat -f %m "$STAMP" 2>/dev/null || echo 0) )) -gt 3600 ]]; then
    log "smoke gate stale or missing — running smoke first"
    "$SMOKE" || fail "smoke gate FAILED — refusing to publish"
    touch "$STAMP"
  else
    log "smoke gate stamp fresh (< 1 h) — skipping re-run"
  fi
else
  log "WARNING: bypass smoke gate via --i-know-what-im-doing"
fi

# --- version match ---------------------------------------------------

WORKSPACE_VERSION="$(grep '^version' "$ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')"
[[ "$WORKSPACE_VERSION" == "$VERSION" ]] \
  || fail "workspace Cargo.toml version=$WORKSPACE_VERSION doesn't match arg $VERSION"

# python3, not `node -p`: under nvm, `node` is a shell function that only
# exists in an interactive shell, so a ship started from a script or a
# non-interactive context died here with "node: command not found" after
# every gate had already passed. python3 is what the rest of this script
# already relies on.
NPM_VERSION="$(python3 -c 'import json;print(json.load(open("'"$ROOT"'/npm/smix-rn/package.json"))["version"])')"
[[ "$NPM_VERSION" == "$VERSION" ]] \
  || fail "npm package.json version=$NPM_VERSION doesn't match arg $VERSION"

# v1.0.26 — Android side version gates. Two spots historically drifted:
#   1. android-runner Kotlin runner VERSION (froze at v6.0-c3b for
#      multiple releases while the workspace advanced — /health lied).
#   2. android-runner/sdk gradle mavenCentralVersion.
KOTLIN_RUNNER_VERSION="$(grep 'const val VERSION' "$ROOT/android-runner/app/src/main/kotlin/dev/smix/runner/SmixRunner.kt" | sed 's/.*"\(.*\)".*/\1/')"
[[ "$KOTLIN_RUNNER_VERSION" == "$VERSION" ]] \
  || fail "android-runner SmixRunner.VERSION=$KOTLIN_RUNNER_VERSION doesn't match arg $VERSION (bump android-runner/app/src/main/kotlin/dev/smix/runner/SmixRunner.kt)"

GRADLE_VERSION="$(grep 'val mavenCentralVersion' "$ROOT/android-runner/sdk/build.gradle.kts" | sed 's/.*"\(.*\)".*/\1/')"
[[ "$GRADLE_VERSION" == "$VERSION" ]] \
  || fail "android-runner sdk mavenCentralVersion=$GRADLE_VERSION doesn't match arg $VERSION"

# v1.0.26 — README install snippet shows the current gradle release
# coordinate; gate it so it can't silently go stale across releases.
README_GRADLE_VERSION="$(grep 'jp.golia.smix:smix-sdk:' "$ROOT/README.md" | sed 's/.*smix-sdk:\([0-9.]*\).*/\1/' | head -1)"
[[ "$README_GRADLE_VERSION" == "$VERSION" ]] \
  || fail "README.md gradle coordinate=$README_GRADLE_VERSION doesn't match arg $VERSION (update the Install section)"

# The CLI's three per-platform packages are hand-written files, unlike
# the napi ones that `create-npm-dirs` regenerates from the crate version
# every run. 6.3.0 walked into the difference: the parent was bumped and
# already listed 6.3.0 in optionalDependencies while all three platform
# packages still said 6.2.0, so the publish tried to overwrite 6.2.0 and
# npm refused — after four packages had already gone out. Left unnoticed
# it is worse than a failed publish: a parent that resolves to platform
# versions nobody published installs as nothing at all.
for cli_pkg in "$ROOT/npm/smix-cli/package.json" \
               "$ROOT/npm/smix-cli/npm"/*/package.json; do
  cli_pkg_version="$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['version'])" "$cli_pkg")"
  [ "$cli_pkg_version" = "$VERSION" ] \
    || fail "$cli_pkg is version $cli_pkg_version, not $VERSION — bump it with the rest"
done
cli_opt_mismatch="$(python3 -c "
import json,sys
d = json.load(open(sys.argv[1]))
want = sys.argv[2]
print(' '.join(f'{k}@{v}' for k, v in d.get('optionalDependencies', {}).items() if v != want))
" "$ROOT/npm/smix-cli/package.json" "$VERSION")"
[ -z "$cli_opt_mismatch" ] \
  || fail "npm/smix-cli optionalDependencies point at $cli_opt_mismatch, not $VERSION"

SHIP_DRY="${SMIX_SHIP_DRYRUN:-0}"

# --- npm write preflight ---------------------------------------------

# Nine npm packages go out after crates.io, and crates.io cannot be
# unpublished. `npm whoami` is not the predicate that matters: 6.3.0 had a
# working whoami and still stopped dead at the first publish with EOTP,
# by which point all 30 crates were already out. Read permission is not
# write permission, so this asks the registry for a real write before the
# first crate goes out.
#
# The write is `latest` set to the version it already holds, sent as a raw
# PUT. Two earlier shapes of this check were wrong and both are worth
# remembering. A throwaway tag writes fine but cannot be cleaned up: the
# token can PUT a dist-tag and not DELETE one (403), so every run would
# leave another tag nothing can remove. Going through `npm dist-tag add`
# writes nothing at all when the tag already holds that version — it says
# "already set" and skips the request, which is green on a token that
# cannot publish. curl does not short-circuit, so the PUT is always real,
# and writing the value back over itself needs no cleanup.
if [ "$SHIP_DRY" != 1 ]; then
  log "npm write preflight (PUT latest over itself)"
  PREFLIGHT_PKG="@goliapkg/smix"
  PREFLIGHT_VER="$(npm view "$PREFLIGHT_PKG" version 2>/dev/null)"
  [ -n "$PREFLIGHT_VER" ] \
    || fail "npm write preflight: cannot read $PREFLIGHT_PKG from the registry"
  PREFLIGHT_TOKEN="$(grep -m1 '_authToken=' "$HOME/.npmrc" 2>/dev/null | sed -E 's/.*_authToken=//' || true)"
  [ -n "$PREFLIGHT_TOKEN" ] \
    || fail "npm write preflight: no //registry.npmjs.org/:_authToken= in ~/.npmrc"
  PREFLIGHT_CODE="$(curl -s -o /dev/null -w '%{http_code}' -X PUT \
    -H "Authorization: Bearer $PREFLIGHT_TOKEN" \
    -H 'Content-Type: application/json' \
    --data "\"$PREFLIGHT_VER\"" \
    "https://registry.npmjs.org/-/package/@goliapkg%2fsmix/dist-tags/latest")"
  case "$PREFLIGHT_CODE" in
    2*) log "  npm accepts a write without prompting (latest still $PREFLIGHT_VER)" ;;
    *)  fail "npm write preflight: the registry answered $PREFLIGHT_CODE to a dist-tag write. The token in ~/.npmrc cannot publish without a human — an EOTP token asks for a one-time password on every write. Generate one that bypasses 2FA at https://www.npmjs.com/settings/goliapanda/tokens and set it as //registry.npmjs.org/:_authToken= in ~/.npmrc" ;;
  esac
fi

# Everything above this line reads files and makes one HTTP request; it
# finishes in seconds. It used to sit at gate 50 of 53, behind an hour of
# `cargo test`, a release build and two device suites — so 6.3.0 spent
# four separate 90-minute runs discovering, at the very end, that a
# version string was stale or a token could not write. A check that costs
# a second belongs before the ones that cost an hour.

# --- the seconds-long judgements, before anything expensive --------
#
# Moved here from between gate 40 and gate 55, where the profile found
# them: napi loader (0s) sat behind six minutes, and fact scan,
# llms.txt freshness, fence check and clippy (0-3s) behind thirteen.
# The napi one is not hypothetical — it went red forty minutes into
# both 6.4.0 and 6.5.0, over a version string embedded in a generated
# file, and costs nothing to ask.
#
# They were placed correctly once, by hand, at 6.3.0. Nothing kept
# them there; the ordering check does now.


# --- fact scan --------------------------------------------------------
# hygiene-scan asks "does it read as internal?"; fact-scan asks "is it
# true?" — install coordinates vs the workspace version, tool-count
# claims vs #[tool(] registrations, and noise inside the quoted strings
# hygiene-scan structurally cannot see.
log "fact scan"
python3 "$ROOT/scripts/dev/fact-scan.py" > /tmp/smix-ship-facts.log 2>&1 \
  || fail "fact-scan FAILED — a user-facing surface states something untrue (see /tmp/smix-ship-facts.log)"
# renames, which the version bump covers.

# --- llms.txt freshness ----------------------------------------------
# llms.txt / llms-full.txt are a projection of VERB_TABLE + the Selector
# enum + the workspace version. Gate them like the FFI bindings so the
# AI-facing index can't drift from the sources it mirrors.
log "llms.txt freshness"
# Confirms the crates' API changes are the major break the 2.0.0 bump
# claims. Runs when the tool is installed; a ship must have it. It
# validates in-place breaks like SimctlError → DeviceControlError, not
# The AI tier sits beside the resolver, not inside it. Nothing in the
# type system says so, and the check that does say so was running in no
# gate at all when this line was added.
log "fence check"
bash "$ROOT/scripts/dev/fence-check.sh" > /tmp/smix-ship-fence.log 2>&1 \
  || fail "fence-check FAILED — the sense path reaches smix-ai-tier (see /tmp/smix-ship-fence.log)"

python3 "$ROOT/scripts/dev/gen-llms.py" --check > /tmp/smix-ship-llms.log 2>&1 \
  || fail "llms.txt/llms-full.txt are stale — run scripts/dev/gen-llms.py and commit (see /tmp/smix-ship-llms.log)"
# --- cargo-semver-checks ----------------------------------------------

# --- TS SDK tests ------------------------------------------------------
# A second, and it needs node_modules rather than a Rust build — so it
# belongs with the other judgements that can be paid for in seconds.
# It sat after `cargo test --workspace` and the ordering gate said so.
log "npm/smix-rn typecheck + vitest"
( cd "$ROOT/npm/smix-rn" && bun run typecheck && bun run test ) \
    > /tmp/smix-ship-ts-test.log 2>&1 \
  || fail "TS SDK tests FAILED — see /tmp/smix-ship-ts-test.log"

# --- source-only judgements -------------------------------------------
# Everything below reads files. No build, no device, no network — which
# is why it belongs here rather than after six minutes of compiling.
#
# It used to sit after the workspace tests. That was inside tolerance
# until this release's tests pushed `cargo test --workspace` past five
# minutes on its own, and the ordering gate said so: forty-six
# second-level judgements waiting behind work that has nothing to do
# with them. The answer is the same one 6.3.0 reached by hand — move
# them to the front — and this time a gate will notice if they drift
# back.
# --- route conformance ------------------------------------------------
# Derives the served-route list from both runner sources and sweeps every
# shipped file for phantom endpoints. It caught 13 fictional routes in
# review, then sat unwired while ship.sh ran everything except it.
log "route conformance"
python3 "$ROOT/scripts/dev/route-conformance.py" > /tmp/smix-ship-routes.log 2>&1 \
  || fail "route conformance FAILED — see /tmp/smix-ship-routes.log"

# --- every verb reads a locale map ------------------------------------
# `localizedText:` is rewritten to a Text selector by a pure function.
# Three of twelve verbs called it; the rest handed the locale map to the
# resolver, which matches nothing against it — the same element found by
# `assertVisible` and not by `longPressOn`, measured on emulator-5554.
log "every verb reads a locale map"
python3 "$ROOT/scripts/dev/every-verb-reads-a-locale-map.py" > /tmp/smix-ship-locale.log 2>&1 \
  || fail "every verb reads a locale map FAILED — see /tmp/smix-ship-locale.log"

# --- guide claims + corpus ---------------------------------------------
# The guides are the release's user-facing half, and this is the last
# place before they reach anybody. Both of these ran nowhere until
# 2026-08-06 — a check that never runs reads as coverage while providing
# none, and the version of that which matters is the one on the path to
# users.
log "guide claims scan"
python3 "$ROOT/scripts/dev/guide-claims-scan.py" > /tmp/smix-ship-guide-claims.log 2>&1 \
  || fail "guide claims scan FAILED — a guide claims something the code does not do (see /tmp/smix-ship-guide-claims.log)"
log "guide corpus in step with the guides"
python3 "$ROOT/scripts/dev/guide-corpus-sync.py" --check > /tmp/smix-ship-guide-corpus.log 2>&1 \
  || fail "guide corpus is out of step with the guides — run scripts/dev/guide-corpus-sync.py (see /tmp/smix-ship-guide-corpus.log)"

# --- android gate scan -------------------------------------------------
# Re-derives the Android modules and checks each one's test tasks are run
# by preflight, CI and this script. The app module's unit tests were
# outside all three for the whole of v1 and v2, which is how a header
# nobody read and a placeholder package both shipped.
log "android gate scan"
python3 "$ROOT/scripts/dev/android-gate-scan.py" > /tmp/smix-ship-android-gate.log 2>&1 \
  || fail "android gate scan FAILED — an Android test task is outside the gates (see /tmp/smix-ship-android-gate.log)"

# --- audit ledger ------------------------------------------------------
# Re-evaluates every citation in .claude/docs/audit-ledger.md. That table records
# which known defects are still live, and its predecessor drifted badly
# enough that three of five sampled entries had been fixed while still
# reading as open. Shipping against a stale account of what is broken is
# how a defect reaches users with a note saying someone already knew.
log "audit ledger"
python3 "$ROOT/scripts/dev/audit-ledger-scan.py" > /tmp/smix-ship-ledger.log 2>&1 \
  || fail "audit ledger scan FAILED — a citation no longer holds; re-verify that row (see /tmp/smix-ship-ledger.log)"

# --- release record ----------------------------------------------------
# The breaking-change table and the CHANGELOG's Breaking section are two
# lists of the same thing, and they once held six entries and eight. Also
# checks that every behaviour change reached the release notes, and that
# the publish DAG below still covers the workspace in a topological order
# — a crate missing from it is discovered seventeen publishes in, when the
# earlier steps cannot be taken back.
log "release record"
# `--shipping` is what makes the CHANGELOG entry mandatory. Bare, this gate
# runs during development, where the current major has no entry yet and the
# reconciliation stands down; passing the version says a release is what is
# happening, and then the entry's absence is the failure.
python3 "$ROOT/scripts/dev/release-record-scan.py" --shipping "$VERSION" > /tmp/smix-ship-record.log 2>&1 \
  || fail "release record scan FAILED — the release's several lists disagree (see /tmp/smix-ship-record.log)"


# --- hygiene scan ------------------------------------------------------
# Development noise and dead doc pointers in everything a reader outside
# this repo can see. Its own docstring says it exits non-zero "so it can
# gate a release" — and until now this script mentioned it only in the
# two comments below, never calling it. preflight ran it, CI ran it, the
# release did not.
log "hygiene scan"
python3 "$ROOT/scripts/dev/hygiene-scan.py" > /tmp/smix-ship-hygiene.log 2>&1 \
  || fail "hygiene scan FAILED — shipped sources carry development noise or dead doc pointers (see /tmp/smix-ship-hygiene.log)"

# --- publish dag ------------------------------------------------------
# Before anything is built, ask whether the publish list below covers
# the workspace. A crate that something depends on and is missing fails
# at cargo publish, forty minutes in; a crate nothing depends on yet —
# every crate, on the release that introduces it — is simply never
# published, and the ship says COMPLETE. Seconds, so it runs early.
log "publish dag"
python3 "$ROOT/scripts/dev/publish-dag-is-complete.py" > /tmp/smix-ship-publish-dag.log 2>&1 \
  || fail "publish dag FAILED — the crates.io list and the workspace disagree (see /tmp/smix-ship-publish-dag.log)"

# --- actions pinned ----------------------------------------------------
# What CI ran is part of what this release was tested by, and a moving
# action tag means that cannot be stated. Seconds, so it runs here.
log "actions pinned"
python3 "$ROOT/scripts/dev/actions-are-pinned.py" > /tmp/smix-ship-actions.log 2>&1 \
  || fail "actions pinned FAILED — a workflow names a moving ref (see /tmp/smix-ship-actions.log)"

# --- job ceilings ------------------------------------------------------
log "job ceilings"
python3 "$ROOT/scripts/dev/jobs-have-a-ceiling.py" > /tmp/smix-ship-ceilings.log 2>&1 \
  || fail "job ceilings FAILED — a CI job may run for six hours (see /tmp/smix-ship-ceilings.log)"

# --- self-tests are wired ----------------------------------------------
# v10's reconciliation gate, on recorded payloads and no device. It went
# blind to its own wire for a whole checkpoint while this suite stayed
# green — the payloads it was recorded from pre-dated an envelope the CLI
# grew — so the suite now asserts the recorded shape too.
log "the two-paths gate can still go red"
python3 "$ROOT/scripts/dev/two-paths-agree.test.py" > /tmp/smix-ship-twopaths.log 2>&1 \
  || fail "the two-paths gate no longer goes red (see /tmp/smix-ship-twopaths.log)"

log "the publish-dag gate can still go red"
python3 "$ROOT/scripts/dev/publish-dag-is-complete.test.py" > /tmp/smix-ship-dagtest.log 2>&1 \
  || fail "the publish-dag gate no longer goes red on a broken list (see /tmp/smix-ship-dagtest.log)"

# Named one by one rather than looped: workflow-scan reads this file
# for the gate it is looking for, and a name assembled from a loop
# variable is a name it cannot find. A scan that cannot see an
# invocation reports it missing, which is the right way round.
log "today's gates can still go red"
python3 "$ROOT/scripts/dev/actions-are-pinned.test.py" > /tmp/smix-ship-gatetests.log 2>&1 \
  || fail "actions-are-pinned no longer goes red on broken input (see /tmp/smix-ship-gatetests.log)"
python3 "$ROOT/scripts/dev/jobs-have-a-ceiling.test.py" >> /tmp/smix-ship-gatetests.log 2>&1 \
  || fail "jobs-have-a-ceiling no longer goes red on broken input (see /tmp/smix-ship-gatetests.log)"
python3 "$ROOT/scripts/dev/a-selftest-nobody-runs.test.py" >> /tmp/smix-ship-gatetests.log 2>&1 \
  || fail "a-selftest-nobody-runs no longer goes red on broken input (see /tmp/smix-ship-gatetests.log)"

log "the publication verifier asks the right things"
python3 "$ROOT/scripts/dev/verify-published-reads-registries.test.py" \
  > /tmp/smix-ship-verifytest.log 2>&1 \
  || fail "the publication verifier no longer asks the right things (see /tmp/smix-ship-verifytest.log)"

log "a published crate can run its tests"
python3 "$ROOT/scripts/dev/a-published-crate-can-run-its-tests.py" \
  > /tmp/smix-ship-packagetests.log 2>&1 \
  || fail "a crate's tests read files its package will not carry, undeclared (see /tmp/smix-ship-packagetests.log)"

log "a verb does not assume a platform"
python3 "$ROOT/scripts/dev/a-verb-does-not-assume-a-platform.py" \
  > /tmp/smix-ship-verbplatform.log 2>&1 \
  || fail "a runner verb reaches one platform without saying so (see /tmp/smix-ship-verbplatform.log)"

log "a hand-copied table says a number"
python3 "$ROOT/scripts/dev/a-hand-copied-table-says-a-number.py" \
  > /tmp/smix-ship-verbcount.log 2>&1 \
  || fail "a written verb-table count disagrees with the table (see /tmp/smix-ship-verbcount.log)"

log "self-tests are wired"
python3 "$ROOT/scripts/dev/a-selftest-nobody-runs.py" > /tmp/smix-ship-selftests.log 2>&1 \
  || fail "a self-test is invoked by nothing (see /tmp/smix-ship-selftests.log)"

# --- scope promise scan ------------------------------------------------
# Every promise in the scope file still matches what exists. `--stable`
# was promised, never built, never withdrawn, and agreed with by four
# documents — three of them gitignored — for seven months. A shipped
# promise may not cite a document as evidence it was implemented.
log "scope promise scan"
python3 "$ROOT/scripts/dev/scope-promise-scan.py" > /tmp/smix-ship-scope.log 2>&1 \
  || fail "scope promise scan FAILED — the scope file and the tree disagree (see /tmp/smix-ship-scope.log)"

# Measured 482s on a cold cache and 0s on a warm one: it runs a napi
# build. Kept first — where 6.6 put it, reading the warm number — it
# pushed every seconds-long judgement eight minutes down the run, and
# the ordering gate said so. A build belongs with the builds.
log "napi loader"
# --verbose, because the quiet form reports a line COUNT. "104 lines
# differ" cannot be read after the fact, and this gate went red once
# during 6.4.0's ship and clean on the next hand-run with the tree
# unchanged — with only a count in the log there was nothing to compare.
"$ROOT/scripts/dev/napi-dts-fresh.sh" --verbose > /tmp/smix-ship-napi-dts.log 2>&1 \
  || fail "napi loader (index.d.ts/index.js) is not what napi generates — see /tmp/smix-ship-napi-dts.log"


# The corpus gate's verdict, driven with fabricated records. FLAKE is
# not green, and that rule is otherwise only reachable by booting a sim.
log "flake classifier + corpus verdict self-tests"
python3 "$ROOT/scripts/dev/flake-classify.test.py" > /tmp/smix-ship-flake.log 2>&1 \
  || fail "flake classifier self-test FAILED — see /tmp/smix-ship-flake.log"
bash "$ROOT/scripts/release/corpus-gate.sh" --selftest >> /tmp/smix-ship-flake.log 2>&1 \
  || fail "corpus-gate verdict self-test FAILED — see /tmp/smix-ship-flake.log"
bash "$ROOT/scripts/dev/v3.0-c3-determinism.sh" --selftest >> /tmp/smix-ship-flake.log 2>&1 \
  || fail "determinism verdict self-test FAILED — see /tmp/smix-ship-flake.log"

# A flow excused from the gate must carry a measured rate and a
# history, or "known unstable" is just a flow someone got tired of.
# Twenty of twenty-one corpus flows name system-app identifiers that
# differ by iOS version; the portable tier is what a CI runner can run.
# The CI job must run the same script a person can, or reproducing a
# CI failure means running something that was equivalent when written.
# A gate that goes red because the machine was busy is one people stop
# reading, and then stop running.
# One script shutting down a device it was lent is how four others fail.
# A gate naming a system app's resource ids has taken that app's
# version as a contract; when one goes missing the failure lands
# somewhere else entirely.
log "android gates drive our own app"
python3 "$ROOT/scripts/dev/android-subject-scan.py" > /tmp/smix-ship-android-subject.log 2>&1 \
  || fail "android-subject scan FAILED — see /tmp/smix-ship-android-subject.log"

# A device record written into a checkout is a record the next checkout
# cannot read. That is how a runner came to be on the books and
# invisible at the same time.
log "device facts are machine-scoped"
python3 "$ROOT/scripts/dev/device-facts-are-machine-scoped.py" > /tmp/smix-ship-device-scope.log 2>&1 \
  || fail "device-facts-are-machine-scoped FAILED — see /tmp/smix-ship-device-scope.log"

# A ledger written into a checkout is a ledger the next checkout cannot
# read — which is how a runner came to be on the books and invisible.
log "leases are machine-scoped"
python3 "$ROOT/scripts/dev/leases-are-machine-scoped.py" > /tmp/smix-ship-lease-scope.log 2>&1 \
  || fail "leases-are-machine-scoped FAILED — see /tmp/smix-ship-lease-scope.log"

# A tree's old book is read and never obeyed. While the two disagree,
# nothing acts — for ninety-one minutes on 2026-08-11 the machine ledger
# called a live runner abandoned.
log "no second ledger path"
python3 "$ROOT/scripts/dev/no-second-ledger-path.py" > /tmp/smix-ship-ledger-path.log 2>&1 \
  || fail "no-second-ledger-path FAILED — see /tmp/smix-ship-ledger-path.log"

# The one gate on this list whose defect has already shipped. `llms.txt`
# — the first file an agent reads — opened with "never a physical device"
# through 3.x and into 4.1, two majors after §9 #1 stopped saying it.
# Every other gate was green, because none of them knows a rule has a day.
# The four layers, and which of the two shapes layer three is in. A
# release cut while the record of what is being worked on has gone
# missing is one nobody can reconstruct afterwards.
log "the four layers are all present"
python3 "$ROOT/scripts/dev/contract-scan.py" > /tmp/smix-ship-contract.log 2>&1 \
  || fail "contract scan FAILED — a layer is missing or the gap is unclaimed (see /tmp/smix-ship-contract.log)"

# An element can be nameable in a flow and unnameable from the surface an
# agent drives through, with nothing red. `point` was, for two majors.
log "every selector form is declared on every surface"
python3 "$ROOT/scripts/dev/selector-surface-scan.py" > /tmp/smix-ship-selector-surface.log 2>&1 \
  || fail "selector-surface scan FAILED — a selector form is undeclared on a surface (see /tmp/smix-ship-selector-surface.log)"

# /health says the server is answering and nothing about the app binding.
# Two commands concluded a device was drivable from it, and one of them
# was the command you reach for when it is not.
log "every health_ok call site says whether it decides"
python3 "$ROOT/scripts/dev/health-is-not-a-session-check.py" > /tmp/smix-ship-health-session.log 2>&1 \
  || fail "health/session scan FAILED — a call site decides from /health alone (see /tmp/smix-ship-health-session.log)"

# Asking whether a session works without naming an app answers about
# whichever app the runner was bound to at startup. Harmless where smix
# started the runner; expensive the first time it did not.
log "every session probe says which app it asks about"
python3 "$ROOT/scripts/dev/probes-name-the-app.py" > /tmp/smix-ship-probe-naming.log 2>&1 \
  || fail "probe-naming scan FAILED — a probe asks about an unnamed app (see /tmp/smix-ship-probe-naming.log)"

# A surface that quietly does its own tap-then-screenshot works, passes
# every test, and takes the frame 237 ms later from a different layer
# than the touch. Only a scan sees that.
log "tap-then-frame is one path"
python3 "$ROOT/scripts/dev/tap-then-capture-is-one-path.py" > /tmp/smix-ship-one-path.log 2>&1 \
  || fail "one-path scan FAILED — the combined action grew a second implementation (see /tmp/smix-ship-one-path.log)"

# A flag with no description on the surface is a sentence nobody wrote.
# Twenty were blank when this was written, four of them found by a reader.
log "every flag says what it does"
python3 "$ROOT/scripts/dev/every-flag-says-what-it-does.py" > /tmp/smix-ship-flag-docs.log 2>&1 \
  || fail "flag-description scan FAILED — a flag reaches the surface with nothing to read (see /tmp/smix-ship-flag-docs.log)"
log "an-authorised-hatch-reaches-every-surface"
python3 "$ROOT/scripts/dev/an-authorised-hatch-reaches-every-surface.py" > /tmp/smix-ship-an-authorised-hatch-reaches-every-surface.log 2>&1 \
  || fail "an-authorised-hatch-reaches-every-surface FAILED — see /tmp/smix-ship-an-authorised-hatch-reaches-every-surface.log"
log "a-tap-proves-aim-not-arrival"
python3 "$ROOT/scripts/dev/a-tap-proves-aim-not-arrival.py" > /tmp/smix-ship-a-tap-proves-aim-not-arrival.log 2>&1 \
  || fail "a-tap-proves-aim-not-arrival FAILED — see /tmp/smix-ship-a-tap-proves-aim-not-arrival.log"
log "v5.1-c10-ground-truth-is-complete"
python3 "$ROOT/scripts/dev/v5.1-c10-ground-truth-is-complete.py" > /tmp/smix-ship-v5.1-c10-ground-truth-is-complete.log 2>&1 \
  || fail "v5.1-c10-ground-truth-is-complete FAILED — see /tmp/smix-ship-v5.1-c10-ground-truth-is-complete.log"
log "no-script-picks-a-device-by-accident"
python3 "$ROOT/scripts/dev/no-script-picks-a-device-by-accident.py" > /tmp/smix-ship-no-script-picks-a-device-by-accident.log 2>&1 \
  || fail "no-script-picks-a-device-by-accident FAILED — see /tmp/smix-ship-no-script-picks-a-device-by-accident.log"

log "generated-artifacts-are-load-bearing"
python3 "$ROOT/scripts/dev/generated-artifacts-are-load-bearing.py" > /tmp/smix-ship-generated-artifacts.log 2>&1 \
  || fail "generated-artifacts-are-load-bearing FAILED — see /tmp/smix-ship-generated-artifacts.log"

log "project-pointer-holds-no-facts"
python3 "$ROOT/scripts/dev/project-pointer-holds-no-facts.py" > /tmp/smix-ship-project-pointer.log 2>&1 \
  || fail "project-pointer-holds-no-facts FAILED — see /tmp/smix-ship-project-pointer.log"

log "a retired sentence is off the surfaces"
python3 "$ROOT/scripts/dev/retired-claims-scan.py" > /tmp/smix-ship-retired-claims.log 2>&1 \
  || fail "retired-claims scan FAILED — see /tmp/smix-ship-retired-claims.log"

log "teardown restores rather than imposes"
python3 "$ROOT/scripts/dev/teardown-restores-scan.py" > /tmp/smix-ship-teardown.log 2>&1 \
  || fail "teardown-restores scan FAILED — see /tmp/smix-ship-teardown.log"

log "a yield is not a failure"
python3 "$ROOT/scripts/dev/yield-is-not-failure-scan.py" > /tmp/smix-ship-yield.log 2>&1 \
  || fail "yield-is-not-failure scan FAILED — see /tmp/smix-ship-yield.log"

log "portable tier parity"
python3 "$ROOT/scripts/dev/portable-tier-parity.py" > /tmp/smix-ship-tier-parity.log 2>&1 \
  || fail "portable tier parity FAILED — see /tmp/smix-ship-tier-parity.log"

log "corpus portability scan"
python3 "$ROOT/scripts/dev/corpus-portability-scan.py" > /tmp/smix-ship-portability.log 2>&1 \
  || fail "corpus portability scan FAILED — see /tmp/smix-ship-portability.log"

log "known-unstable list scan"
python3 "$ROOT/scripts/dev/known-unstable-scan.py" > /tmp/smix-ship-known-unstable.log 2>&1 \
  || fail "known-unstable list scan FAILED — see /tmp/smix-ship-known-unstable.log"

log "three readers agree"
python3 "$ROOT/scripts/dev/three-readers-agree.py" > /tmp/smix-ship-three-readers.log 2>&1 \
  || fail "three-readers-agree FAILED — the recorded reports differ across the three host trees (see /tmp/smix-ship-three-readers.log)"
python3 "$ROOT/scripts/dev/three-readers-agree.py" --assert-ci-union >> /tmp/smix-ship-three-readers.log 2>&1 \
  || fail "three-readers-agree --assert-ci-union FAILED — a reader is named by no CI job (see /tmp/smix-ship-three-readers.log)"

log "mcp cli parity scan"
python3 "$ROOT/scripts/dev/mcp-cli-parity-scan.py" > /tmp/smix-ship-mcp-parity.log 2>&1 \
  || fail "mcp cli parity scan FAILED — see /tmp/smix-ship-mcp-parity.log"

# A fuzz lockfile that no longer satisfies the manifests above it is
# not a lockfile: the next cargo command resolves something else and
# writes it back. Four of them sat that way after kevy 5.3 -> 5.4.1
# and nothing said so.
log "fuzz lockfiles are usable"
python3 "$ROOT/scripts/dev/fuzz-lockfiles-are-usable.py" > /tmp/smix-ship-fuzz-locks.log 2>&1 \
  || fail "fuzz lockfiles are usable FAILED — see /tmp/smix-ship-fuzz-locks.log"

log "the fuzz-lockfile gate can still go red"
python3 "$ROOT/scripts/dev/fuzz-lockfiles-are-usable.test.py" > /tmp/smix-ship-fuzz-locks-selftest.log 2>&1 \
  || fail "fuzz-lockfile gate self-test FAILED — see /tmp/smix-ship-fuzz-locks-selftest.log"

# The device gates run an hour in. This one's judgement is a pure
# function now, so the shape that made it crash rather than speak is
# checked here, before anything is compiled.
log "the A4 window verdict can still speak"
python3 "$ROOT/scripts/dev/android-a4-verdict.test.py" > /tmp/smix-ship-a4-selftest.log 2>&1 \
  || fail "A4 verdict self-test FAILED — see /tmp/smix-ship-a4-selftest.log"

log "every verdict answers in sentences"
python3 "$ROOT/scripts/dev/a-verdict-answers-in-sentences.py" > /tmp/smix-ship-verdict-sentences.log 2>&1 \
  || fail "a verdict cannot report its own finding — see /tmp/smix-ship-verdict-sentences.log"

log "no reply is waiting in our own records"
python3 "$ROOT/scripts/dev/a-reply-nobody-sent.py" > /tmp/smix-ship-reply-sent.log 2>&1 \
  || fail "a consumer letter nobody can show was sent — see /tmp/smix-ship-reply-sent.log"

log "the delivery sweep can still go red"
python3 "$ROOT/scripts/dev/a-reply-nobody-sent.test.py" > /tmp/smix-ship-reply-sent-selftest.log 2>&1 \
  || fail "the delivery sweep cannot go red — see /tmp/smix-ship-reply-sent-selftest.log"

log "no gate says yes with its subject gone"
python3 "$ROOT/scripts/dev/a-gate-without-its-subject.py" > /tmp/smix-ship-gate-subject.log 2>&1 \
  || fail "a gate passes without its subject — see /tmp/smix-ship-gate-subject.log"

log "the subject sweep can still go red"
python3 "$ROOT/scripts/dev/a-gate-without-its-subject.test.py" > /tmp/smix-ship-gate-subject-selftest.log 2>&1 \
  || fail "the subject sweep cannot go red — see /tmp/smix-ship-gate-subject-selftest.log"

log "the verdict sweep can still go red"
python3 "$ROOT/scripts/dev/a-verdict-answers-in-sentences.test.py" > /tmp/smix-ship-verdict-sweep-selftest.log 2>&1 \
  || fail "verdict sweep self-test FAILED — see /tmp/smix-ship-verdict-sweep-selftest.log"

log "preflight parity scan"
python3 "$ROOT/scripts/dev/preflight-parity-scan.py" > /tmp/smix-ship-parity.log 2>&1 \
  || fail "preflight parity scan FAILED — see /tmp/smix-ship-parity.log"

log "gate port scan"
python3 "$ROOT/scripts/dev/gate-port-scan.py" > /tmp/smix-ship-gate-port.log 2>&1 \
  || fail "gate port scan FAILED — see /tmp/smix-ship-gate-port.log"

log "fuzz targets compile"
python3 "$ROOT/scripts/dev/fuzz-targets-compile.py" > /tmp/smix-ship-fuzz-compile.log 2>&1 \
  || fail "fuzz-targets-compile FAILED — a fuzz crate no longer builds against the crate it fuzzes. See /tmp/smix-ship-fuzz-compile.log"

log "every runner a gate starts comes down"
python3 "$ROOT/scripts/dev/every-runner-a-gate-starts-comes-down.py" \
  > /tmp/smix-ship-teardown.log 2>&1 \
  || fail "every-runner-a-gate-starts-comes-down FAILED — a gate leaves a runner behind, or hides what its teardown said. See /tmp/smix-ship-teardown.log"

log "route context scan"
python3 "$ROOT/scripts/dev/route-context-scan.py" > /tmp/smix-ship-route-context.log 2>&1 \
  || fail "route context scan FAILED — see /tmp/smix-ship-route-context.log"

log "gate subject diversity"
python3 "$ROOT/scripts/dev/gate-subject-diversity.py" > /tmp/smix-ship-subjects.log 2>&1 \
  || fail "gate subject diversity FAILED — see /tmp/smix-ship-subjects.log"

# --- workflow scan -----------------------------------------------------
# The development contract survives a clone: charter and rule cards
# tracked, hook scripts present and wired, guards tested, no GNU-only
# tools, and every source gate running in all three places. That last
# check is what found this script missing two gates.
log "workflow scan"
python3 "$ROOT/scripts/dev/workflow-scan.py" > /tmp/smix-ship-workflow.log 2>&1 \
  || fail "workflow scan FAILED — see /tmp/smix-ship-workflow.log"

# --- clippy -----------------------------------------------------------
# `warnings = "deny"` in the workspace lints covers rustc, not clippy, and
# nothing ran clippy — so four lints sat in the tree, one of them a doc
# comment detached from the type it described in a stone crate. Clean at
# the time this was added; here so it stays that way.
#
# Here rather than after the release build, where it used to be: nothing
# before it is a precondition — it reads source — and on an already-built
# tree it costs three seconds. `cheap-gates-come-first` measured those
# three seconds sitting behind eleven minutes of Gradle and device work,
# which is what that gate exists to say. A lint error is now found before
# anything has been compiled for it.
# Beside clippy, and for the same reason: both read source and neither needs
# anything compiled first. `preflight.sh` has had this check since it existed
# and the ship did not, so every path that reached a release without going
# through preflight reached it unformatted. v10 lost two CI rounds that way —
# once after a field went into fifty struct literals, once after an `if let`
# was collapsed by hand. The code was right both times; the round was gone.
log "rustfmt"
( cd "$ROOT" && cargo fmt --all --check ) > /tmp/smix-ship-fmt.log 2>&1 \
  || fail "rustfmt FAILED — run \`cargo fmt --all\` (see /tmp/smix-ship-fmt.log)"

log "clippy"
( cd "$ROOT" && cargo clippy --workspace --all-targets ) > /tmp/smix-ship-clippy.log 2>&1 \
  || fail "clippy FAILED — see /tmp/smix-ship-clippy.log"

# --- Android unit tests + androidTest compile --------------------------
# Compiles the generated Kotlin bindings AND runs the unit suites.
# The bindings previously first compiled during `gradlew :sdk:publish` —
# publish-time was the first compile, which is exactly how the
# DriveError field/Throwable.message collision reached a release branch.
#
# The task name is bare rather than `:sdk:`-qualified because it used to
# be qualified, and the app module's eight test files were consequently
# run by nothing at all — including the ones written to cover the empty
# set_target_bundle_id and the placeholder package in the view-id lookup.
#
# assembleDebugAndroidTest is the Android counterpart of the
# `xcodebuild build-for-testing` step above: it compiles the runner body
# that ships to users without starting a device.
#
# :app's connectedDebugAndroidTest is NOT here and will not be. It was
# measured sitting at "Tests 0/1 completed" for three minutes forty
# while /health answered 200: it does not fail, it never returns, and in
# a release script that is a hang. :sdk's runs below, via the delegate.
log "android unit tests + androidTest compile (sdk + app; compiles kotlin bindings)"
( cd "$ROOT/android-runner" && ./gradlew testDebugUnitTest assembleDebugAndroidTest --console=plain ) \
    > /tmp/smix-ship-kotlin-test.log 2>&1 \
  || fail "Android unit tests / androidTest compile FAILED — see /tmp/smix-ship-kotlin-test.log"
# Built here rather than just before the corpus gate: the android gates
# run first and used to fall back to whatever smix was on PATH, so a
# 6.2.0 binary spent this release verifying 6.3.0. The corpus gate had
# already been fixed for exactly that; the device gates above it had not.
# Build the workspace's own smix release for the gate — a global `smix` on
# PATH is whatever version was installed some other day, and a mismatch
# between it and the runner sources this workspace ships is exactly how a
# pre-fold binary drove the post-fold runner in dry-run and the gate turned
# red on a real driver/runner drift.
log "cargo build -p smix-cli --release (for corpus gate)"
( cd "$ROOT" && cargo build -p smix-cli --release ) || fail "cargo build smix-cli --release"



# --- swift-bridge unit tests ------------------------------------------
# NOT bypassable. This suite sat outside the gate long enough for a test
# asserting a two-release-old contract to fail unnoticed for 15+ releases.
# ~18 s.
log "swift-bridge unit tests"
( cd "$ROOT/swift-bridge" && swift test ) > /tmp/smix-ship-swift-test.log 2>&1 \
  || fail "swift-bridge tests FAILED — see /tmp/smix-ship-swift-test.log"

# --- SmixRunner UITest compile ----------------------------------------
# `swift test` covers the SwiftPM library targets, not SmixRunnerUITests —
# the XCUITest body that ships in the runner sources and is what actually
# drives a device. It went uncompiled by any gate. build-for-testing on a
# generic simulator destination compiles it without booting a simulator.
log "SmixRunner UITest build"
( cd "$ROOT/swift-bridge" && xcodebuild build-for-testing \
    -scheme SmixRunner -destination 'generic/platform=iOS Simulator' ) \
    > /tmp/smix-ship-uitest-build.log 2>&1 \
  || fail "SmixRunnerUITests build FAILED — see /tmp/smix-ship-uitest-build.log"

# The runner tarballs are compared against the trees they were built
# from, and that comparison lived inside `cargo test --workspace` — which
# is where it was found, twice in one cycle, at thirteen and thirty-one
# minutes. Both times the fix was one command and the cost was the wait.
#
# Buried inside an expensive check it is also invisible to
# `cheap-gates-come-first`: that gate reads a profile of named gates, and
# a judgement with no name of its own cannot be said to be in the wrong
# place. Naming it is what makes its position a choice somebody made.
#
# Measured at 64s on a warm tree, against ~40 minutes for the suite it
# used to hide in.
log "runner tarballs match their sources"
( cd "$ROOT" && cargo test -p smix-runner-sources ) > /tmp/smix-ship-tarball.log 2>&1 \
  || fail "runner tarballs are stale — run scripts/release/build-runner-tarball.sh and build-android-runner-tarball.sh (see /tmp/smix-ship-tarball.log)"

# --- rust workspace tests ---------------------------------------------
# The workspace suite (830+ tests) had NO gate: ship.sh ran smoke + swift
# + lints while `cargo test` was left to whoever remembered. That is how
# /tap shipped a response body the wire crate deserialized to all-None
# without one red test. Non-bypassable, like the swift suite above.
log "cargo test --workspace"
( cd "$ROOT" && cargo test --workspace ) > /tmp/smix-ship-cargo-test.log 2>&1 \
  || fail "cargo test FAILED — see /tmp/smix-ship-cargo-test.log"

# --- judgements that need a build ------------------------------------
# These three are not seconds-long source reads. Two compile the adapter
# to ask the compiled table what it says, and the workflow scan takes a
# minute and a half on its own. Kept at the front they made the
# fail-fast block cost seven minutes, and the ordering gate reported the
# genuinely cheap judgements that followed them — correctly. Here the
# adapter is already built, so the two cost nothing, and nothing cheap
# waits behind them.

# --- every cell is declared -------------------------------------------
# The verb-by-form table has to agree with the code rather than with
# itself: a slot handed out and not listed is one the tests walk past,
# and a cell claiming a dispatch runtime.rs does not perform is the
# shape of the defect the table exists for.
log "every cell is declared"
python3 "$ROOT/scripts/dev/every-cell-is-declared.py" > /tmp/smix-ship-cells.log 2>&1 \
  || fail "every cell is declared FAILED — see /tmp/smix-ship-cells.log"

# --- selector matrix in the guide -------------------------------------
# The guide's verb-by-form table is generated from the one the code
# decides by. It said "any selector position accepts `ocrText:`"
# directly above the list of the four verbs that read it — a sentence
# and a list disagreeing in the same paragraph, both written by hand.
log "selector matrix in the guide"
python3 "$ROOT/scripts/dev/gen-selector-matrix.py" --check > /tmp/smix-ship-matrix.log 2>&1 \
  || fail "the guide's selector matrix is not what the table says — see /tmp/smix-ship-matrix.log"

# Twenty corpus flows against one system app is one subject walked
# twenty ways, and a defect that only shows on an ordinary app was
# invisible to every device gate at once — which is how a consumer
# found `/tree` returning only the SystemUI windows while everything
# here was green. This asks whether the gates below are pointed at more
# than the platform's own app; it does not ask whether they pass.
# A route that drives the app and does not read `App-Bundle-Id` uses
# whichever app the runner booted with, in silence. Three did.
# A gate any bystander process can turn red judges nothing.
# preflight promises to run what CI runs. Nothing checked, and two
# steps had no local counterpart at all.
# The plugin adds initiative, not capability. Nothing checked that
# direction, and two MCP tools had no CLI behind them.
# --- android instrumentation (device) ----------------------------------
# The :sdk assertion suite on a pinned emulator. Placed early — before
# fuzz, clippy, semver and anything that publishes — so a missing
# emulator costs seconds rather than being discovered after the long
# work. Device selection and the deadline live in the delegate, not
# here: keeping them inline would put an adb call in a script the
# PreToolUse guard can no longer read, and the delegate carries the same
# emulator-only rule the guard enforces.
# Three legs need this emulator -- instrumentation, behaviour, and v10's
# four gates -- and it is the one the dogfood consumer runs its suites
# on. Their `smix run` takes a lease, and ours refuses a device somebody
# else holds; that refusal is correct and it is not what a release wants
# to discover at minute 250. So: wait here, before the first of the
# three, rather than in front of each.
#
# This decides whether to WAIT, not whether it is safe. `smix run` and
# `runner up` decide that from the lease and name the holder (see
# .claude/rfcs/10.0-android-runner-ownership.md); a second copy of that
# judgement here would be the copy that goes stale. So it asks the
# cheaper question off the same ledger -- is anything holding this
# device -- and when the wait ends, lets the product speak.
android_device_is_busy() {
  "$ROOT/target/release/smix" runner list 2>/dev/null \
    | grep -qE "[[:space:]]$ANDROID_DEVICE[[:space:]]" && return 0
  python3 - "$ANDROID_DEVICE" <<'PYBUSY'
import json, os, subprocess, sys
serial = sys.argv[1]
path = os.path.join(
    os.environ.get("XDG_DATA_HOME", os.path.expanduser("~/.local/share")),
    "smix", "leases", f"{serial}.json",
)
try:
    holder = json.load(open(path))["holder"]
except Exception:
    sys.exit(1)
alive = subprocess.run(["ps", "-p", str(holder["pid"])], capture_output=True).returncode == 0
sys.exit(0 if alive and holder["pid"] != os.getpid() else 1)
PYBUSY
}
ANDROID_DEVICE="${SMIX_V10_ANDROID:-emulator-5554}"
if android_device_is_busy; then
  log "android: $ANDROID_DEVICE is held by another process — waiting up to 15m"
  for _ in $(seq 1 90); do
    android_device_is_busy || break
    sleep 10
  done
  android_device_is_busy \
    && log "android: $ANDROID_DEVICE still held after 15m — letting the gates judge it" \
    || log "android: $ANDROID_DEVICE is free"
fi

log "android instrumentation (device)"
bash "$ROOT/scripts/release/android-instrumentation-gate.sh" \
  || fail "android instrumentation gate FAILED — see the verdict above; start an emulator with \
\"\$ANDROID_HOME/emulator/emulator\" -avd sim-smix-android-01 -port 5554 -no-snapshot-save &"

# --- android behaviour (device) ----------------------------------------
# Three assertions that each go red when their fix is reverted: the
# key-events flag actually changing the driver's path, every driving
# request carrying the app under test, and the qualified view-id
# spelling being what found the node. All three shipped broken once
# without failing anything.
#
# Adjacent to the instrumentation gate because they share the emulator:
# a missing device should fail once, in one place, early.
log "android behaviour (device)"
SMIX_BIN="$ROOT/target/release/smix" \
  bash "$ROOT/scripts/release/android-behaviour-gate.sh" \
  || fail "android behaviour gate FAILED — see the verdict above"


# --- v10's device gates ------------------------------------------------
# The probe's four, run where they can go red for someone other than the
# person who wrote them. Written during v10 and wired here in the same
# release: a gate that only ever ran by hand stops running the day its
# author stops typing it, and nothing says so.
#
# They need the fixture with `debugImplementation("jp.golia.smix:smix-probe")`
# installed on the emulator; each says which line is missing when it is not.
V10_DEVICE="${SMIX_V10_ANDROID:-emulator-5554}"

# These five need a runner and none of them starts one. They passed for
# weeks because a runner happened to be up on 22095 from a hand-run, and
# the first ship without one said the two readers disagreed -- when what
# had happened is that one of them was never asked. A gate whose
# precondition nobody owns is a gate that reports on the wrong thing.
#
# The port comes from the OS for the reason written at gate-port-scan:
# a literal is a socket somebody else can be holding.
# `runner up` refuses a port another runner holds. It says nothing about
# a device somebody else is already driving -- and this emulator is the
# one the dogfood consumer runs its suites on. Bringing a second
# instrumentation up on it would end their run mid-flow, which is not a
# thing a release of ours gets to do. Seen 2026-08-29: a consumer batch
# was on this serial while the ship was still four hours from needing it.
#
# So: wait for it to go quiet, then say who has it rather than taking it.
if [[ -z "${SMIX_V10_ANDROID_PORT:-}" ]]; then
  V10_PORT="$(python3 -c 'import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()')"
  log "v10: runner up on $V10_DEVICE:$V10_PORT"
  SMIX_RUNNER_PORT="$V10_PORT" "$ROOT/target/release/smix" runner up "$V10_DEVICE" \
    --platform android --runner-port "$V10_PORT" > /tmp/smix-ship-v10-runner.log 2>&1 \
    || fail "v10: runner up failed on $V10_DEVICE (see /tmp/smix-ship-v10-runner.log)"
  V10_RUNNER_OURS=1
else
  V10_PORT="$SMIX_V10_ANDROID_PORT"
  V10_RUNNER_OURS=0
fi

v10_runner_down() {
  [[ "${V10_RUNNER_OURS:-0}" == "1" ]] || return 0
  SMIX_RUNNER_PORT="$V10_PORT" "$ROOT/target/release/smix" runner down \
    --platform android --device "$V10_DEVICE" >> /tmp/smix-ship-v10-runner.log 2>&1 || true
  V10_RUNNER_OURS=0
}
# ship_profile_close already owns EXIT (line ~73). Replacing it would have
# silently dropped the profile written at the end of every run, so this
# chains rather than overwrites.
trap 'v10_runner_down; ship_profile_close' EXIT

log "v10: two perception paths agree"
python3 "$ROOT/scripts/dev/two-paths-agree.py" --device "$V10_DEVICE" \
  --port "$V10_PORT" --min-both 16 \
  || fail "two-paths-agree FAILED — the semantics and accessibility readers disagree"

log "v10: the three that went red"
python3 "$ROOT/scripts/dev/the-three-that-went-red.py" --device "$V10_DEVICE" \
  --port "$V10_PORT" \
  || fail "the-three-that-went-red FAILED — a 6.4.0 root cause is unguarded again"

log "v10: a wait that does not end early"
python3 "$ROOT/scripts/dev/a-wait-that-does-not-end-early.py" --device "$V10_DEVICE" \
  --port "$V10_PORT" \
  || fail "a-wait-that-does-not-end-early FAILED"

log "v10: a semantics action is not a touch"
python3 "$ROOT/scripts/dev/a-semantics-action-is-not-a-touch.py" --device "$V10_DEVICE" \
  --port "$V10_PORT" \
  || fail "a-semantics-action-is-not-a-touch FAILED — the probe's action surface grew a touch substitute"

v10_runner_down
trap ship_profile_close EXIT


# --- corpus gate (real sim) -------------------------------------------
# Runs the bootstrap corpus end-to-end on a simulator. Device selection
# is explicit env first, else this repo's own booted dev sim.
#
# Not "the first booted sim", which is what it used to be. This machine
# also runs a consumer's sim, and picking blind meant the release gate
# could install its runner onto someone else's device and drive it —
# whichever one simctl happened to list first that day.
if [[ -z "${SMIX_CORPUS_SIM:-}" ]]; then
  SMIX_CORPUS_SIM="$(bash "$ROOT/scripts/dev/pick-dev-sim.sh")" \
    || fail "corpus gate needs SMIX_CORPUS_SIM (no unambiguous dev sim booted)"
fi
[[ -n "$SMIX_CORPUS_SIM" ]] \
  || fail "corpus gate needs SMIX_CORPUS_SIM or a booted dev sim"

# v10's iOS gate, on the sim the corpus already picked — a control behind a
# modal is still in the tree and a touch aimed at it is swallowed, and smix
# used to report that as a success.
log "v10: a tap that cannot land says so"
bash "$ROOT/scripts/dev/a-tap-that-cannot-land-says-so.sh" "$SMIX_CORPUS_SIM" \
  "${SMIX_V10_IOS_PORT:-}" \
  || fail "a-tap-that-cannot-land-says-so FAILED — a tap nothing could receive was reported as one that landed"

# The same shape one layer up. A request naming an app that is not on the
# device used to hang XCUITest's `.activate()` on the main actor until the
# watchdog killed the runner; the corpus then reported `runner unreachable`
# about twenty-three flows that never got to run.
log "v10: a foreground that cannot happen says so"
bash "$ROOT/scripts/dev/a-foreground-that-cannot-happen-says-so.sh" "$SMIX_CORPUS_SIM" \
  || fail "a-foreground-that-cannot-happen-says-so FAILED — either the refusal did not name the missing app, or the runner did not survive it"

log "corpus gate on $SMIX_CORPUS_SIM"
SMIX_CORPUS_SIM="$SMIX_CORPUS_SIM" \
SMIX_BIN="$ROOT/target/release/smix" \
  "$ROOT/scripts/release/corpus-gate.sh" \
    > /tmp/smix-ship-corpus.log 2>&1 \
  || fail "corpus gate FAILED — see /tmp/smix-ship-corpus.log"

# --- ffi bindings -----------------------------------------------------
# The Swift and Kotlin bindings are committed next to binary blobs, and
# nothing regenerated them: the build scripts Package.swift and
# build.gradle.kts name did not exist. Clean at the time this was added; here
# so the boundary cannot drift away from the crate again.
log "ffi bindings"
"$ROOT/scripts/dev/ffi-bindings-fresh.sh" > /tmp/smix-ship-ffi-bindings.log 2>&1 \
  || fail "FFI bindings are not what smix-ffi generates — see /tmp/smix-ship-ffi-bindings.log"
# --- fuzz smoke -------------------------------------------------------
# 15 fuzz targets existed with nothing running them; two had bit-rotted
# to the point of not compiling. A short budget per target keeps them
# honest — longer soaks stay manual.
log "fuzz smoke"
"$ROOT/scripts/dev/fuzz-smoke.sh" > /tmp/smix-ship-fuzz.log 2>&1 \
  || fail "fuzz smoke FAILED — see /tmp/smix-ship-fuzz.log"
#
# A crate with no published baseline is EXCLUDED, not tolerated. The
# comment here used to say the tool was "blind to brand-new crates" —
# it is not. It stops:
#
#     error: failed to retrieve index of crate versions from registry
#     Caused by: smix-ai-tier not found in registry (crates.io)
#
# and exits 1, which this step reads as a failed gate. Nobody had run it
# with a new crate in the workspace, so the sentence went unchallenged
# until the ship it would have blocked.
#
# Which crates those are is asked, not listed: a hand-kept list of
# exceptions is the thing that goes stale. The skipped set is logged,
# because a gate that quietly checks three fewer crates reads exactly
# like one that checked them all.
if command -v cargo-semver-checks >/dev/null 2>&1; then
  log "cargo-semver-checks"
  # Some crates cannot be checked at all, and the tool ABORTS THE WHOLE
  # RUN rather than skipping them. Two shapes seen here:
  #
  #   error: ... smix-ai-tier not found in registry (crates.io)
  #   error: failed to build rustdoc for crate smix-mcp v1.0.27
  #        (its 1.0.27 baseline was bin-only; it gained a lib in v2)
  #
  # The comment here used to call the tool "blind to brand-new crates".
  # It is not blind, it stops — and nobody had run it with a new crate
  # in the workspace, so the sentence stood until the ship it would have
  # blocked.
  #
  # Rather than keep a list of exceptions (the thing that goes stale) or
  # guess the reason from metadata (the current version's targets do not
  # predict the baseline's), run it and let its own error name the crate
  # it cannot handle, exclude that one, and go again. Every exclusion is
  # logged with the reason the tool gave, because a gate that quietly
  # checks fewer crates reads exactly like one that checked them all.
  SEMVER_EXCLUDE=()
  SEMVER_SKIPPED=()
  SEMVER_LOG=/tmp/smix-ship-semver.log
  SEMVER_ATTEMPTS=0
  SEMVER_MAX=$(cd "$ROOT" && cargo metadata --no-deps --format-version 1 |
      python3 -c 'import json,sys; print(len(json.load(sys.stdin)["packages"]))')
  while :; do
      SEMVER_ATTEMPTS=$((SEMVER_ATTEMPTS + 1))
      # `${arr[@]+"${arr[@]}"}` — bash 3.2 (macOS) errors on `"${arr[@]}"`
      # when the array is empty under `set -u`; this expands to nothing
      # when unset and to the array's quoted elements otherwise.
      if ( cd "$ROOT" && cargo semver-checks check-release --workspace \
              ${SEMVER_EXCLUDE[@]+"${SEMVER_EXCLUDE[@]}"} ) > "$SEMVER_LOG" 2>&1; then
          break
      fi
      if [ "$SEMVER_ATTEMPTS" -gt "$SEMVER_MAX" ]; then
          fail "cargo-semver-checks kept failing after $SEMVER_ATTEMPTS attempts — see $SEMVER_LOG"
      fi
      # Both patterns are taken from real output, not guessed: the
      # registry one is a `Caused by:` continuation line, indented and
      # with no colon before the name.
      UNCHECKABLE="$(sed -n 's/.*failed to build rustdoc for crate \([^ ]*\) .*/\1/p;
                             s/^[[:space:]]*\([a-z0-9._-]*\) not found in registry.*/\1/p' \
                         "$SEMVER_LOG" | head -1)"
      if [ -z "$UNCHECKABLE" ]; then
          fail "cargo-semver-checks FAILED — see $SEMVER_LOG"
      fi
      # The reason, taken now. Each attempt truncates $SEMVER_LOG, so by
      # the time the loop succeeds the refusal that caused an exclusion
      # is gone and only the name survives. The comment above promised
      # the reason was logged; it was not, and an exclusion without one
      # reads as a decision somebody made rather than a tool that
      # stopped.
      SEMVER_WHY="$(grep -m1 -E 'not found in registry|failed to build rustdoc' "$SEMVER_LOG" \
                    | sed 's/^[[:space:]]*//' | cut -c1-120)"
      SEMVER_EXCLUDE+=(--exclude "$UNCHECKABLE")
      SEMVER_SKIPPED+=("$UNCHECKABLE (${SEMVER_WHY:-no reason line found})")
  done
  # Report coverage from the run's own output, not from the exclusion
  # count. The tool also skips crates silently — anything with
  # `publish = false` or no library target — so "4 excluded" would have
  # read as "26 checked" when 21 were. The number that matters is how
  # many it actually looked at.
  SEMVER_CHECKED=$(grep -c '^ *Checking ' "$SEMVER_LOG" || true)
  SEMVER_TOTAL=$(cd "$ROOT" && cargo metadata --no-deps --format-version 1 |
      python3 -c 'import json,sys; print(len(json.load(sys.stdin)["packages"]))')
  note "semver-checks: $SEMVER_CHECKED of $SEMVER_TOTAL crates checked"
  if [ ${#SEMVER_SKIPPED[@]} -gt 0 ]; then
      note "semver-checks: excluded by name after the tool refused them: ${SEMVER_SKIPPED[*]}"
  fi
else
  fail "cargo-semver-checks not installed — cargo install cargo-semver-checks (required for a 2.0.0 ship)"
fi

# --- publish crates.io (DAG order) -----------------------------------

# `smix-lease` sits before `smix-simctl` and `smix-adapter-maestro`,
# which now depend on it: the machine root — where this machine keeps
# smix's data — is resolved in one place, and that place is there. It
# used to come after them, back when nothing above the capsule needed it.
# --- gate ordering ----------------------------------------------------
# Every gate has now run and the profile is complete. Before anything
# irreversible, read it: a judgement costing seconds that sat behind an
# hour of compiling is the shape that cost 6.3.0 four ninety-minute
# rounds, and until now the only thing preventing a recurrence was
# somebody remembering.
log "the ordering gate can still go red"
python3 "$ROOT/scripts/dev/cheap-gates-come-first.test.py" > /tmp/smix-ship-ordertest.log 2>&1 \
  || fail "cheap-gates-come-first no longer goes red on a bad ordering (see /tmp/smix-ship-ordertest.log)"

log "gate ordering"
ship_profile_close
python3 "$ROOT/scripts/dev/cheap-gates-come-first.py" "$SHIP_PROFILE" \
  > /tmp/smix-ship-ordering.log 2>&1 \
  || fail "gate ordering FAILED — a cheap judgement sits behind expensive work (see /tmp/smix-ship-ordering.log)"

# `note`, not `log`: publishing is an action, not a judgement, and the
# ordering check reads the profile looking for cheap judgements stranded
# behind expensive ones. Every publish is seconds long and comes last by
# necessity, so each one read as a gate in the wrong place — thirteen
# complaints, none of them about a gate. Same reason `note` exists at
# all; this is the second kind of line that is not a gate.
note "publish crates.io DAG at $VERSION"
CRATES=(
  smix-sim-health smix-runner-sources
  smix-screen smix-selector smix-input smix-error
  smix-verbs smix-metro-log smix-adb smix-ai-tier smix-contract
  smix-runner-wire smix-selector-resolver smix-fixture
  smix-annotate smix-migrate smix-authoring-ir
  smix-store smix-lease smix-simctl smix-runner-client
  smix-usbmux
  smix-capsule
  smix-host-coord-resolver smix-driver
  smix-sdk smix-mcp smix-adapter-maestro smix-recorder
  smix-authoring-propose
  smix-cli
)
# SMIX_SHIP_DRYRUN=1 runs every publish leg without touching a registry:
# npm/napi/gradle go through their own dry-run, git-tag is skipped, and
# cargo is skipped here — 27 interdependent crates cannot be `cargo publish
# --dry-run`'d (a dependent's dry-run cannot find a sibling that is not on
# crates.io yet), so its validation is CI's `cargo test --workspace` + the
# version and DAG gates above, not a dry-run.
# Does the sparse index already carry this exact version? The index lays
# names out by length: 1/2 char names sit in /1/ and /2/, 3 char names in
# /3/<first>/, everything longer under <first two>/<next two>/.
index_has_version() {
  local name="$1" want="$2" path
  case ${#name} in
    1) path="1/$name" ;;
    2) path="2/$name" ;;
    3) path="3/${name:0:1}/$name" ;;
    *) path="${name:0:2}/${name:2:2}/$name" ;;
  esac
  curl -sf -A "smix-ship" "https://index.crates.io/$path" 2>/dev/null \
    | grep -qE "\"vers\":\"$want\""
}

[ "$SHIP_DRY" = 1 ] && export SMIX_SHIP_NAPI_DRYRUN=1

for c in "${CRATES[@]}"; do
  if [ "$SHIP_DRY" = 1 ]; then
    log "cargo publish -p $c — SKIPPED (dry-run; interdependent crates validated by CI)"
    continue
  fi
  # A ship that dies after crate 17 has to be re-runnable, and re-running
  # means most of the DAG is already on the index. Ask the index first:
  # packaging and verifying a crate only to be told the version exists
  # costs a minute each, thirty times over. A curl that fails answers
  # "not there" and falls through to the publish below, so a network
  # hiccup degrades to the slow path rather than skipping a real upload.
  if index_has_version "$c" "$VERSION"; then
    log "cargo publish -p $c — already $VERSION on crates.io, skipping"
    continue
  fi
  log "cargo publish -p $c"
  # v1.0.4+ pattern from prior ship cycles: crates.io rate-limits at
  # ~1-2 publishes per 90s window under aggressive sequential publish.
  # Retry-with-backoff on 429/already-in-progress until success.
  #
  # The verdict is read from the log file, not from the pipeline's status:
  # `set -o pipefail` at the top of this script hands back cargo's exit
  # code even when the grep matched, which made the "already exists"
  # tolerance dead code — 6.3.0 hit it, retried five times against a crate
  # that was already published, and aborted the run.
  attempt=0
  while :; do
    ( cd "$ROOT" && cargo publish -p "$c" ) 2>&1 | tee /tmp/pub-$c.log || true
    grep -qE "Published|already exists|already uploaded" /tmp/pub-$c.log && break
    attempt=$((attempt+1))
    if grep -qE "429|rate limit|too many requests" /tmp/pub-$c.log; then
      log "  rate-limited ($attempt), sleeping 90s"
      sleep 90
    elif [[ $attempt -gt 5 ]]; then
      fail "cargo publish $c — exhausted retries; check /tmp/pub-$c.log"
    else
      log "  attempt $attempt failed, retry after 30s"
      sleep 30
    fi
  done
  sleep 8
done

# --- publish napi smix-node (per-triple prebuilds + loader) -----------
#
# The TS SDK (`@goliapkg/smix`) declares an optionalDependency on
# `@goliapkg/smix-node`, so that package and its three per-triple `.node`
# addons must exist on npm BEFORE smix-rn is published — otherwise the
# published SDK resolves a dependency that is not there and `loadNodeDriver`
# throws at runtime for every consumer.
#
# The three addons are built by the `napi-prebuild` CI matrix on native
# runners (darwin-arm64, darwin-x64, linux-x64-gnu). linux-x64-gnu cannot be
# cross-built on a mac ship host, so this step does not build — it collects
# the artifacts of the green CI run for THIS commit and publishes them. A
# missing or non-green run is a hard fail: no partial or stale publish.
#
# Set SMIX_SHIP_NAPI_DRYRUN=1 to run every publish here as `--dry-run`.
log "napi smix-node — collect prebuilds + publish"
NODE_DIR="$ROOT/crates/smix-node"
NAPI_DRY=""
[ "${SMIX_SHIP_NAPI_DRYRUN:-0}" = 1 ] && NAPI_DRY="--dry-run"

# Does the registry already carry this exact version? Nine publish legs
# run in sequence and any one of them can fail late; without this a rerun
# dies on the first package that already went out — the same defect the
# crates leg carried until 6.3.0 walked into it.
npm_has_version() {
  local pkg="$1" want="$2" esc
  esc="$(printf '%s' "$pkg" | sed 's|/|%2f|')"
  curl -sf -o /dev/null -A "smix-ship" "https://registry.npmjs.org/$esc/$want"
}

npm_publish_dir() {
  local dir="$1" pkg="$2"
  if [ -z "$NAPI_DRY" ] && npm_has_version "$pkg" "$VERSION"; then
    note "  npm publish $pkg@$VERSION — already published, skipping"
    return 0
  fi
  note "  npm publish $pkg@$VERSION${NAPI_DRY:+ (dry-run)}"
  ( cd "$dir" && bun publish --access public $NAPI_DRY ) || fail "npm publish $pkg"
}

HEAD_SHA="$(cd "$ROOT" && git rev-parse HEAD)"
RUN_ID="$(gh run list --repo goliajp/smix --workflow ci.yml --commit "$HEAD_SHA" \
  --json databaseId,conclusion --jq '[.[] | select(.conclusion=="success")][0].databaseId')" \
  || fail "gh run list failed — is gh authenticated for goliajp/smix?"
[ -n "$RUN_ID" ] || fail "no green ci.yml run for HEAD ($HEAD_SHA): push HEAD and let napi-prebuild build the three .node addons before shipping"

ART_DIR="$(mktemp -d)"
gh run download "$RUN_ID" --repo goliajp/smix --dir "$ART_DIR" \
  --pattern 'smix-node-*' || fail "gh run download of napi prebuilds failed"

# Generate the platform-agnostic loader (index.js / index.d.ts) once. This
# also produces the host's own .node, which we overwrite from the artifacts
# so all three come from the same reproducible CI build.
( cd "$NODE_DIR" && bunx napi build --platform --release ) || fail "napi loader build"
( cd "$NODE_DIR" && bunx napi create-npm-dirs ) || fail "napi create-npm-dirs"

# Place each downloaded .node into its per-triple subpackage. The platform
# short-name is in the file name (smix-node.<platform>.node).
found=0
while IFS= read -r nodefile; do
  base="$(basename "$nodefile")"                       # smix-node.darwin-arm64.node
  plat="${base#smix-node.}"; plat="${plat%.node}"      # darwin-arm64
  [ -d "$NODE_DIR/npm/$plat" ] || fail "no subpackage dir for platform $plat"
  cp "$nodefile" "$NODE_DIR/npm/$plat/" || fail "stage $base"
  found=$((found + 1))
done < <(find "$ART_DIR" -name '*.node')
[ "$found" = 3 ] || fail "expected 3 prebuilt .node addons, collected $found"

# Publish the three per-triple subpackages, then the main loader package.
for plat in darwin-arm64 darwin-x64 linux-x64-gnu; do
  npm_publish_dir "$NODE_DIR/npm/$plat" "@goliapkg/smix-node-$plat"
done
npm_publish_dir "$NODE_DIR" "@goliapkg/smix-node"

# --- publish the CLI as prebuilt binaries -----------------------------

# The same shape as the napi addon above, for the same reason: someone
# whose app is Swift or Kotlin should not need a Rust toolchain to get
# smix. The per-triple binaries come from the `cli-prebuild` CI matrix on
# native runners, so this collects that run's artifacts rather than
# cross-compiling here.
CLI_DIR="$ROOT/npm/smix-cli"
CLI_ART="$(mktemp -d)"
log "npm smix-cli — collect prebuilds + publish"
gh run download "$RUN_ID" --repo goliajp/smix --dir "$CLI_ART" \
  --pattern 'smix-cli-*' || fail "gh run download of the CLI prebuilds failed"

declare -a CLI_TRIPLES=(aarch64-apple-darwin darwin-arm64 x86_64-apple-darwin darwin-x64 \
                        x86_64-unknown-linux-gnu linux-x64-gnu)
i=0
while [ "$i" -lt "${#CLI_TRIPLES[@]}" ]; do
  triple="${CLI_TRIPLES[$i]}"
  plat="${CLI_TRIPLES[$((i + 1))]}"
  for exe in smix smix-mcp; do
    src="$CLI_ART/smix-cli-$triple/$exe"
    [ -f "$src" ] || fail "missing $exe for $triple in the CI artifacts"
    cp "$src" "$CLI_DIR/npm/$plat/$exe" || fail "stage $exe for $plat"
    chmod +x "$CLI_DIR/npm/$plat/$exe"
  done
  i=$((i + 2))
done

( cd "$CLI_DIR" && bun x tsc ) || fail "smix-cli launcher build"

for plat in darwin-arm64 darwin-x64 linux-x64-gnu; do
  npm_publish_dir "$CLI_DIR/npm/$plat" "@goliapkg/smix-cli-$plat"
done
npm_publish_dir "$CLI_DIR" "@goliapkg/smix-cli"

# --- publish npm ------------------------------------------------------

# v0.1.0 SDK ship cycle finding: `npm publish` crashes on nvm 26.5.0
# node ("Cannot find module npm.js"), `bun publish` works. Prefer bun.
( cd "$ROOT/npm/smix-rn" && bun run build ) || fail "smix-rn build"
npm_publish_dir "$ROOT/npm/smix-rn" "@goliapkg/smix"

# --- publish Maven Central -------------------------------------------

# In dry-run, publish to the local Maven repo (validates POM + signing +
# artifact assembly) instead of Maven Central.
# Both artifacts. `smix-probe` is the line the guides tell a consumer to
# write, and a coordinate that resolves to nothing is worse than no line:
# they would add it, see no change, and conclude the feature does not work.
# An ARRAY, not a string. Quoted as one word, gradle reads two tasks as a
# single task name and answers "cannot locate tasks that match
# ':sdk:publish :probe:publish'" — the whole publish leg fails, an hour in,
# on a shell quoting mistake rather than on anything about the release.
GRADLE_PUB_TASKS=(":sdk:publish" ":probe:publish")
[ "$SHIP_DRY" = 1 ] && GRADLE_PUB_TASKS=(":sdk:publishToMavenLocal" ":probe:publishToMavenLocal")
# Names both artifacts. Publishing two and logging one is the shape §14
# is about: the record has to say what happened, not half of it.
log "gradle ${GRADLE_PUB_TASKS[*]} — jp.golia.smix:{smix-sdk,smix-probe}:$VERSION"
# `|| fail` is not enough here. 6.3.0 found gpg exiting 0 while printing
# nothing at all — its database was held by a lock whose owner had died,
# so the export was empty and that empty string went to gradle as the
# signing key. Gradle then failed with "no configured signatory" a screen
# later, naming neither gpg nor the lock. An empty key is a failure even
# when the command that produced it says it succeeded.
# A lock this ship's own gpg left behind stops the NEXT ship, every time:
# 9.0.0 was held by pid 26149 from the 8.0.1 release the night before.
# Reporting it was not enough — the remedy is mechanical, so it is done
# here, but ONLY when the holder is provably gone. A live holder is
# somebody else's gpg and is left alone; the export below then fails as
# it always did.
#
# The two files are hardlinks of the same inode, so both names go or
# neither does. This says what it removed rather than doing it quietly:
# a lock disappearing without a word is indistinguishable from there
# never having been one.
GPG_LOCK="$HOME/.gnupg/public-keys.d/pubring.db.lock"
if [ -f "$GPG_LOCK" ]; then
  LOCK_PID="$(awk 'NR==1{print $1}' "$GPG_LOCK" 2>/dev/null)"
  if [ -n "$LOCK_PID" ] && ! ps -p "$LOCK_PID" >/dev/null 2>&1; then
    log "gpg keybox lock held by pid $LOCK_PID, which is gone — removing it and its hardlink"
    rm -f "$GPG_LOCK" "$HOME"/.gnupg/public-keys.d/.#lk*
    gpgconf --kill keyboxd >/dev/null 2>&1 || true
  else
    log "gpg keybox lock held by pid ${LOCK_PID:-?}, which is alive — leaving it"
  fi
fi

GPG_KEY="$(gpg --export-secret-keys --armor FBD802632CFAD78B 2>/dev/null)" \
  || fail "gpg export failed for signing key FBD802632CFAD78B"
[ -n "$GPG_KEY" ] \
  || fail "gpg exported an empty key for FBD802632CFAD78B — a dead process still holds the keybox lock. There are TWO files and they are hardlinks of each other: ~/.gnupg/public-keys.d/pubring.db.lock and the .#lk* beside it. Removing one leaves the other, and gpg goes on naming the dead pid. Check the pid inside the lock is gone (\`cat\` it, then \`ps -p\`), remove BOTH, and \`gpgconf --kill keyboxd\`."
( cd "$ROOT/android-runner" && \
  ORG_GRADLE_PROJECT_signingInMemoryKey="$GPG_KEY" \
  ORG_GRADLE_PROJECT_signingInMemoryKeyId=2CFAD78B \
  ORG_GRADLE_PROJECT_signingInMemoryKeyPassword="" \
  ./gradlew "${GRADLE_PUB_TASKS[@]}" --console=plain ) \
  || fail "gradle publish (${GRADLE_PUB_TASKS[*]})"

# --- tag Swift Package + push ----------------------------------------

if [ "$SHIP_DRY" = 1 ]; then
  note "tag swift-v$VERSION — SKIPPED (dry-run)"
else
  note "tag swift-v$VERSION + push"
  ( cd "$ROOT" && git tag -a "swift-v$VERSION" -m "Swift Package v$VERSION" && git push origin "swift-v$VERSION" ) \
    || fail "git tag + push"
fi

if [ "$SHIP_DRY" = 1 ]; then
  log "SHIP DRY-RUN COMPLETE — all gates green, every publish leg dry-run (cargo skipped, validated by CI)"
else
  # Ask the registries rather than assert on their behalf.
  #
  # This line used to read "live on crates.io + npm + Maven Central +
  # Swift Package", printed from control flow having asked none of
  # them. It was not merely unverified: Maven Central took three hours
  # to publish 6.5.0, so the sentence was false at the moment it was
  # printed, and every release since has been checked by hand instead.
  #
  # Maven is allowed to be late and never allowed to be claimed — the
  # verifier reports it as still to come and says so in the summary.
  log "verify what the registries took"
  if bash "$ROOT/scripts/release/verify-published.sh" "$VERSION" \
       2>&1 | tee /tmp/smix-ship-verify.log; then
    note "SHIP COMPLETE — see the line above for what was confirmed"
  else
    fail "published, but a channel does not have v$VERSION — see /tmp/smix-ship-verify.log. \
The publish legs ran; this is about what the registries actually serve."
  fi
fi
