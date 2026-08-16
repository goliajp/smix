#!/usr/bin/env bash
# Android end-to-end element-act smoke.
# Probes /tap-by-id + /double-tap-at-norm-coord + /long-press-at-norm-coord +
# /input-text endpoints.

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

curl -sf http://127.0.0.1:$PORT/health >/dev/null && echo "/health OK"

echo "=== 5. POST /tap-by-id id=workspace (launcher resource — should miss) ==="
curl -sf -X POST -H 'Content-Type: application/json' \
  -d '{"id":"workspace"}' \
  http://127.0.0.1:$PORT/tap-by-id
echo

echo "=== 6. POST /double-tap-at-norm-coord (0.5, 0.5) ==="
curl -sf -X POST -H 'Content-Type: application/json' \
  -d '{"nx":0.5,"ny":0.5}' \
  http://127.0.0.1:$PORT/double-tap-at-norm-coord
echo

echo "=== 7. POST /long-press-at-norm-coord (0.5, 0.5) durationMs=800 ==="
curl -sf -X POST -H 'Content-Type: application/json' \
  -d '{"nx":0.5,"ny":0.5,"durationMs":800}' \
  http://127.0.0.1:$PORT/long-press-at-norm-coord
echo

# back to home before input-text test
curl -sf -X POST -H 'Content-Type: application/json' -d '{}' \
  http://127.0.0.1:$PORT/back >/dev/null
curl -sf -X POST -H 'Content-Type: application/json' -d '{}' \
  http://127.0.0.1:$PORT/back >/dev/null

echo "=== 8. POST /input-text text='hello world' ==="
# Note: no focused input on home screen; just verify the endpoint doesn't
# fail. Response is {status: ok} regardless — actual character delivery
# requires a focused EditText (validated in real fixture flows).
curl -sf -X POST -H 'Content-Type: application/json' \
  -d '{"text":"hello world"}' \
  http://127.0.0.1:$PORT/input-text
echo

echo "=== 9. probe /find-text-by-ocr ==="
curl -sw "\nHTTP %{http_code}\n" -X POST http://127.0.0.1:$PORT/find-text-by-ocr -d '{}' 2>&1 | head -3

echo
echo "=== 10. teardown ==="
adb -s $SERIAL shell am force-stop dev.smix.runner.test || true
adb -s $SERIAL forward --remove tcp:$PORT || true
smoke_emulator_down
wait $INSTR_PID 2>/dev/null || true

echo
echo "ALL PASS (Android element act surface)"
