#!/usr/bin/env bash
# Android end-to-end /health smoke.
#
# Boots the emulator (pin-named via the AVD_NAME env var), installs the
# runner APK, starts instrumentation, curls /health, and tears down.
#
# Prereqs (one-time):
#   brew install --cask android-commandlinetools
#   sdkmanager --install "system-images;android-33;default;arm64-v8a" "platforms;android-33"
#   # Create an AVD at ~/.android/avd/<AVD_NAME>.{ini,avd/}
#
# Usage:
#   AVD_NAME=my-avd ./android-runner/scripts/smoke-health.sh

set -euo pipefail

AVD_NAME="${AVD_NAME:-android-emulator}"
SERIAL="${ADB_SERIAL:-emulator-5554}"
PORT=28080

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APK="$ROOT/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk"

if [ ! -f "$APK" ]; then
  echo "→ building APK..."
  (cd "$ROOT" && ./gradlew :app:assembleDebugAndroidTest)
fi

echo "=== 1. boot emulator $AVD_NAME ==="
emulator @$AVD_NAME -no-audio -no-snapshot -no-boot-anim ${EMU_VISIBLE_FLAGS:-} \
  -port 5554 -id $AVD_NAME -skin 1080x2340 \
  >/tmp/smix-android-emulator.log 2>&1 &
EMU_PID=$!
echo "emulator pid=$EMU_PID"

echo "=== 2. wait boot complete ==="
until adb -s $SERIAL shell getprop sys.boot_completed 2>/dev/null | grep -q 1; do sleep 5; done
SDK=$(adb -s $SERIAL shell getprop ro.build.version.sdk | tr -d '\r')
echo "boot ok — API $SDK"

echo "=== 3. install APK ==="
adb -s $SERIAL install -r "$APK"

echo "=== 4. start instrumentation + forward port ==="
adb -s $SERIAL forward tcp:$PORT tcp:$PORT
adb -s $SERIAL shell am instrument -w -e debug false \
  -e class dev.smix.runner.RunnerTest \
  dev.smix.runner.test/androidx.test.runner.AndroidJUnitRunner \
  >/tmp/smix-runner-instrument.log 2>&1 &
INSTR_PID=$!

sleep 5

echo "=== 5. curl /health ==="
HEALTH=$(curl -sw "\nHTTP %{http_code}\n" http://127.0.0.1:$PORT/health)
echo "$HEALTH"
if ! echo "$HEALTH" | grep -q '"status":"ok"'; then
  echo "FAIL: /health did not return ok"
  exit 1
fi
if ! echo "$HEALTH" | grep -q 'HTTP 200'; then
  echo "FAIL: /health did not return 200"
  exit 1
fi

echo "=== 6. probe 501 route /tree ==="
TREE=$(curl -sw "\nHTTP %{http_code}\n" http://127.0.0.1:$PORT/tree)
echo "$TREE"
if ! echo "$TREE" | grep -q 'HTTP 501'; then
  echo "FAIL: /tree did not return 501"
  exit 1
fi

echo "=== 7. teardown ==="
adb -s $SERIAL shell am force-stop dev.smix.runner.test || true
adb -s $SERIAL forward --remove tcp:$PORT || true
adb -s $SERIAL emu kill || true
wait $INSTR_PID 2>/dev/null || true
wait $EMU_PID 2>/dev/null || true

echo
echo "ALL PASS (Android /health end-to-end)"
