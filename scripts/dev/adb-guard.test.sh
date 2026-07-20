#!/usr/bin/env bash
#
# Exercises adb-guard against the command shapes it exists to judge.
# The physical serial R5CT52DF07D is the one attached while this gate
# was written; the emulator forms mirror the smoke scripts.

set -uo pipefail

GUARD="$(cd "$(dirname "$0")" && pwd)/adb-guard.sh"
fails=0

# feed <expected-exit> <command>
feed() {
  local want="$1"; shift
  local cmd="$1"
  local json got
  json="$(python3 -c 'import json,sys; print(json.dumps({"tool_input":{"command":sys.argv[1]}}))' "$cmd")"
  printf '%s' "$json" | bash "$GUARD" >/dev/null 2>&1
  got=$?
  if [ "$got" != "$want" ]; then
    echo "FAIL want=$want got=$got: $cmd"
    fails=$((fails + 1))
  fi
}

BLOCK=2
ALLOW=0

# --- must BLOCK: reaches a physical device ---
feed $BLOCK "adb install -r app.apk"
feed $BLOCK "adb -s R5CT52DF07D install -r app.apk"
feed $BLOCK "adb -s R5CT52DF07D shell am instrument -w dev.smix.runner.test/androidx.test.runner.AndroidJUnitRunner"
feed $BLOCK "adb uninstall dev.smix.runner.test"
feed $BLOCK "adb shell am instrument -w -e class dev.smix.runner.RunnerTest dev.smix.runner.test/androidx.test.runner.AndroidJUnitRunner"
feed $BLOCK "adb push fixture.apk /data/local/tmp/"
feed $BLOCK "./gradlew :app:installDebugAndroidTest"
feed $BLOCK "./gradlew connectedAndroidTest"
feed $BLOCK "cd android-runner && ./gradlew :app:connectedCheck"
# a physical -s even on an otherwise read-only verb is explicit intent
feed $BLOCK "adb -s R5CT52DF07D shell input tap 100 200"

# --- must ALLOW: emulator explicit, or read-only ---
feed $ALLOW "adb -s emulator-5554 install -r app.apk"
feed $ALLOW "adb -s emulator-5554 shell am instrument -w dev.smix.runner.test/androidx.test.runner.AndroidJUnitRunner"
feed $ALLOW "adb -s emulator-5554 uninstall dev.smix.runner.test"
feed $ALLOW "ANDROID_SERIAL=emulator-5554 ./gradlew :app:installDebugAndroidTest"
feed $ALLOW "ANDROID_SERIAL=emulator-5554 ./gradlew connectedAndroidTest"
feed $ALLOW "adb devices"
feed $ALLOW "adb -s emulator-5554 shell getprop sys.boot_completed"
feed $ALLOW "adb -s emulator-5554 forward tcp:28080 tcp:28080"
feed $ALLOW "adb -s emulator-5554 emu kill"
feed $ALLOW "adb logcat -d"
# unrelated commands are never touched
feed $ALLOW "cargo test -p smix-driver"
feed $ALLOW "git status"

if [ "$fails" -eq 0 ]; then
  echo "adb-guard: all cases pass"
  exit 0
fi
echo "adb-guard: $fails case(s) failed"
exit 1
