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

log() { printf '[ship] %s\n' "$*"; }
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

# --- rust workspace tests ---------------------------------------------
# The workspace suite (830+ tests) had NO gate: ship.sh ran smoke + swift
# + lints while `cargo test` was left to whoever remembered. That is how
# /tap shipped a response body the wire crate deserialized to all-None
# without one red test. Non-bypassable, like the swift suite above.
log "cargo test --workspace"
( cd "$ROOT" && cargo test --workspace ) > /tmp/smix-ship-cargo-test.log 2>&1 \
  || fail "cargo test FAILED — see /tmp/smix-ship-cargo-test.log"

# --- TS SDK tests ------------------------------------------------------
log "npm/smix-rn typecheck + vitest"
( cd "$ROOT/npm/smix-rn" && bun run typecheck && bun run test ) \
    > /tmp/smix-ship-ts-test.log 2>&1 \
  || fail "TS SDK tests FAILED — see /tmp/smix-ship-ts-test.log"

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

# --- android instrumentation (device) ----------------------------------
# The :sdk assertion suite on a pinned emulator. Placed early — before
# fuzz, clippy, semver and anything that publishes — so a missing
# emulator costs seconds rather than being discovered after the long
# work. Device selection and the deadline live in the delegate, not
# here: keeping them inline would put an adb call in a script the
# PreToolUse guard can no longer read, and the delegate carries the same
# emulator-only rule the guard enforces.
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
bash "$ROOT/scripts/release/android-behaviour-gate.sh" \
  || fail "android behaviour gate FAILED — see the verdict above"

# --- route conformance ------------------------------------------------
# Derives the served-route list from both runner sources and sweeps every
# shipped file for phantom endpoints. It caught 13 fictional routes in
# review, then sat unwired while ship.sh ran everything except it.
log "route conformance"
python3 "$ROOT/scripts/dev/route-conformance.py" > /tmp/smix-ship-routes.log 2>&1 \
  || fail "route conformance FAILED — see /tmp/smix-ship-routes.log"

# --- android gate scan -------------------------------------------------
# Re-derives the Android modules and checks each one's test tasks are run
# by preflight, CI and this script. The app module's unit tests were
# outside all three for the whole of v1 and v2, which is how a header
# nobody read and a placeholder package both shipped.
log "android gate scan"
python3 "$ROOT/scripts/dev/android-gate-scan.py" > /tmp/smix-ship-android-gate.log 2>&1 \
  || fail "android gate scan FAILED — an Android test task is outside the gates (see /tmp/smix-ship-android-gate.log)"

# --- audit ledger ------------------------------------------------------
# Re-evaluates every citation in docs/audit-ledger.md. That table records
# which known defects are still live, and its predecessor drifted badly
# enough that three of five sampled entries had been fixed while still
# reading as open. Shipping against a stale account of what is broken is
# how a defect reaches users with a note saying someone already knew.
log "audit ledger"
python3 "$ROOT/scripts/dev/audit-ledger-scan.py" > /tmp/smix-ship-ledger.log 2>&1 \
  || fail "audit ledger scan FAILED — a citation no longer holds; re-verify that row (see /tmp/smix-ship-ledger.log)"

# --- hygiene scan ------------------------------------------------------
# Development noise and dead doc pointers in everything a reader outside
# this repo can see. Its own docstring says it exits non-zero "so it can
# gate a release" — and until now this script mentioned it only in the
# two comments below, never calling it. preflight ran it, CI ran it, the
# release did not.
log "hygiene scan"
python3 "$ROOT/scripts/dev/hygiene-scan.py" > /tmp/smix-ship-hygiene.log 2>&1 \
  || fail "hygiene scan FAILED — shipped sources carry development noise or dead doc pointers (see /tmp/smix-ship-hygiene.log)"

# --- workflow scan -----------------------------------------------------
# The development contract survives a clone: charter and rule cards
# tracked, hook scripts present and wired, guards tested, no GNU-only
# tools, and every source gate running in all three places. That last
# check is what found this script missing two gates.
log "workflow scan"
python3 "$ROOT/scripts/dev/workflow-scan.py" > /tmp/smix-ship-workflow.log 2>&1 \
  || fail "workflow scan FAILED — see /tmp/smix-ship-workflow.log"

# --- scope promise scan ------------------------------------------------
# Every promise in the scope file still matches what exists. `--stable`
# was promised, never built, never withdrawn, and agreed with by four
# documents — three of them gitignored — for seven months. A shipped
# promise may not cite a document as evidence it was implemented.
log "scope promise scan"
python3 "$ROOT/scripts/dev/scope-promise-scan.py" > /tmp/smix-ship-scope.log 2>&1 \
  || fail "scope promise scan FAILED — the scope file and the tree disagree (see /tmp/smix-ship-scope.log)"

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
# Build the workspace's own smix release for the gate — a global `smix` on
# PATH is whatever version was installed some other day, and a mismatch
# between it and the runner sources this workspace ships is exactly how a
# pre-fold binary drove the post-fold runner in dry-run and the gate turned
# red on a real driver/runner drift.
log "cargo build -p smix-cli --release (for corpus gate)"
( cd "$ROOT" && cargo build -p smix-cli --release ) || fail "cargo build smix-cli --release"

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

# --- fact scan --------------------------------------------------------
# hygiene-scan asks "does it read as internal?"; fact-scan asks "is it
# true?" — install coordinates vs the workspace version, tool-count
# claims vs #[tool(] registrations, and noise inside the quoted strings
# hygiene-scan structurally cannot see.
log "fact scan"
python3 "$ROOT/scripts/dev/fact-scan.py" > /tmp/smix-ship-facts.log 2>&1 \
  || fail "fact-scan FAILED — a user-facing surface states something untrue (see /tmp/smix-ship-facts.log)"

# --- llms.txt freshness ----------------------------------------------
# llms.txt / llms-full.txt are a projection of VERB_TABLE + the Selector
# enum + the workspace version. Gate them like the FFI bindings so the
# AI-facing index can't drift from the sources it mirrors.
log "llms.txt freshness"
# The AI tier sits beside the resolver, not inside it. Nothing in the
# type system says so, and the check that does say so was running in no
# gate at all when this line was added.
log "fence check"
bash "$ROOT/scripts/dev/fence-check.sh" > /tmp/smix-ship-fence.log 2>&1 \
  || fail "fence-check FAILED — the sense path reaches smix-ai-tier (see /tmp/smix-ship-fence.log)"

python3 "$ROOT/scripts/dev/gen-llms.py" --check > /tmp/smix-ship-llms.log 2>&1 \
  || fail "llms.txt/llms-full.txt are stale — run scripts/dev/gen-llms.py and commit (see /tmp/smix-ship-llms.log)"

# --- clippy -----------------------------------------------------------
# `warnings = "deny"` in the workspace lints covers rustc, not clippy, and
# nothing ran clippy — so four lints sat in the tree, one of them a doc
# comment detached from the type it described in a stone crate. Clean at
# the time this was added; here so it stays that way.
log "clippy"
( cd "$ROOT" && cargo clippy --workspace --all-targets ) > /tmp/smix-ship-clippy.log 2>&1 \
  || fail "clippy FAILED — see /tmp/smix-ship-clippy.log"

# --- cargo-semver-checks ----------------------------------------------
# Confirms the crates' API changes are the major break the 2.0.0 bump
# claims. Runs when the tool is installed; a ship must have it. It
# validates in-place breaks like SimctlError → DeviceControlError, not
# renames, which the version bump covers.
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
      SEMVER_EXCLUDE+=(--exclude "$UNCHECKABLE")
      SEMVER_SKIPPED+=("$UNCHECKABLE")
  done
  # Report coverage from the run's own output, not from the exclusion
  # count. The tool also skips crates silently — anything with
  # `publish = false` or no library target — so "4 excluded" would have
  # read as "26 checked" when 21 were. The number that matters is how
  # many it actually looked at.
  SEMVER_CHECKED=$(grep -c '^ *Checking ' "$SEMVER_LOG" || true)
  SEMVER_TOTAL=$(cd "$ROOT" && cargo metadata --no-deps --format-version 1 |
      python3 -c 'import json,sys; print(len(json.load(sys.stdin)["packages"]))')
  log "semver-checks: $SEMVER_CHECKED of $SEMVER_TOTAL crates checked"
  if [ ${#SEMVER_SKIPPED[@]} -gt 0 ]; then
      log "semver-checks: excluded by name after the tool refused them: ${SEMVER_SKIPPED[*]}"
  fi
else
  fail "cargo-semver-checks not installed — cargo install cargo-semver-checks (required for a 2.0.0 ship)"
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

# --- publish crates.io (DAG order) -----------------------------------

log "publish crates.io DAG at $VERSION"
CRATES=(
  smix-sim-health smix-runner-sources
  smix-screen smix-selector smix-input smix-error
  smix-verbs smix-metro-log smix-adb smix-ai-tier
  smix-runner-wire smix-selector-resolver smix-fixture
  smix-annotate smix-migrate smix-authoring-ir
  smix-store smix-simctl smix-runner-client
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
SHIP_DRY="${SMIX_SHIP_DRYRUN:-0}"
[ "$SHIP_DRY" = 1 ] && export SMIX_SHIP_NAPI_DRYRUN=1

for c in "${CRATES[@]}"; do
  if [ "$SHIP_DRY" = 1 ]; then
    log "cargo publish -p $c — SKIPPED (dry-run; interdependent crates validated by CI)"
    continue
  fi
  log "cargo publish -p $c"
  # v1.0.4+ pattern from prior ship cycles: crates.io rate-limits at
  # ~1-2 publishes per 90s window under aggressive sequential publish.
  # Retry-with-backoff on 429/already-in-progress until success.
  attempt=0
  until ( cd "$ROOT" && cargo publish -p "$c" ) 2>&1 | tee /tmp/pub-$c.log | grep -qE "Published|already exists|already uploaded"; do
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
  log "  npm publish @goliapkg/smix-node-$plat@$VERSION"
  ( cd "$NODE_DIR/npm/$plat" && bun publish --access public $NAPI_DRY ) \
    || fail "bun publish smix-node-$plat"
done
log "  npm publish @goliapkg/smix-node@$VERSION"
( cd "$NODE_DIR" && bun publish --access public $NAPI_DRY ) \
  || fail "bun publish smix-node"

# --- publish npm ------------------------------------------------------

log "npm publish @goliapkg/smix@$VERSION${NAPI_DRY:+ (dry-run)}"
# v0.1.0 SDK ship cycle finding: `npm publish` crashes on nvm 26.5.0
# node ("Cannot find module npm.js"), `bun publish` works. Prefer bun.
if command -v bun >/dev/null 2>&1; then
  ( cd "$ROOT/npm/smix-rn" && bun run build && bun publish --access public $NAPI_DRY ) \
    || fail "bun publish"
else
  ( cd "$ROOT/npm/smix-rn" && npm publish --access public ${NAPI_DRY:+--dry-run} ) || fail "npm publish"
fi

# --- publish Maven Central -------------------------------------------

# In dry-run, publish to the local Maven repo (validates POM + signing +
# artifact assembly) instead of Maven Central.
GRADLE_PUB_TASK=":sdk:publish"
[ "$SHIP_DRY" = 1 ] && GRADLE_PUB_TASK=":sdk:publishToMavenLocal"
log "gradle $GRADLE_PUB_TASK jp.golia.smix:smix-sdk:$VERSION"
GPG_KEY="$(gpg --export-secret-keys --armor FBD802632CFAD78B 2>/dev/null)" \
  || fail "gpg export failed for signing key FBD802632CFAD78B"
( cd "$ROOT/android-runner" && \
  ORG_GRADLE_PROJECT_signingInMemoryKey="$GPG_KEY" \
  ORG_GRADLE_PROJECT_signingInMemoryKeyId=2CFAD78B \
  ORG_GRADLE_PROJECT_signingInMemoryKeyPassword="" \
  ./gradlew "$GRADLE_PUB_TASK" --console=plain ) \
  || fail "gradle publish"

# --- tag Swift Package + push ----------------------------------------

if [ "$SHIP_DRY" = 1 ]; then
  log "tag swift-v$VERSION — SKIPPED (dry-run)"
else
  log "tag swift-v$VERSION + push"
  ( cd "$ROOT" && git tag -a "swift-v$VERSION" -m "Swift Package v$VERSION" && git push origin "swift-v$VERSION" ) \
    || fail "git tag + push"
fi

if [ "$SHIP_DRY" = 1 ]; then
  log "SHIP DRY-RUN COMPLETE — all gates green, every publish leg dry-run (cargo skipped, validated by CI)"
else
  log "SHIP COMPLETE — v$VERSION live on crates.io + npm + Maven Central + Swift Package"
fi
