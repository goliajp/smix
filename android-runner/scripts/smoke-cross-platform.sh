#!/usr/bin/env bash
# Cross-platform yaml smoke (Android side).
# Mirror harness of the iOS path; runs the SAME yaml flow
# (00_cross_platform_smoke.yaml) against Android emulator via
# smix-maestro --platform android.

set -euo pipefail

AVD_NAME="${AVD_NAME:-android-emulator}"
SERIAL="${ADB_SERIAL:-emulator-5554}"
PORT=28080
YAML=examples/maestro/00_cross_platform_smoke.yaml
APPS_CONFIG=examples/smix-apps.yaml

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

APK="android-runner/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk"
if [ ! -f "$APK" ]; then
  (cd android-runner && ./gradlew :app:assembleDebugAndroidTest)
fi
if [ ! -x target/release/smix-maestro ]; then
  cargo build --release -p smix-adapter-maestro
fi

echo "=== 1. boot emulator $AVD_NAME ==="
emulator @$AVD_NAME -no-audio -no-snapshot -no-boot-anim ${EMU_VISIBLE_FLAGS:-} \
  -port 5554 -id $AVD_NAME -skin 1080x2340 \
  >/tmp/smix-android-emulator.log 2>&1 &
EMU_PID=$!

echo "=== 2. wait boot complete ==="
until adb -s $SERIAL shell getprop sys.boot_completed 2>/dev/null | grep -q 1; do sleep 5; done
echo "boot ok"

echo "=== 3. install APK ==="
adb -s $SERIAL install -r "$APK"

echo "=== 4. start instrumentation + forward ==="
adb -s $SERIAL forward tcp:$PORT tcp:$PORT
adb -s $SERIAL shell am instrument -w -e debug false \
  -e class dev.smix.runner.RunnerTest \
  dev.smix.runner.test/androidx.test.runner.AndroidJUnitRunner \
  >/tmp/smix-runner-instrument.log 2>&1 &
INSTR_PID=$!
sleep 5

curl -sf http://127.0.0.1:$PORT/health >/dev/null && echo "/health OK"

echo "=== 5. smix-maestro test --platform android ==="
target/release/smix-maestro test \
  --platform android \
  --apps-config "$APPS_CONFIG" \
  --runner-port $PORT \
  --no-launch \
  "$YAML"

echo
echo "=== 6. teardown ==="
adb -s $SERIAL shell am force-stop dev.smix.runner.test || true
adb -s $SERIAL forward --remove tcp:$PORT || true
adb -s $SERIAL emu kill || true
wait $INSTR_PID 2>/dev/null || true
wait $EMU_PID 2>/dev/null || true

echo
echo "ALL PASS (Android cross-platform yaml smoke)"
