#!/usr/bin/env bash
# Cross-platform yaml smoke (Android side).
# Mirror harness of the iOS path; runs the SAME yaml flow
# (00_cross_platform_smoke.yaml) against Android emulator via
# smix-maestro --platform android.

set -euo pipefail

# The emulator's start and stop live in one place for all six smoke
# scripts, and go through smix so the machine's ledger says who
# booted it and teardown stops only that.
. "$(cd "$(dirname "$0")" && pwd)/lib/emulator-lifecycle.sh"
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

echo "=== 1-2. emulator up (through smix; SERIAL is set by the lifecycle) ==="
smoke_emulator_up

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
smoke_emulator_down
wait $INSTR_PID 2>/dev/null || true

echo
echo "ALL PASS (Android cross-platform yaml smoke)"
