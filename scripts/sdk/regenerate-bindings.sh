#!/usr/bin/env bash
# Regenerate the FFI bindings and rebuild the artifacts that carry them.
#
# Three steps have to happen together, and doing them one at a time by hand is
# how the bindings came to name symbols the binary did not have. This is the
# whole cycle: regenerate the Swift and Kotlin bindings, rebuild the
# xcframework, rebuild the Android .so. After it, scripts/dev/ffi-bindings-fresh.sh
# is green.
#
# Run this whenever smix-ffi's exported surface changes.
#
# Usage: scripts/sdk/regenerate-bindings.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

SWIFT_OUT="$ROOT/swift-bridge/Sources/SmixCoreFFIBindings/Generated"
KOTLIN_OUT="$ROOT/android-runner/sdk/src/main/kotlin/uniffi/smix"

echo "regenerate-bindings: building smix-ffi (host)"
cargo build -p smix-ffi --release

LIB="$ROOT/target/release/libsmix_ffi.dylib"
[[ -f "$LIB" ]] || { echo "regenerate-bindings: no $LIB after build" >&2; exit 1; }

STAGING="$(mktemp -d)"
trap 'rm -rf "$STAGING"' EXIT

echo "regenerate-bindings: swift bindings"
cargo run -q -p smix-ffi --features bindgen-cli --bin smix-bindgen-swift -- \
  --swift-sources "$LIB" "$STAGING/swift"
cp "$STAGING/swift/smix.swift" "$SWIFT_OUT/smix.swift"

echo "regenerate-bindings: kotlin bindings (library mode)"
# Under the name the Kotlin bindings load — see build-android-aar.sh.
mkdir -p "$STAGING/lib"
cp "$LIB" "$STAGING/lib/libuniffi_smix.dylib"
cargo run -q -p smix-ffi --features bindgen-cli --bin smix-bindgen -- \
  generate --library "$STAGING/lib/libuniffi_smix.dylib" --language kotlin --out-dir "$STAGING/kotlin"
cp "$STAGING/kotlin/uniffi/smix/smix.kt" "$KOTLIN_OUT/smix.kt"

echo "regenerate-bindings: rebuilding the xcframework"
"$ROOT/scripts/sdk/build-xcframework.sh"

echo "regenerate-bindings: rebuilding the Android libraries"
"$ROOT/scripts/sdk/build-android-aar.sh"

echo "regenerate-bindings: done — checking"
"$ROOT/scripts/dev/ffi-bindings-fresh.sh"
