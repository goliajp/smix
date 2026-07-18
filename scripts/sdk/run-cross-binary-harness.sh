#!/usr/bin/env bash
# Cross-binary conformance harness: every fixture through every backend,
# outputs diffed byte-for-byte.
#
# Three SDK READMEs, Package.swift, the Swift and TS fixture runners,
# and a Kotlin test all named this script; it did not exist. The halves
# it orchestrates were all real — this is only the missing conductor.
#
# Backends:
#   rust  — cargo bin fixture-runner (smix-ffi resolver, the reference)
#   swift — SwiftFixtureRunner via SmixCoreFFIBindings (FFI dylib)
#   ts    — npm/smix-rn/bin/ts-fixture-runner.ts (wire round-trip; the
#           resolve itself delegates to the Rust binary by design)
#
# Each backend self-checks against fixture.expected AND the harness
# diffs the backends' stdout against each other, so "everyone equally
# wrong" cannot pass as agreement with expectations.
#
# Usage: scripts/sdk/run-cross-binary-harness.sh [fixture-id ...]

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURES_DIR="$ROOT/crates/smix-core-conformance/fixtures"
OUT_DIR="$(mktemp -d)"
trap 'rm -rf "$OUT_DIR"' EXIT

log() { printf '[harness] %s\n' "$*"; }
fail() { printf '[harness] FAIL: %s\n' "$*" >&2; exit 1; }

command -v bun >/dev/null 2>&1 || fail "bun required for the TS backend"

log "building rust fixture-runner"
( cd "$ROOT" && cargo build -q -p smix-core-conformance --bin fixture-runner ) \
  || fail "cargo build fixture-runner"
RUST_BIN="$ROOT/target/debug/fixture-runner"

log "building SwiftFixtureRunner"
( cd "$ROOT/swift-bridge" && swift build -q --product SwiftFixtureRunner ) \
  || fail "swift build SwiftFixtureRunner"
SWIFT_BIN="$ROOT/swift-bridge/.build/debug/SwiftFixtureRunner"

if [[ $# -gt 0 ]]; then
  IDS=("$@")
else
  IDS=()
  for f in "$FIXTURES_DIR"/*.json; do
    base="$(basename "$f" .json)"
    IDS+=("$base")
  done
fi

pass=0
mismatch=()
for id in "${IDS[@]}"; do
  fixture="$FIXTURES_DIR/$id.json"
  [[ -f "$fixture" ]] || fail "no fixture $fixture"

  "$RUST_BIN" rust "$id" > "$OUT_DIR/$id.rust" \
    || { mismatch+=("$id: rust backend failed its self-check"); continue; }
  "$SWIFT_BIN" "$fixture" > "$OUT_DIR/$id.swift" \
    || { mismatch+=("$id: swift backend failed its self-check"); continue; }
  ( cd "$ROOT/npm/smix-rn" && bun bin/ts-fixture-runner.ts "$fixture" ) > "$OUT_DIR/$id.ts" \
    || { mismatch+=("$id: ts backend failed its self-check"); continue; }

  if ! diff -q "$OUT_DIR/$id.rust" "$OUT_DIR/$id.swift" >/dev/null; then
    mismatch+=("$id: rust vs swift differ: $(cat "$OUT_DIR/$id.rust") vs $(cat "$OUT_DIR/$id.swift")")
    continue
  fi
  if ! diff -q "$OUT_DIR/$id.rust" "$OUT_DIR/$id.ts" >/dev/null; then
    mismatch+=("$id: rust vs ts differ: $(cat "$OUT_DIR/$id.rust") vs $(cat "$OUT_DIR/$id.ts")")
    continue
  fi
  pass=$((pass + 1))
done

if [[ ${#mismatch[@]} -gt 0 ]]; then
  log "${#mismatch[@]} fixture(s) NOT byte-identical:"
  printf '  %s\n' "${mismatch[@]}"
  exit 1
fi
log "Summary: $pass / ${#IDS[@]} fixtures byte-identical (Rust + Swift + TS)"
