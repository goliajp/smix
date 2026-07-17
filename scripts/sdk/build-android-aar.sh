#!/usr/bin/env bash
# Rebuild the Android SDK's JNI libraries from crates/smix-ffi.
#
# build.gradle.kts names this script an "idempotent reproducer" and it did
# not exist, so the .so files under jniLibs were binary blobs with no source
# — the Android half of the same gap the xcframework had.
#
# Two ABIs: the arm64 devices ship on and the x86_64 the emulator runs.
#
# The output is renamed to libuniffi_smix.so. cargo produces libsmix_ffi.so
# from the crate name, and uniffi's Kotlin bindings load the library as
# uniffi_smix — the rename is a real step, and it used to happen by hand in
# someone's shell, which is how the checked-in bindings came to name a
# library no build produced. It happens here now, in the open.
#
# Usage: scripts/sdk/build-android-aar.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

JNI_LIBS="$ROOT/android-runner/sdk/src/main/jniLibs"

command -v cargo-ndk >/dev/null 2>&1 \
  || { echo "build-android-aar: cargo-ndk not installed — cargo install cargo-ndk" >&2; exit 1; }

for target in aarch64-linux-android x86_64-linux-android; do
  rustup target list --installed | grep -qx "$target" \
    || { echo "build-android-aar: missing rust target $target — rustup target add $target" >&2; exit 1; }
done

echo "build-android-aar: building smix-ffi for both ABIs"
cargo ndk -t arm64-v8a -t x86_64 -o "$JNI_LIBS" build -p smix-ffi --release

# cargo emits libsmix_ffi.so; the Kotlin bindings load libuniffi_smix. Rename
# in place, so what ships is what the bindings look for.
for abi in arm64-v8a x86_64; do
  src="$JNI_LIBS/$abi/libsmix_ffi.so"
  dst="$JNI_LIBS/$abi/libuniffi_smix.so"
  if [[ -f "$src" ]]; then
    mv -f "$src" "$dst"
  elif [[ ! -f "$dst" ]]; then
    echo "build-android-aar: cargo-ndk produced no library for $abi" >&2
    exit 1
  fi
done

echo "build-android-aar: done — $JNI_LIBS/{arm64-v8a,x86_64}/libuniffi_smix.so"
