#!/usr/bin/env bash
# Regenerate crates/smix-runner-sources/data/android-runner-sources.tar.gz
# from the current android-runner/ tree.
#
# The Android counterpart of build-runner-tarball.sh, and deliberately
# the same shape: what ships is the *project*, not the artifact. The
# instrumentation APK is 51 MB — far past what belongs inside a crate —
# while the sources that produce it are under 100 KB, and the user who
# drives an Android device already has the SDK that builds them. That is
# exactly the iOS bargain: ship the Xcode project, let the machine that
# has Xcode compile it.
#
# Only `:app` is carried. The runner is `:app`'s androidTest target and
# does not depend on `:sdk` (checked: app/build.gradle.kts names no
# project dependency), so shipping the Kotlin SDK would add 6.6 MB of
# jniLibs to every install for a module the runner never loads.
#
# `settings.gradle.kts` loses its `include(":sdk")` line on the way in:
# gradle fails outright on an include whose directory is absent — a
# shipped tree must describe itself, not the repository it was cut from.
# One line goes; the rest of the file stays, because the repository
# declarations (pluginManagement / dependencyResolutionManagement) live
# there too, and a tree that cannot say where AGP comes from cannot
# build at all.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SRC="$REPO_ROOT/android-runner"
DST_DIR="$REPO_ROOT/crates/smix-runner-sources/data"
DST_TAR="$DST_DIR/android-runner-sources.tar.gz"

[ -d "$SRC" ] || { echo "no android-runner/ at $SRC" >&2; exit 1; }
mkdir -p "$DST_DIR"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

# COPYFILE_DISABLE=1 / gzip -n mirror the iOS script: without the
# former, bsdtar packs macOS extended attributes as `._*` companions
# that ship to every consumer; without the latter the gzip header
# carries a name and mtime, so identical sources produce different
# bytes and the content digest that decides "is the installed copy
# stale" flips on every repack.
COPYFILE_DISABLE=1 tar cf - -C "$SRC" \
  --exclude='./*/build' --exclude='./build' --exclude='./.gradle' \
  --exclude='./.kotlin' --exclude='./local.properties' --exclude='./.idea' \
  --exclude='./sdk' --exclude='./scripts' \
  --exclude='./.DS_Store' --exclude='./__MACOSX' \
  . | ( cd "$STAGE" && tar xf - )

grep -v '^include("\:sdk")' "$SRC/settings.gradle.kts" > "$STAGE/settings.gradle.kts"
grep -q '^include("\:app")' "$STAGE/settings.gradle.kts" \
  || { echo "settings.gradle.kts lost :app while dropping :sdk" >&2; exit 1; }

COPYFILE_DISABLE=1 tar cf - -C "$STAGE" . | gzip -n -9 > "$DST_TAR"
shasum -a 256 "$DST_TAR" | awk '{print $1"  android-runner-sources.tar.gz"}' > "$DST_TAR.sha256"

printf 'smix-runner-sources: wrote %s (%s bytes)\n' "$DST_TAR" "$(wc -c < "$DST_TAR" | tr -d ' ')"
cat "$DST_TAR.sha256"
