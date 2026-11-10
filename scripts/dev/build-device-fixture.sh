#!/usr/bin/env bash
# Build the Apple TV or Apple Watch fixture app into a directory.
#
# Like the phone fixture, no xcodeproj: a simulator .app is a directory
# with an Info.plist and a Mach-O swiftc builds from the one source file.
# Built where the caller says rather than kept in the tree — it takes
# seconds, and nothing else installs it.
#
#   build-device-fixture.sh tv|watch <out-dir>   → <out-dir>/SmixTVFixture.app
#                                                 or SmixWatchFixture.app
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
KIND="${1:?usage: build-device-fixture.sh tv|watch <out-dir>}"
OUT_DIR="${2:?usage: build-device-fixture.sh tv|watch <out-dir>}"

fail() { printf '[device-fixture] FAIL: %s\n' "$*" >&2; exit 1; }

case "$KIND" in
  tv)
    SRC="$ROOT/test-fixtures/tv-app/main.swift" SDK_NAME=appletvsimulator OS=tvos MIN=17.0
    EXE=SmixTVFixture BUNDLE_ID=jp.golia.smix.tvfixture FAMILY=3 LIBRARY=()
    ;;
  watch)
    SRC="$ROOT/test-fixtures/watch-app/main.swift" SDK_NAME=watchsimulator OS=watchos MIN=10.0
    EXE=SmixWatchFixture BUNDLE_ID=jp.golia.smix.watchfixture FAMILY=4 LIBRARY=(-parse-as-library)
    ;;
  *) fail "unknown kind '$KIND' — expected tv or watch" ;;
esac
SDK="$(xcrun --sdk "$SDK_NAME" --show-sdk-path 2>/dev/null)" || fail "no $SDK_NAME SDK"
case "$(uname -m)" in
  arm64) TRIPLE="arm64-apple-${OS}${MIN}-simulator" ;;
  x86_64) TRIPLE="x86_64-apple-${OS}${MIN}-simulator" ;;
  *) fail "unsupported host architecture: $(uname -m)" ;;
esac

APP="$OUT_DIR/$EXE.app"
rm -rf "$APP"
mkdir -p "$APP"
xcrun --sdk "$SDK_NAME" swiftc ${LIBRARY[@]+"${LIBRARY[@]}"} -target "$TRIPLE" -sdk "$SDK" -o "$APP/$EXE" "$SRC" \
  || fail "swiftc failed"

WATCH_KEYS=""
[ "$KIND" = watch ] && WATCH_KEYS="<key>WKApplication</key><true/><key>WKWatchOnly</key><true/>"
cat > "$APP/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>$EXE</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleName</key><string>$EXE</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>MinimumOSVersion</key><string>$MIN</string>
  <key>UIDeviceFamily</key><array><integer>$FAMILY</integer></array>
  $WATCH_KEYS
</dict>
</plist>
PLIST
file "$APP/$EXE" | grep -q 'Mach-O.*executable' || fail "$EXE is not a Mach-O executable"
printf '%s\n' "$APP"
