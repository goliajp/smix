#!/usr/bin/env bash
# Build the fixture app the standalone-loop e2e installs and drives.
#
# No xcodeproj. A simulator .app is a directory with an Info.plist and a
# Mach-O built against the simulator SDK, and swiftc produces that from a
# single file — which leaves nothing to maintain but the source. An
# xcodeproj here would be a second build system carried for one binary.
#
# Output: test-fixtures/demo-app/build/SmixFixture.app
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SRC="$ROOT/test-fixtures/demo-app"
OUT="$SRC/build/SmixFixture.app"
BUNDLE_ID="jp.golia.smix.fixture"
EXE="SmixFixture"

log()  { printf '[fixture] %s\n' "$*"; }
fail() { printf '[fixture] FAIL: %s\n' "$*" >&2; exit 1; }

command -v xcrun >/dev/null || fail "xcrun not found"
[ -f "$SRC/main.swift" ] || fail "missing source: $SRC/main.swift"

SDK="$(xcrun --sdk iphonesimulator --show-sdk-path 2>/dev/null)" \
  || fail "no iphonesimulator SDK — install Xcode's command-line tools"

# A fixed floor, not one derived from the SDK. Apple's version numbers are
# not a sequence to do arithmetic on — they went 18 to 26, so "two majors
# back" from the iOS 26 SDK asked clang for ios24.0, which does not exist
# and fails at link. 17.0 is old enough for any runtime worth testing on
# and new enough for the SwiftUI used here.
TARGET_OS="17.0"
SDK_VER="$(xcrun --sdk iphonesimulator --show-sdk-version 2>/dev/null || echo unknown)"

ARCH="$(uname -m)"
case "$ARCH" in
  arm64)  TRIPLE="arm64-apple-ios${TARGET_OS}-simulator" ;;
  x86_64) TRIPLE="x86_64-apple-ios${TARGET_OS}-simulator" ;;
  *) fail "unsupported host architecture: $ARCH" ;;
esac

log "sdk $SDK_VER, target $TRIPLE"
rm -rf "$OUT"
mkdir -p "$OUT"

xcrun --sdk iphonesimulator swiftc \
  -target "$TRIPLE" \
  -sdk "$SDK" \
  -o "$OUT/$EXE" \
  "$SRC/main.swift" \
  || fail "swiftc failed"

cat > "$OUT/Info.plist" <<PLIST
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
  <key>LSRequiresIPhoneOS</key><true/>
  <key>MinimumOSVersion</key><string>$TARGET_OS</string>
  <key>UIDeviceFamily</key><array><integer>1</integer></array>
  <key>UILaunchScreen</key><dict/>
</dict>
</plist>
PLIST

# The two things an install actually depends on. A bundle that is missing
# either installs and then fails to launch, which reads as a smix defect.
ACTUAL_ID="$(plutil -extract CFBundleIdentifier raw "$OUT/Info.plist")" \
  || fail "Info.plist is not readable"
[ "$ACTUAL_ID" = "$BUNDLE_ID" ] || fail "bundle id is $ACTUAL_ID, expected $BUNDLE_ID"
file "$OUT/$EXE" | grep -q 'Mach-O.*executable' \
  || fail "$EXE is not a Mach-O executable: $(file "$OUT/$EXE")"

log "built $OUT ($BUNDLE_ID)"
