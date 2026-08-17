#!/usr/bin/env bash
# Check that the checked-in napi loader is what NAPI-RS generates today.
#
# `crates/smix-node/index.d.ts` and `index.js` are committed and
# auto-generated from the crate's `#[napi]` annotations, but nothing
# regenerated and compared them: they were produced once at ship time
# (ship.sh runs `napi build`), so between ships the committed copy drifts
# from the crate. Before 6.0.0, `swipeAtCoord` reached the Rust bindings
# but not `index.d.ts`, and only the dry-run caught it.
#
# This regenerates the loader and diffs. A difference means the loader and
# the crate have parted ways — the exact shape of that swipeAtCoord drift.
#
# Failing beats not knowing: if any step cannot run, this exits non-zero
# and says which. A check that generates nothing has no diff to report,
# and reporting "no diff" for that reason is how a gate certifies air.
#
# Usage:
#   scripts/dev/napi-dts-fresh.sh [--verbose]

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERBOSE=0
[[ "${1:-}" == "--verbose" ]] && VERBOSE=1

NODE_DIR="$ROOT/crates/smix-node"
DTS_CHECKED="$NODE_DIR/index.d.ts"
JS_CHECKED="$NODE_DIR/index.js"

fail() {
  echo "napi-dts-fresh: $1" >&2
  exit 1
}

command -v bunx >/dev/null 2>&1 || fail "no bunx — @napi-rs/cli is a smix-node devDependency"
[[ -f "$DTS_CHECKED" ]] || fail "no committed index.d.ts at $DTS_CHECKED"
[[ -f "$JS_CHECKED" ]]  || fail "no committed index.js at $JS_CHECKED"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# Regenerate the loader into a scratch dir. Debug build: the .d.ts is a
# pure type projection and the .js loader is mode-independent, so this is
# byte-identical to the committed (release-built) pair while compiling far
# faster. --platform matches the committed .js binding format.
if (( VERBOSE )); then
  ( cd "$NODE_DIR" && bunx napi build --platform -o "$TMP" ) || fail "napi build failed"
else
  ( cd "$NODE_DIR" && bunx napi build --platform -o "$TMP" ) >/dev/null 2>&1 || fail "napi build failed (run with --verbose)"
fi

[[ -s "$TMP/index.d.ts" ]] || fail "napi build wrote no index.d.ts — nothing to diff"
[[ -s "$TMP/index.js" ]]   || fail "napi build wrote no index.js — nothing to diff"

status=0
for f in index.d.ts index.js; do
  if ! diff -q "$NODE_DIR/$f" "$TMP/$f" >/dev/null 2>&1; then
    n="$(diff "$NODE_DIR/$f" "$TMP/$f" 2>/dev/null | grep -c '^[<>]')"
    echo "napi-dts-fresh: $f differs from what napi generates ($n lines) — regenerate: (cd crates/smix-node && napi build --platform --release)" >&2
    (( VERBOSE )) && diff "$NODE_DIR/$f" "$TMP/$f" >&2
    status=1
  fi
done

if (( status == 0 )); then
  echo "napi-dts-fresh: clean — index.d.ts and index.js are what napi generates"
fi
exit "$status"
