#!/usr/bin/env bash
#
# Regenerate crates/smix-runner-sources/data/swift-runner-sources.tar.gz
# from the current swift-bridge/ tree.
#
# Excludes binary artefacts (SmixCoreFFI.xcframework/, DerivedData,
# xcuserdata, .swiftpm), build metadata (.bak-*), and anything the
# consumer must not receive verbatim (nothing today, but reserved).
#
# The tarball ships as the source of truth for `smix runner up` to
# extract into ~/.local/share/smix/runner/ on version mismatch, so the
# contents MUST reproduce a working xcodebuild target on a machine
# with just the CLI + Xcode installed.
#
# Called by hand when swift-bridge/ changes. Whether the checked-in
# tarball is current is enforced by
# crates/smix-runner-sources/tests/tarball_is_current.rs, which runs in
# `cargo test --workspace` and names the drifted files. This header
# previously claimed a ship gate compared its SHA256; no such gate
# existed, and three Swift files reached a release branch without ever
# entering the tarball a consumer builds.
#
# Reproducibility: gzip -n strips the mtime header so the tarball is
# byte-identical when the input files are byte-identical.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

SRC="$REPO_ROOT/swift-bridge"
DST_DIR="$REPO_ROOT/crates/smix-runner-sources/data"
DST_TAR="$DST_DIR/swift-runner-sources.tar.gz"
DST_SHA="$DST_TAR.sha256"

if [[ ! -d "$SRC" ]]; then
  echo "error: $SRC does not exist" >&2
  exit 1
fi

mkdir -p "$DST_DIR"

# tar with:
#   -C swift-bridge/  : cd into swift-bridge so paths in the tarball
#                       are relative (SmixRunner.xcodeproj/... etc.,
#                       not swift-bridge/SmixRunner.xcodeproj/...) —
#                       matches what extract_to writes on the consumer.
#   --exclude         : binary xcframework, IDE cache, per-user state,
#                       previous backups. Must be BEFORE the .
#   .                 : entire cwd (after cd), respecting excludes.
#
# gzip -n : no filename / no mtime → reproducible byte output.
#
# The tar options work on both macOS (bsdtar) and GNU tar with the
# same semantics for --exclude, -C, and .-final.

COPYFILE_DISABLE=1 tar \
  --exclude='./SmixCoreFFI.xcframework' \
  --exclude='./SmixCoreFFI.xcframework.zip' \
  --exclude='./SmixCoreFFI.xcframework.zip.sha256' \
  --exclude='./.swiftpm' \
  --exclude='./DerivedData' \
  --exclude='./*.xcodeproj/xcuserdata' \
  --exclude='./*.xcodeproj/project.xcworkspace/xcuserdata' \
  --exclude='./.bak-*' \
  --exclude='./.build' \
  --exclude='./.DS_Store' \
  --exclude='./__MACOSX' \
  -cf - -C "$SRC" . \
  | gzip -n -9 > "$DST_TAR"

# SHA256 sidecar — matches the format we use elsewhere in the repo.
if command -v shasum >/dev/null 2>&1; then
  ( cd "$DST_DIR" && shasum -a 256 "$(basename "$DST_TAR")" > "$DST_SHA" )
elif command -v sha256sum >/dev/null 2>&1; then
  ( cd "$DST_DIR" && sha256sum "$(basename "$DST_TAR")" > "$DST_SHA" )
else
  echo "warning: no shasum/sha256sum available, skipping checksum" >&2
fi

SIZE=$(wc -c < "$DST_TAR" | tr -d ' ')
echo "smix-runner-sources: wrote $DST_TAR ($SIZE bytes)"
if [[ -f "$DST_SHA" ]]; then
  cat "$DST_SHA"
fi
