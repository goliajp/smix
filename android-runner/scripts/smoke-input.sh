#!/usr/bin/env bash
# Android end-to-end input/gesture smoke.
# Probes /press-key + /swipe-once + /swipe-at-norm-coord + /back +
# /hide-keyboard + /set-orientation.

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

echo "=== 5. POST /press-key key=return ==="
curl -sf -X POST -H 'Content-Type: application/json' \
  -d '{"key":"return"}' \
  http://127.0.0.1:$PORT/press-key
echo

echo "=== 6. POST /press-key key=arrowDown ==="
curl -sf -X POST -H 'Content-Type: application/json' \
  -d '{"key":"arrowDown"}' \
  http://127.0.0.1:$PORT/press-key
echo

echo "=== 7. POST /swipe-once direction=up ==="
curl -sf -X POST -H 'Content-Type: application/json' \
  -d '{"direction":"up"}' \
  http://127.0.0.1:$PORT/swipe-once
echo

echo "=== 8. POST /swipe-once direction=down ==="
curl -sf -X POST -H 'Content-Type: application/json' \
  -d '{"direction":"down"}' \
  http://127.0.0.1:$PORT/swipe-once
echo

echo "=== 9. POST /swipe-at-norm-coord (0.2,0.5)→(0.8,0.5) ==="
curl -sf -X POST -H 'Content-Type: application/json' \
  -d '{"fromNx":0.2,"fromNy":0.5,"toNx":0.8,"toNy":0.5}' \
  http://127.0.0.1:$PORT/swipe-at-norm-coord
echo

echo "=== 10. POST /back ==="
curl -sf -X POST -H 'Content-Type: application/json' -d '{}' \
  http://127.0.0.1:$PORT/back
echo

echo "=== 11. POST /hide-keyboard ==="
curl -sf -X POST -H 'Content-Type: application/json' -d '{}' \
  http://127.0.0.1:$PORT/hide-keyboard
echo

echo "=== 12. POST /set-orientation orientation=landscapeLeft ==="
curl -sf -X POST -H 'Content-Type: application/json' \
  -d '{"orientation":"landscapeLeft"}' \
  http://127.0.0.1:$PORT/set-orientation
echo

echo "=== 13. POST /set-orientation orientation=portrait ==="
curl -sf -X POST -H 'Content-Type: application/json' \
  -d '{"orientation":"portrait"}' \
  http://127.0.0.1:$PORT/set-orientation
echo

echo "=== 14. probe /fill ==="
curl -sw "\nHTTP %{http_code}\n" -X POST http://127.0.0.1:$PORT/fill -d '{}' 2>&1 | head -3

echo
echo "=== 15. teardown ==="
adb -s $SERIAL shell am force-stop dev.smix.runner.test || true
adb -s $SERIAL forward --remove tcp:$PORT || true
smoke_emulator_down
wait $INSTR_PID 2>/dev/null || true

echo
echo "ALL PASS (Android input/gesture endpoints)"
