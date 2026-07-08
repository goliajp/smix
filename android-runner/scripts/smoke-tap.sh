#!/usr/bin/env bash
# Android end-to-end /tap-at-norm-coord smoke.
#
# Boots emulator, installs runner APK, POSTs to /tap-at-norm-coord with
# (nx=0.5, ny=0.5) to verify the click dispatches through UiDevice and
# the response shape is well-formed.

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

echo "=== 5. probe /health (sanity) ==="
curl -sf http://127.0.0.1:$PORT/health
echo

echo "=== 6. POST /tap-at-norm-coord (nx=0.5, ny=0.5) — center tap ==="
RESP=$(curl -sf -X POST -H 'Content-Type: application/json' \
  -d '{"nx":0.5,"ny":0.5}' \
  http://127.0.0.1:$PORT/tap-at-norm-coord)
echo "$RESP"
if command -v jq >/dev/null; then
  STATUS=$(echo "$RESP" | jq -r .status)
  W=$(echo "$RESP" | jq -r .displayWidth)
  H=$(echo "$RESP" | jq -r .displayHeight)
  X=$(echo "$RESP" | jq -r .x)
  Y=$(echo "$RESP" | jq -r .y)
  echo "  status=$STATUS  display=${W}x${H}  tapped at (${X},${Y})"
  if [ "$STATUS" != "ok" ]; then
    echo "FAIL: expected status=ok, got $STATUS"; exit 1
  fi
  [ "$W" -gt 0 ] || { echo "FAIL: bad displayWidth $W"; exit 1; }
  [ "$H" -gt 0 ] || { echo "FAIL: bad displayHeight $H"; exit 1; }
else
  echo "$RESP" | grep -q '"status":"ok"' || { echo "FAIL: status != ok"; exit 1; }
fi

echo
echo "=== 7. tap a screen corner (nx=0.05, ny=0.05) — verify other coords work ==="
curl -sf -X POST -H 'Content-Type: application/json' \
  -d '{"nx":0.05,"ny":0.05}' \
  http://127.0.0.1:$PORT/tap-at-norm-coord | tee /tmp/smix-tap-corner.json
echo

echo "=== 8. probe /fill ==="
curl -sw "\nHTTP %{http_code}\n" http://127.0.0.1:$PORT/fill 2>&1 | head -3

echo
echo "=== 9. teardown ==="
adb -s $SERIAL shell am force-stop dev.smix.runner.test || true
adb -s $SERIAL forward --remove tcp:$PORT || true
adb -s $SERIAL emu kill || true
wait $INSTR_PID 2>/dev/null || true
wait $EMU_PID 2>/dev/null || true

echo
echo "ALL PASS (Android /tap-at-norm-coord end-to-end)"
