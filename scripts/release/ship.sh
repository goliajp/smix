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

# --- publish crates.io (DAG order) -----------------------------------

log "publish crates.io DAG at $VERSION"
CRATES=(
  smix-sim-health
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
  ( cd "$ROOT" && cargo publish -p "$c" ) || fail "cargo publish $c"
  sleep 5
done

# --- publish npm ------------------------------------------------------

log "npm publish @goliapkg/smix@$VERSION"
( cd "$ROOT/npm/smix-rn" && npm publish --access public ) || fail "npm publish"

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
