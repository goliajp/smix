#!/usr/bin/env bash
# Android end-to-end /tap-at-norm-coord smoke.
#
# Boots emulator, installs runner APK, POSTs to /tap-at-norm-coord with
# (nx=0.5, ny=0.5) to verify the click dispatches through UiDevice and
# the response shape is well-formed.

set -euo pipefail

# The emulator's start and stop live in one place for all six smoke
# scripts, and go through smix so the machine's ledger says who
# booted it and teardown stops only that.
. "$(cd "$(dirname "$0")" && pwd)/lib/emulator-lifecycle.sh"
PORT=28080

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APK="$ROOT/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk"

if [ ! -f "$APK" ]; then
  (cd "$ROOT" && ./gradlew :app:assembleDebugAndroidTest)
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
smoke_emulator_down
wait $INSTR_PID 2>/dev/null || true

echo
echo "ALL PASS (Android /tap-at-norm-coord end-to-end)"
