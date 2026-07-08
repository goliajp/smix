#!/usr/bin/env bash
# Android end-to-end /tree smoke.
# Boots emulator, installs runner APK, dumps UiAutomator2 a11y tree
# via Rust AndroidDriver::tree (HttpRunnerClient.get_tree), asserts
# JSON shape matches the A11yNode contract.

set -euo pipefail

AVD_NAME="${AVD_NAME:-android-emulator}"
SERIAL="${ADB_SERIAL:-emulator-5554}"
PORT=28080

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APK="$ROOT/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk"

if [ ! -f "$APK" ]; then
  (cd "$ROOT" && ./gradlew :app:assembleDebugAndroidTest)
fi

echo "=== 1. boot emulator $AVD_NAME ==="
emulator @$AVD_NAME -no-audio -no-snapshot -no-boot-anim ${EMU_VISIBLE_FLAGS:-} \
  -port 5554 -id $AVD_NAME -skin 1080x2340 \
  >/tmp/smix-android-emulator.log 2>&1 &
EMU_PID=$!

echo "=== 2. wait boot complete ==="
until adb -s $SERIAL shell getprop sys.boot_completed 2>/dev/null | grep -q 1; do sleep 5; done
echo "boot ok — API $(adb -s $SERIAL shell getprop ro.build.version.sdk | tr -d '\r')"

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

echo "=== 5. probe /health ==="
curl -sf http://127.0.0.1:$PORT/health | tee /tmp/smix-android-health.json
echo

echo "=== 6. probe /tree (UiAutomator2 dumpWindowHierarchy → A11yNode) ==="
TREE=$(curl -sf http://127.0.0.1:$PORT/tree)
echo "$TREE" >/tmp/smix-android-tree.json
echo "tree byte size: ${#TREE}"

# Assert tree shape via jq if available, otherwise grep for required fields.
if command -v jq >/dev/null; then
  echo "$TREE" | jq -r '"rawType: \(.rawType)\nchildren count: \(.children | length)\nfirst child rawType: \(.children[0].rawType // "none")\nhas bounds: \(.bounds != null)"'
  ROOT_TYPE=$(echo "$TREE" | jq -r '.rawType')
  CHILD_COUNT=$(echo "$TREE" | jq -r '.children | length')
  if [ -z "$ROOT_TYPE" ] || [ "$ROOT_TYPE" = "null" ]; then
    echo "FAIL: no rawType on root"; exit 1
  fi
  if [ "$CHILD_COUNT" -lt 1 ]; then
    echo "FAIL: tree has 0 children (expected populated launcher / SystemUI hierarchy)"; exit 1
  fi
else
  # No jq — grep
  echo "$TREE" | grep -q '"rawType":' || { echo "FAIL: no rawType"; exit 1; }
  echo "$TREE" | grep -q '"bounds":' || { echo "FAIL: no bounds"; exit 1; }
  echo "$TREE" | grep -q '"children":' || { echo "FAIL: no children"; exit 1; }
fi

echo
echo "=== 7. probe still-501 route /tap ==="
TAP=$(curl -sw "\nHTTP %{http_code}\n" http://127.0.0.1:$PORT/tap)
echo "$TAP" | head -3
echo "$TAP" | grep -q 'HTTP 501' || { echo "FAIL: /tap should be 501"; exit 1; }

echo
echo "=== 8. teardown ==="
adb -s $SERIAL shell am force-stop dev.smix.runner.test || true
adb -s $SERIAL forward --remove tcp:$PORT || true
adb -s $SERIAL emu kill || true
wait $INSTR_PID 2>/dev/null || true
wait $EMU_PID 2>/dev/null || true

echo
echo "ALL PASS (Android /tree end-to-end)"
