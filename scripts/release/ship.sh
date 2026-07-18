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

# --- Kotlin SDK unit tests --------------------------------------------
# Compiles the generated Kotlin bindings AND runs the sdk unit suite.
# The bindings previously first compiled during `gradlew :sdk:publish` —
# publish-time was the first compile, which is exactly how the
# DriveError field/Throwable.message collision reached a release branch.
log "android sdk unit tests (compiles kotlin bindings)"
( cd "$ROOT/android-runner" && ./gradlew :sdk:testDebugUnitTest --console=plain ) \
    > /tmp/smix-ship-kotlin-test.log 2>&1 \
  || fail "Kotlin sdk tests FAILED — see /tmp/smix-ship-kotlin-test.log"

# --- route conformance ------------------------------------------------
# Derives the served-route list from both runner sources and sweeps every
# shipped file for phantom endpoints. It caught 13 fictional routes in
# review, then sat unwired while ship.sh ran everything except it.
log "route conformance"
python3 "$ROOT/scripts/dev/route-conformance.py" > /tmp/smix-ship-routes.log 2>&1 \
  || fail "route conformance FAILED — see /tmp/smix-ship-routes.log"

# --- corpus gate (real sim) -------------------------------------------
# Runs the bootstrap corpus end-to-end on a simulator. Device selection
# mirrors the smoke: explicit env first, else the first booted sim.
if [[ -z "${SMIX_CORPUS_SIM:-}" ]]; then
  SMIX_CORPUS_SIM="$(xcrun simctl list devices booted -j 2>/dev/null \
    | jq -r '[.devices | to_entries[] | .value[]] | .[0].udid // empty')"
fi
[[ -n "$SMIX_CORPUS_SIM" ]] \
  || fail "corpus gate needs SMIX_CORPUS_SIM or a booted sim"
log "corpus gate on $SMIX_CORPUS_SIM"
SMIX_CORPUS_SIM="$SMIX_CORPUS_SIM" "$ROOT/scripts/release/corpus-gate.sh" \
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
# claims. Runs when the tool is installed; a ship must have it. It is blind
# to crate renames (recorder-ir → authoring-ir has no old-name baseline) and
# to brand-new crates — it validates in-place breaks like SimctlError →
# DeviceControlError, not the renames, which the version bump covers.
if command -v cargo-semver-checks >/dev/null 2>&1; then
  log "cargo-semver-checks"
  ( cd "$ROOT" && cargo semver-checks check-release --workspace ) \
      > /tmp/smix-ship-semver.log 2>&1 \
    || fail "cargo-semver-checks FAILED — see /tmp/smix-ship-semver.log"
else
  fail "cargo-semver-checks not installed — cargo install cargo-semver-checks (required for a 2.0.0 ship)"
fi

# --- version match ---------------------------------------------------

WORKSPACE_VERSION="$(grep '^version' "$ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')"
[[ "$WORKSPACE_VERSION" == "$VERSION" ]] \
  || fail "workspace Cargo.toml version=$WORKSPACE_VERSION doesn't match arg $VERSION"

NPM_VERSION="$(cd "$ROOT/npm/smix-rn" && node -p 'require("./package.json").version')"
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
  smix-simctl smix-runner-client smix-driver
  smix-host-coord-resolver
  smix-sdk smix-mcp smix-adapter-maestro smix-recorder
  smix-cli
)
for c in "${CRATES[@]}"; do
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

# --- publish npm ------------------------------------------------------

log "npm publish @goliapkg/smix@$VERSION"
# v0.1.0 SDK ship cycle finding: `npm publish` crashes on nvm 26.5.0
# node ("Cannot find module npm.js"), `bun publish` works. Prefer bun.
if command -v bun >/dev/null 2>&1; then
  ( cd "$ROOT/npm/smix-rn" && bun run build && bun publish --access public ) \
    || fail "bun publish"
else
  ( cd "$ROOT/npm/smix-rn" && npm publish --access public ) || fail "npm publish"
fi

# --- publish Maven Central -------------------------------------------

log "gradle publish jp.golia.smix:smix-sdk:$VERSION"
GPG_KEY="$(gpg --export-secret-keys --armor FBD802632CFAD78B 2>/dev/null)" \
  || fail "gpg export failed for signing key FBD802632CFAD78B"
( cd "$ROOT/android-runner" && \
  ORG_GRADLE_PROJECT_signingInMemoryKey="$GPG_KEY" \
  ORG_GRADLE_PROJECT_signingInMemoryKeyId=2CFAD78B \
  ORG_GRADLE_PROJECT_signingInMemoryKeyPassword="" \
  ./gradlew :sdk:publish --console=plain ) \
  || fail "gradle publish"

# --- tag Swift Package + push ----------------------------------------

log "tag swift-v$VERSION + push"
( cd "$ROOT" && git tag -a "swift-v$VERSION" -m "Swift Package v$VERSION" && git push origin "swift-v$VERSION" ) \
  || fail "git tag + push"

log "SHIP COMPLETE — v$VERSION live on crates.io + npm + Maven Central + Swift Package"
