#!/usr/bin/env bash
# Build the Android fixture app and print the APK path.
#
# The counterpart of build-fixture-app.sh, and it exists for the same
# reason on the other platform: the device gates drove Settings, a
# system app, and a defect that only shows on an ordinary app was
# invisible to all of them at once.
#
# Deliberately outside android-runner/ — that tree ships inside smix and
# this must not.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TREE="$ROOT/test-fixtures/android-app"
APK="$TREE/app/build/outputs/apk/debug/app-debug.apk"

fail() { printf 'build-android-fixture: %s\n' "$*" >&2; exit 1; }

[ -f "$TREE/settings.gradle.kts" ] || fail "no fixture project at $TREE"

( cd "$TREE" && ./gradlew :app:assembleDebug --console=plain ) >/dev/null \
  || fail "gradle failed — run it there to see why: (cd $TREE && ./gradlew :app:assembleDebug)"

[ -f "$APK" ] || fail "gradle reported success and produced no APK at $APK"

# The id the gates address it by has to be the id it actually declares.
# A drift here makes `am start` fail with something that names the
# activity rather than the mismatch.
DECLARED="$(grep -m1 'applicationId' "$TREE/app/build.gradle.kts" | cut -d'"' -f2)"
[ -n "$DECLARED" ] || fail "no applicationId in $TREE/app/build.gradle.kts"

printf '%s\n' "$APK"
printf 'build-android-fixture: %s (applicationId %s)\n' "$APK" "$DECLARED" >&2
