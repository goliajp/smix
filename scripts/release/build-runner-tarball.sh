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

# The manifest that goes in describes the runner, not the workspace.
#
# `swift-bridge/Package.swift` declares the SDK, the UniFFI bindings and
# a `.binaryTarget` pointing at SmixCoreFFI.xcframework — which the
# excludes below deliberately leave out, at 49 MB against a 0.25 MB
# archive compiled into the CLI. SwiftPM resolves the whole graph before
# building, so shipping that declaration without the file stopped
# `runner up` everywhere except the machine whose earlier builds had
# left an xcframework lying in ~/.local/share/smix/runner/. CI found it
# on the first push; nothing here could, because here it was present.
#
# Staged into a copy: the workspace manifest is the source of truth and
# stays as it is.
# Staged as a whole tree, with the manifest replaced in place.
#
# The first attempt excluded ./Package.swift and appended the trimmed
# one with a second `-C`; bsdtar took the exclude and dropped the
# append, and the archive shipped with no manifest at all. One source
# directory, one pass, nothing to get wrong about how the two combine.
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
cp -R "$SRC/." "$STAGE/tree"
python3 "$REPO_ROOT/scripts/release/runner-package-manifest.py" > "$STAGE/tree/Package.swift" \
  || { echo "error: could not build the runner manifest" >&2; exit 1; }

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
  -cf - -C "$STAGE/tree" . \
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

# The CLI EMBEDS this tarball, so writing it is only half the job.
#
# `runner install` compares the installed sources against the bytes the
# CLI carries — correctly — and a CLI built before this rebuild still
# carries the old ones. It then reports "already at vX — nothing to do",
# which is true about the CLI and reads as a statement about the repo. A
# Swift change then sits in git, in the tarball, and nowhere on the
# device, while the runner keeps serving the old behaviour.
echo "smix-runner-sources: the CLI embeds this — rebuild it or the device keeps the old runner:"
echo "  cargo build --release -p smix-cli && smix runner install --force"
