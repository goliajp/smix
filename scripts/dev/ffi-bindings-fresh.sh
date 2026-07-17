#!/usr/bin/env bash
# Check that the checked-in FFI bindings are what smix-ffi generates today.
#
# The Swift and Kotlin bindings are committed, and the xcframework and .so
# beside them are binary blobs. Nothing regenerated any of it: the scripts
# Package.swift and build.gradle.kts name — one of which gradle calls an
# "idempotent reproducer" — did not exist. So the boundary was whatever it
# had been the day someone last ran a command by hand, and adding a function
# to it was not possible without first inventing the way back.
#
# This regenerates and diffs. A difference means the bindings and the crate
# have parted ways.
#
# Failing beats not knowing: if any step cannot run, this exits non-zero and
# says which one. A check that generates nothing has no diff to report, and
# reporting "no diff" for that reason is how a gate comes to certify air.
#
# Usage:
#   scripts/dev/ffi-bindings-fresh.sh [--verbose]

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERBOSE=0
[[ "${1:-}" == "--verbose" ]] && VERBOSE=1

SWIFT_CHECKED="$ROOT/swift-bridge/Sources/SmixCoreFFIBindings/Generated/smix.swift"
KOTLIN_CHECKED="$ROOT/android-runner/sdk/src/main/kotlin/uniffi/smix/smix.kt"

fail() {
  echo "ffi-bindings-fresh: $1" >&2
  exit 1
}

run() {
  if (( VERBOSE )); then
    "$@" || return 1
  else
    "$@" >/dev/null 2>&1 || return 1
  fi
}

for f in "$SWIFT_CHECKED" "$KOTLIN_CHECKED"; do
  [[ -f "$f" ]] || fail "no checked-in bindings at ${f#"$ROOT"/} — nothing to compare against"
done

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# The library the bindgen reads. Built for the host, since the bindings it
# emits are the same whatever the target triple.
run cargo build -p smix-ffi --release \
  || fail "cargo build -p smix-ffi failed — run with --verbose"

# cargo names the cdylib after the crate: libsmix_ffi.
LIB=""
for candidate in "$ROOT/target/release/libsmix_ffi.dylib" \
                 "$ROOT/target/release/libsmix_ffi.so"; do
  [[ -f "$candidate" ]] && LIB="$candidate" && break
done
[[ -n "$LIB" ]] || fail "built smix-ffi but found no cdylib in target/release — \
the crate-type or the library name moved"

run cargo run -q -p smix-ffi --features bindgen-cli --bin smix-bindgen-swift -- \
  --swift-sources "$LIB" "$TMP/swift" \
  || fail "swift bindgen failed — run with --verbose"

# Library mode, not UDL. UDL mode reads only the .udl file, so anything
# exported by proc-macro is silently absent from the Kotlin side while the
# Swift side has it — a difference no error would announce.
# The Kotlin bindings load the library by name, and library-mode bindgen
# reads that name off the filename — so the bindgen has to see the name the
# Android artifact ships under. Naming the crate `uniffi_smix` instead would
# be tidier and does not work: uniffi maps a UDL-defined interface back to
# its .udl by crate name, and renaming the lib breaks that lookup ("No path
# known to UDL files for 'smix_ffi'"). So the rename is a step, and this is
# the step, written down. It used to happen in someone's shell.
UNIFFI_LIB="$TMP/lib/libuniffi_smix.${LIB##*.}"
mkdir -p "$TMP/lib"
cp "$LIB" "$UNIFFI_LIB" || fail "could not stage the library under its uniffi name"

run cargo run -q -p smix-ffi --features bindgen-cli --bin smix-bindgen -- \
  generate --library "$UNIFFI_LIB" --language kotlin --out-dir "$TMP/kotlin" \
  || fail "kotlin bindgen failed — run with --verbose"

SWIFT_FRESH="$TMP/swift/smix.swift"
KOTLIN_FRESH="$TMP/kotlin/uniffi/smix/smix.kt"
[[ -f "$SWIFT_FRESH" ]] || fail "swift bindgen wrote no smix.swift to $TMP/swift"
[[ -f "$KOTLIN_FRESH" ]] || fail "kotlin bindgen wrote no smix.kt to $TMP/kotlin"

status=0
for pair in "swift:$SWIFT_FRESH:$SWIFT_CHECKED" "kotlin:$KOTLIN_FRESH:$KOTLIN_CHECKED"; do
  lang="${pair%%:*}"; rest="${pair#*:}"; fresh="${rest%%:*}"; checked="${rest#*:}"
  if ! diff -q "$fresh" "$checked" >/dev/null 2>&1; then
    echo "ffi-bindings-fresh: $lang bindings differ from ${checked#"$ROOT"/}"
    if (( VERBOSE )); then
      diff -u "$checked" "$fresh" | head -40
    else
      echo "    $(diff "$checked" "$fresh" | grep -c '^[<>]') line(s) differ — \
rerun with --verbose, or regenerate with scripts/sdk/build-*.sh"
    fi
    status=1
  fi
done

# The bindings can match while the xcframework does not: the Swift SDK links
# against the checked-in .a, and regenerating the text without rebuilding the
# binary leaves it calling symbols the library does not carry. So check that
# every uniffi function the bindings name is a symbol the xcframework exports.
XCF_LIB="$ROOT/swift-bridge/SmixCoreFFI.xcframework/macos-arm64/libsmix_ffi.a"
if [[ -f "$XCF_LIB" ]]; then
  missing=0
  exported="$(nm "$XCF_LIB" 2>/dev/null)"
  for fn in $(grep -oE "uniffi_smix_ffi_fn_[a-z0-9_]+" "$SWIFT_CHECKED" | sort -u); do
    grep -qF "$fn" <<<"$exported" || { echo "    symbol absent from xcframework: $fn"; missing=$((missing+1)); }
  done
  if (( missing > 0 )); then
    echo "ffi-bindings-fresh: $missing binding symbol(s) missing from the xcframework — \
rebuild it with scripts/sdk/build-xcframework.sh"
    status=1
  fi
else
  echo "ffi-bindings-fresh: no xcframework at ${XCF_LIB#"$ROOT"/} — cannot check its symbols"
  status=1
fi

if (( status == 0 )); then
  echo "ffi-bindings-fresh: clean — the bindings are what smix-ffi generates, and the xcframework carries them"
fi
exit "$status"
