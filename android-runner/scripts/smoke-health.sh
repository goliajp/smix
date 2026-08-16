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

# The emulator's start and stop live in one place for all six smoke
# scripts, and go through smix so the machine's ledger says who
# booted it and teardown stops only that.
. "$(cd "$(dirname "$0")" && pwd)/lib/emulator-lifecycle.sh"
PORT=28080

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APK="$ROOT/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk"

if [ ! -f "$APK" ]; then
  echo "→ building APK..."
  (cd "$ROOT" && ./gradlew :app:assembleDebugAndroidTest)
fi

echo "=== 1-2. emulator up (through smix; SERIAL is set by the lifecycle) ==="
smoke_emulator_up
SDK=$(adb -s $SERIAL shell getprop ro.build.version.sdk | tr -d '\r')

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
smoke_emulator_down
wait $INSTR_PID 2>/dev/null || true

echo
echo "ALL PASS (Android /health end-to-end)"
