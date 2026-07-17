#!/usr/bin/env bash
# Rebuild swift-bridge/SmixCoreFFI.xcframework from crates/smix-ffi.
#
# The xcframework is what the Swift SDK links against, and it was a binary
# blob with no way to reproduce it: Package.swift named this script and it
# did not exist. So when the driving surface was added to smix-ffi, the
# bindings gained functions whose symbols the checked-in .a did not carry.
#
# Two slices, and only two: the iOS simulator and macOS, both arm64. No
# device slice, ever — smix is simulator-only (iron rule §9 #1), and a
# device binary would be the first step toward pretending otherwise.
#
# Usage: scripts/sdk/build-xcframework.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

XCF="$ROOT/swift-bridge/SmixCoreFFI.xcframework"
STAGING="$(mktemp -d)"
trap 'rm -rf "$STAGING"' EXIT

# The two slices, as (rust target, xcframework library identifier).
SLICES=(
  "aarch64-apple-ios-sim:ios-arm64-simulator"
  "aarch64-apple-darwin:macos-arm64"
)

for slice in "${SLICES[@]}"; do
  target="${slice%%:*}"
  rustup target list --installed | grep -qx "$target" \
    || { echo "build-xcframework: missing rust target $target — rustup target add $target" >&2; exit 1; }
done

echo "build-xcframework: building smix-ffi for both slices"
for slice in "${SLICES[@]}"; do
  target="${slice%%:*}"
  cargo build -p smix-ffi --release --target "$target"
done

# The C header and modulemap the Swift side imports. Generated from the
# library, so they match the symbols it exports rather than a hand-kept copy.
HOST_LIB="$ROOT/target/aarch64-apple-darwin/release/libsmix_ffi.a"
echo "build-xcframework: generating headers"
cargo run -q -p smix-ffi --features bindgen-cli --bin smix-bindgen-swift -- \
  --headers "$HOST_LIB" "$STAGING/headers"
# The Swift sources `import smixFFI`, so the module the modulemap declares
# has to be named that — the bindgen default is derived from the crate
# (`smix_ffi`) and would not resolve.
cargo run -q -p smix-ffi --features bindgen-cli --bin smix-bindgen-swift -- \
  --modulemap --module-name smixFFI "$HOST_LIB" "$STAGING/headers"
# xcodebuild wants it under the conventional filename.
if [[ -f "$STAGING/headers/smix_ffi.modulemap" ]]; then
  mv "$STAGING/headers/smix_ffi.modulemap" "$STAGING/headers/module.modulemap"
fi

# xcodebuild -create-xcframework wants each slice as (library, headers dir).
CREATE_ARGS=()
for slice in "${SLICES[@]}"; do
  target="${slice%%:*}"
  CREATE_ARGS+=(-library "$ROOT/target/$target/release/libsmix_ffi.a" -headers "$STAGING/headers")
done

echo "build-xcframework: assembling the xcframework"
rm -rf "$XCF"
xcodebuild -create-xcframework "${CREATE_ARGS[@]}" -output "$XCF"

echo "build-xcframework: done — $XCF"
