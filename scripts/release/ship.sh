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
  smix-verbs smix-metro-log smix-adb
  smix-runner-wire smix-selector-resolver smix-fixture
  smix-annotate smix-migrate smix-recorder-ir
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
