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
# shellcheck source=../lib/android-runner-log.sh
. "$ROOT/scripts/lib/android-runner-log.sh"

# --- pre-flight -------------------------------------------------------

if [[ "$BYPASS" != "--i-know-what-im-doing" ]]; then
  # Require smoke pass in the last hour.
  if [[ ! -f "$STAMP" ]] || \
     [[ $(( $(date +%s) - $(stat -f %m "$STAMP" 2>/dev/null || echo 0) )) -gt 3600 ]]; then
    log "smoke gate stale or missing — running smoke first"
    # The binary about to be published. The smoke was the one gate
    # handed none, and took the PATH's — the last release installed — so
    # a ship smoked the previous version. Built inside this step so its
    # minutes stay with the permission they are for.
    ( cd "$ROOT" && cargo build -p smix-cli --release ) || fail "cargo build smix-cli --release (for the smoke)"
    SMIX_BIN="$ROOT/target/release/smix" "$SMOKE" || fail "smoke gate FAILED — refusing to publish"
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

# --- what this script judges, and what it takes from CI --------------
#
# CI runs every source judgement -- fmt, clippy, rustdoc, the workspace
# tests, the runner tarballs, the kotlin build and the source gates -- on
# this exact commit before anything here starts, and
# `ci-is-green-on-this-commit` refuses unless every job of that run passed
# and the worktree is clean. So the tree being published is byte for byte
# the tree CI judged, and running those judgements again here proved the
# same thing a second time: it was 94% of a three-hour run and changed no
# verdict. What stays is what CI cannot see: the devices, this machine's
# devicectl, Xcode and interpreter, the npm token, cargo-semver-checks
# against what crates.io holds, and the publish legs themselves. The
# napi loader, like the addons and the CLI binaries, is taken from CI's
# run. `cargo publish` still compiles every crate from its package
# before it uploads.


# --- CI is green on this commit ---------------------------------------
# First, because the ship cannot publish without it and everything below
# is wasted if it is false. It used to be asked at line 1349, beside the
# `gh run download` that consumes its answer: 10.0.0's sixth run reached
# it at 143m58s, having passed every gate, and failed on an unpushed
# commit. The npm and crates.io legs had already gone out by then.
#
# It also reads per job now rather than the run-level conclusion, and it
# refuses on a dirty worktree — CI judged HEAD, this publishes the
# worktree.
log "the ci-green gate can still go red"
python3 "$ROOT/scripts/dev/ci-is-green-on-this-commit.test.py" > /tmp/smix-ship-cigreen-test.log 2>&1 \
  || fail "ci-is-green-on-this-commit no longer goes red on broken input — in particular it may have gone back to reading the run-level conclusion, which is green while a continue-on-error job is red. See /tmp/smix-ship-cigreen-test.log"

log "ci is green on this commit"
CI_GREEN_OUT="$(python3 "$ROOT/scripts/dev/ci-is-green-on-this-commit.py" 2>&1)" || {
  printf '%s\n' "$CI_GREEN_OUT"
  fail "ci-is-green-on-this-commit FAILED — the release downloads the .node addons and CLI binaries from that run's artifacts, so there is nothing to publish until it is green"
}
printf '%s\n' "$CI_GREEN_OUT" | sed 's/^/  /'
RUN_ID="$(printf '%s\n' "$CI_GREEN_OUT" | tail -1)"
HEAD_SHA="$(cd "$ROOT" && git rev-parse HEAD)"

# Ship only, never CI: CI has no device hub to read.
log "the device hub reader can still go red"
bash "$ROOT/scripts/dev/device-hub-shows-a-boot.sh" --selftest > /tmp/smix-ship-device-hub-selftest.log 2>&1 \
  || fail "device-hub-shows-a-boot cannot tell an unread window from an unmoved one (see /tmp/smix-ship-device-hub-selftest.log)"

# Ship only, never CI: its input is the devicectl installed on this
# machine, and the ubuntu job has none to ask.
log "a refusal that says devicectl cannot is checked against the installed devicectl"
python3 "$ROOT/scripts/dev/a-refusal-devicectl-outgrew.test.py" \
  > /tmp/smix-ship-devicectl-outgrew-selftest.log 2>&1 \
  || fail "a-refusal-devicectl-outgrew no longer goes red on broken input (see /tmp/smix-ship-devicectl-outgrew-selftest.log)"
python3 "$ROOT/scripts/dev/a-refusal-devicectl-outgrew.py" \
  > /tmp/smix-ship-devicectl-outgrew.log 2>&1 \
  || fail "a physical-device refusal says devicectl lacks a verb it has (see /tmp/smix-ship-devicectl-outgrew.log)"

# The napi loader (index.js / index.d.ts) is taken from CI's run on this
# commit at publish time, not regenerated here: see the publish block.
# --- devices first ---------------------------------------------------
#
# The release binary these gates drive, and then the gates. Ahead of
# every expensive judgement, because a device that is absent, held by
# somebody else, or unregistered can say so in seconds -- and this
# release found each of those three the hard way, hours in.
#
# `cheap-gates-come-first` named this four runs in a row, and each
# time moving them one step earlier only revealed the next expensive
# thing in front of them. The answer was not another step.

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
# Every gate from here on drives this build, named once rather than gate
# by gate: the ones not handed a binary took their own default — the
# Python device gates `./target/release/smix` relative to the working
# directory, the MCP parity scan whichever build was newer. They resolve
# through scripts/lib/e2e-binary.sh now, which honours SMIX_BIN.
export SMIX_BIN="$ROOT/target/release/smix"


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
# `runner up` decide that from the lease and name the holder; a second
# copy of that
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
# One emulator for every Android leg, chosen once, and by whose it is --
# not by which port it answers on. This used to default to emulator-5554,
# a slot: on 2026-09-24 the AVD in that slot was a consumer's
# (`qip-consumer-36`), and the release checklist also exported
# ANDROID_SERIAL=emulator-5554, which every unpinned adb honours. Run as
# written, the ship would have installed our fixture and runner on their
# emulator. The instrumentation and behaviour gates already refused to
# guess (pick-dev-emulator: only a device this machine's ledger says smix
# booted); these legs were the half that still guessed. The same picker
# now answers for all of them, and the gates are handed its answer so the
# four cannot drive two different emulators.
if [[ -n "${SMIX_V10_ANDROID:-}" ]]; then
  ANDROID_DEVICE="$SMIX_V10_ANDROID"
else
  ANDROID_DEVICE="$(bash "$ROOT/scripts/dev/pick-dev-emulator.sh" 2>&1)" \
    || fail "no emulator this release may drive:
$ANDROID_DEVICE"
fi
export SMIX_ANDROID_SERIAL="${SMIX_ANDROID_SERIAL:-$ANDROID_DEVICE}"
[[ "$SMIX_ANDROID_SERIAL" == "$ANDROID_DEVICE" ]] \
  || fail "SMIX_ANDROID_SERIAL=$SMIX_ANDROID_SERIAL and SMIX_V10_ANDROID=$ANDROID_DEVICE name two emulators; the Android legs must drive one"
# Free once is not free enough. The consumer's batch runs flows back to
# back with gaps of seconds, and these three legs take minutes: a check
# that proceeds on the first idle sample walks into the next flow.
# Measured 2026-08-30 -- the ledger said free, the instrumentation gate
# passed, and `runner up` in the behaviour gate a minute later was
# refused by name. So: the device has to stay free for a whole minute
# before this goes on.
android_free_for_60s() {
  local i
  for i in $(seq 1 6); do
    # Sleeping on the busy branch too, or this waits for nothing: the
    # first draft returned immediately when the device was held and the
    # caller retried immediately, so fifteen "attempts" went by in under
    # a second and the fifteen-minute wait was a fifteen-minute claim.
    if android_device_is_busy; then
      sleep 10
      return 1
    fi
    sleep 10
  done
  return 0
}
# --- workflow scan -----------------------------------------------------
# The development contract survives a clone: charter and rule cards
# tracked, hook scripts present and wired, guards tested, no GNU-only
# tools, and every source gate running in all three places. That last
# check is what found this script missing two gates.
log "workflow scan"
python3 "$ROOT/scripts/dev/workflow-scan.py" > /tmp/smix-ship-workflow.log 2>&1 \
  || fail "workflow scan FAILED — see /tmp/smix-ship-workflow.log"

# --- swift-bridge unit tests ------------------------------------------
# NOT bypassable. This suite sat outside the gate long enough for a test
# asserting a two-release-old contract to fail unnoticed for 15+ releases.
# ~18 s.
log "swift-bridge unit tests"
( cd "$ROOT/swift-bridge" && swift test ) > /tmp/smix-ship-swift-test.log 2>&1 \
  || fail "swift-bridge tests FAILED — see /tmp/smix-ship-swift-test.log"

# --- ffi bindings -----------------------------------------------------
# The Swift and Kotlin bindings are committed next to binary blobs, and
# nothing regenerated them: the build scripts Package.swift and
# build.gradle.kts name did not exist. Clean at the time this was added; here
# so the boundary cannot drift away from the crate again.
log "ffi bindings"
# --against-source here and nowhere else: a commit on develop may leave
# the libraries behind the tree, a release may not. The ship does not
# rebuild them itself — what is published has to be a commit CI has seen.
"$ROOT/scripts/dev/ffi-bindings-fresh.sh" --against-source > /tmp/smix-ship-ffi-bindings.log 2>&1 \
  || fail "FFI bindings or libraries are not what this tree builds — see /tmp/smix-ship-ffi-bindings.log; run scripts/sdk/regenerate-bindings.sh, commit, and release that commit"
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

# --- the two device chains, side by side ------------------------------
# The Android emulator and the iOS simulator share nothing: no device, no
# runner, no port. Run in turn they cost the sum of the two; run side by
# side they cost the longer. Each chain logs to a file of its own so the
# two do not interleave, brings down what it started when it ends -- red
# or green -- and closes its own last profile line: a background subshell
# does not inherit this script's EXIT trap.
android_chain() {
  SHIP_LAST=""
  # A deadline, not a number of attempts. Counting attempts made the budget
  # depend on which branch each one took: fifteen busy attempts are 150
  # seconds, fifteen free ones are fifteen minutes, and the log said "up to
  # 15m" either way. Measured 2026-08-30: it gave up after three minutes
  # and said it had waited fifteen.
  log "android: waiting for $ANDROID_DEVICE to stay free for a minute"
  if android_device_is_busy; then
    log "android: $ANDROID_DEVICE is held by another process — waiting for a clear minute, up to 15m"
  fi
  android_wait_until=$(( SECONDS + 900 ))
  while [ "$SECONDS" -lt "$android_wait_until" ]; do
    android_free_for_60s && break
  done
  android_device_is_busy \
    && log "android: $ANDROID_DEVICE still held after 15m — letting the gates judge it" \
    || log "android: $ANDROID_DEVICE has been free for a minute"

  log "android instrumentation (device)"
  bash "$ROOT/scripts/release/android-instrumentation-gate.sh" \
    || fail "android instrumentation gate FAILED — see the verdict above. To give it an emulator it may \
  drive, start one through the ledger: smix sim boot sim-smix-android-01 (an emulator started by hand \
  writes no ledger, and the picker is right to refuse it)"

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
  V10_DEVICE="$ANDROID_DEVICE"

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
    # `adb` has to still be able to see it. This emulator is managed by
    # another session and went away and came back twice during dry-run
    # nineteen; the third time it was gone for the one minute this leg
    # needed, and `runner up` refused with `adb has no ready device` --
    # after the instrumentation gate had passed 4/4 and the behaviour gate
    # 14/14 on it, minutes earlier. Waiting for the device to be free is
    # not the same as waiting for it to be there.
    for _ in $(seq 1 60); do
      adb devices 2>/dev/null | grep -qE "^${V10_DEVICE}[[:space:]]+device" && break
      sleep 5
    done
    adb devices 2>/dev/null | grep -qE "^${V10_DEVICE}[[:space:]]+device" \
      || fail "v10: adb still does not list $V10_DEVICE as ready after 5m.
      This is about the emulator being attached, not about the runner.
      Attached now: $(adb devices 2>/dev/null | tail -n +2 | tr '\n' ' ')"

    log "v10: runner up on $V10_DEVICE:$V10_PORT"
    V10_LOG_SINCE="$(android_device_now "$V10_DEVICE" || true)"
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

  # A red here keeps what the runner logged: each route, its answer and
  # where its time went.
  v10_fail() {
    local n
    n="$(collect_android_runner_log "$V10_DEVICE" "${V10_LOG_SINCE:-}" /tmp/smix-ship-v10-runner-routes.log || true)"
    fail "$1 (the runner's log, $n route line(s): /tmp/smix-ship-v10-runner-routes.log)"
  }

  log "v10: two perception paths agree"
  python3 "$ROOT/scripts/dev/two-paths-agree.py" --device "$V10_DEVICE" \
    --port "$V10_PORT" --min-both 16 --min-bounds-compared 16 --focus compose_input \
    || v10_fail "two-paths-agree FAILED — the semantics and accessibility readers disagree"

  # The same reader, on the screen that has a View hosted inside Compose.
  #
  # It drove one screen for two majors and passed the whole time, while the
  # probe could not see into an `AndroidView` at all — the claim was right
  # and its subject never appeared. A consumer found that for us.
  log "v10.2: the two paths agree where Compose hosts a View"
  python3 "$ROOT/scripts/dev/two-paths-agree.py" --device "$V10_DEVICE" \
    --port "$V10_PORT" --activity .InteropActivity --min-both 8 \
    --min-bounds-compared 8 --prove-differences-exhibited --focus fixture_interop_input \
    || v10_fail "two-paths-agree FAILED on the interop screen"

  log "v10: the three that went red"
  python3 "$ROOT/scripts/dev/the-three-that-went-red.py" --device "$V10_DEVICE" \
    --port "$V10_PORT" \
    || v10_fail "the-three-that-went-red FAILED — a 6.4.0 root cause is unguarded again"

  log "v10: a wait that does not end early"
  python3 "$ROOT/scripts/dev/a-wait-that-does-not-end-early.py" --device "$V10_DEVICE" \
    --port "$V10_PORT" \
    || v10_fail "a-wait-that-does-not-end-early FAILED"

  log "v10: a semantics action is not a touch"
  python3 "$ROOT/scripts/dev/a-semantics-action-is-not-a-touch.py" --device "$V10_DEVICE" \
    --port "$V10_PORT" \
    || v10_fail "a-semantics-action-is-not-a-touch FAILED — the probe's action surface grew a touch substitute"

  v10_runner_down
  trap - EXIT
  ship_profile_close
}

ios_chain() {
  SHIP_LAST=""
  trap ship_profile_close EXIT
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

  trap - EXIT
  ship_profile_close
}

# --- cargo-semver-checks, beside the device chains ---------------------
# Asks crates.io what it holds, so it stays here rather than in CI: on
# develop between releases it would judge every commit against the last
# published version before the version has been raised. It needs no
# device, so it runs beside the chains, as a background task on the
# efficiency cores (taskpolicy -b) — the device gates are the ones that
# go red when the machine is loaded, and this must not be what loads it.
semver_lane() {
  SHIP_LAST=""
  trap ship_profile_close EXIT
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
        if ( cd "$ROOT" && taskpolicy -b nice -n 19 cargo semver-checks check-release --workspace \
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
  trap - EXIT
  ship_profile_close
}

# The step before this line is still open in the profile; close it here
# so neither chain's time is charged to it.
ship_profile_close
SHIP_LAST=""
note "device chains: android on $ANDROID_DEVICE, ios on $SMIX_CORPUS_SIM, and cargo-semver-checks — side by side"
android_chain > /tmp/smix-ship-android-chain.log 2>&1 & ANDROID_CHAIN_PID=$!
ios_chain > /tmp/smix-ship-ios-chain.log 2>&1 & IOS_CHAIN_PID=$!
semver_lane > /tmp/smix-ship-semver-lane.log 2>&1 & SEMVER_LANE_PID=$!
wait "$ANDROID_CHAIN_PID" && ANDROID_CHAIN_RC=0 || ANDROID_CHAIN_RC=$?
wait "$IOS_CHAIN_PID" && IOS_CHAIN_RC=0 || IOS_CHAIN_RC=$?
wait "$SEMVER_LANE_PID" && SEMVER_LANE_RC=0 || SEMVER_LANE_RC=$?
cat /tmp/smix-ship-android-chain.log /tmp/smix-ship-ios-chain.log /tmp/smix-ship-semver-lane.log
[ "$ANDROID_CHAIN_RC" = 0 ] \
  || fail "the android device chain FAILED (exit $ANDROID_CHAIN_RC) — see /tmp/smix-ship-android-chain.log"
[ "$IOS_CHAIN_RC" = 0 ] \
  || fail "the ios device chain FAILED (exit $IOS_CHAIN_RC) — see /tmp/smix-ship-ios-chain.log"
[ "$SEMVER_LANE_RC" = 0 ] \
  || fail "cargo-semver-checks FAILED (exit $SEMVER_LANE_RC) — see /tmp/smix-ship-semver-lane.log and /tmp/smix-ship-semver.log"
SHIP_TPREV="$(date +%s)"


# --- the checkpoint evidence, run by someone other than its author ----
# Every `*-e2e.sh` is a checkpoint's proof, and until this line none of
# them ran here: they were written, run once by hand the day they landed,
# and never again. One had been red since the checkpoint after it — C7
# changed the rectangle C5's Android leg picks its subject from, and
# nothing said so for six checkpoints.
#
# The tier is what counts: a leg that could not judge (exit 2) is named,
# a leg that failed (exit 1) fails this, and everything skipping is a
# failure of its own — a measurement that did not happen must not look
# like one that passed.
#
# One binary for all of them, the release one this ship built, for the
# reason in scripts/lib/e2e-binary.sh.
log "device e2e tier"
SMIX_E2E_UDID="$SMIX_CORPUS_SIM" \
SMIX_BIN="$ROOT/target/release/smix" \
  bash "$ROOT/scripts/release/device-e2e-tier.sh" \
    > /tmp/smix-ship-e2e-tier.log 2>&1 \
  || fail "device e2e tier FAILED — see /tmp/smix-ship-e2e-tier.log"

# fuzz smoke runs in CI (ci.yml `fuzz-smoke`) on this commit, and
# ci-green above requires it; cargo-semver-checks ran beside the chains.

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
# --- publish crates.io (DAG order) -----------------------------------
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
    note "cargo publish -p $c — already $VERSION on crates.io, skipping"
    continue
  fi
  note "cargo publish -p $c"
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
note "napi smix-node — collect prebuilds + publish"
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

# RUN_ID comes from the gate at the top of this script. It used to be
# fetched here, which is where it is consumed — 144 minutes in, after
# every gate and after crates.io had gone out. See that gate.

ART_DIR="$(mktemp -d)"
gh run download "$RUN_ID" --repo goliajp/smix --dir "$ART_DIR" \
  --pattern 'smix-node-*' || fail "gh run download of napi prebuilds failed"

# The platform-agnostic loader (index.js / index.d.ts) comes from the same
# CI run as the three addons (`smix-node-loader`, uploaded by ts-sdk), so
# the loader and what it loads are one build of one commit. It used to be
# regenerated here by a release build of smix-node — 482 s cold — and
# diffed against itself.
for f in index.js index.d.ts; do
  src="$(find "$ART_DIR" -path '*smix-node-loader*' -name "$f" | head -1)"
  [ -s "$src" ] || fail "the napi loader $f is not in CI run $RUN_ID's smix-node-loader artifact"
  cp "$src" "$NODE_DIR/$f" || fail "stage the napi loader $f"
done
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
note "gradle ${GRADLE_PUB_TASKS[*]} — jp.golia.smix:{smix-sdk,smix-probe}:$VERSION"
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
    note "gpg keybox lock held by pid $LOCK_PID, which is gone — removing it and its hardlink"
    rm -f "$GPG_LOCK" "$HOME"/.gnupg/public-keys.d/.#lk*
    gpgconf --kill keyboxd >/dev/null 2>&1 || true
  else
    note "gpg keybox lock held by pid ${LOCK_PID:-?}, which is alive — leaving it"
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
    note "published — see the line above for what was confirmed"
  else
    fail "published, but a channel does not have v$VERSION — see /tmp/smix-ship-verify.log. \
The publish legs ran; this is about what the registries actually serve."
  fi

  # The machine that shipped it runs it. This was a paragraph in the
  # release list, and sixteen days after 10.0.0 this machine had smix
  # 9.0.0, no smix-mcp, and four different plugin versions across six
  # Claude profiles. After the registries have answered, because it
  # installs from them.
  log "install v$VERSION on this machine"
  if bash "$ROOT/scripts/release/install-on-this-machine.sh" "$VERSION" \
       2>&1 | tee /tmp/smix-ship-this-machine.log; then
    note "SHIP COMPLETE — published, and this machine runs v$VERSION"
  else
    fail "v$VERSION is published and nothing about that is in doubt; THIS MACHINE is not running it — \
see /tmp/smix-ship-this-machine.log, then: bash scripts/release/install-on-this-machine.sh $VERSION"
  fi
fi
