#!/usr/bin/env bash
#
# Exercises adb-guard against the command shapes it exists to judge.
# The physical serial R5CT52DF07D is the one attached while this gate
# was written; the emulator forms mirror the smoke scripts.

set -uo pipefail

# The guard ships in the plugin, and the repo's own hook runs that same
# copy — one implementation, so a regression bites us before it bites
# anyone who installed it.
GUARD="$(cd "$(dirname "$0")/../.." && pwd)/plugin/scripts/adb-guard.sh"
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

# Reading a physical device changes nothing, and the header has always
# said so. Until 2026-08-12 it was refused anyway, which made a `getprop`
# come back as a policy refusal that reads like a typo — and taught the
# person who hit it that the rest of the rule was not to be believed
# either. Narrow on purpose: `shell` alone is not read-only, so only the
# forms named here get through.
feed $ALLOW "adb -s R5CT52DF07D shell getprop ro.product.cpu.abi"
feed $ALLOW "adb -s R5CT52DF07D logcat -d"
feed $ALLOW "adb -s R5CT52DF07D get-state"
# and the ones that only look read-only
feed $BLOCK "adb -s R5CT52DF07D shell pm uninstall dev.smix.fixture"
feed $BLOCK "adb -s R5CT52DF07D shell rm -rf /sdcard/x"
feed $BLOCK "adb -s R5CT52DF07D shell settings put global x 1"

# unrelated commands are never touched
feed $ALLOW "cargo test -p smix-driver"
feed $ALLOW "git status"

# --- heredoc bodies: data vs code ---
# Writing prose that mentions an install command is not running one.
# This shape is what refused an append to the decision log.
feed $ALLOW "$(printf 'cat >> .claude/docs/v2.md <<%s\nadb install -r app.apk fans out to every device\nEOF\n' "'EOF'")"
feed $ALLOW "$(printf 'tee /tmp/notes <<%s\nadb -s R5CT52DF07D install app.apk\nNOTE\n' "'NOTE'")"
# ...but a shell reading its body IS running it, so the body still counts.
feed $BLOCK "$(printf 'bash <<%s\nadb install -r app.apk\nEOF\n' "'EOF'")"
# A python heredoc's body is not judged, since 2026-08-12.
#
# This asserted BLOCK, and the reasoning was that python can shell out.
# It can — but not in a shape this guard matches: reaching a device from
# python reads `subprocess.run(["adb", "-s", serial, ...])`, which the
# pattern above never sees. So keeping the body could not catch the
# dangerous form and caught only the harmless one, and what it actually
# blocked was writing anything that described a device command. Three
# attempts to write the change that made this line moot were refused by
# this line.
feed $ALLOW "$(printf 'python3 - <<%s\n# adb install -r app.apk\nPY\n' "'PY'")"
# The line opening an inert heredoc is itself still judged.
feed $BLOCK "$(printf 'adb install -r app.apk && cat <<%s\nharmless\nEOF\n' "'EOF'")"

# --- two commands' words must not be spliced into a third ---
#
# The guard matched a pattern against the whole Bash call and then read a
# word out of the whole Bash call, and those two could land on different
# commands. Five shapes let a mutation through on 2026-08-12 and one
# refused honest work; the dangerous direction is the point.
#
# R2 is the one that matters most: an emulator pin on the first command
# answered for an install onto a phone on the second. That is the exact
# thing the header says this guard exists to stop.
feed $ALLOW "curl -s https://example.com/x.txt -o /tmp/x.txt && adb -s emulator-5554 install -r app.apk"
feed $BLOCK "$(printf 'adb -s emulator-5554 shell getprop sys.boot_completed\nadb -s R5CT52DF07D install -r app.apk')"
feed $BLOCK "$(printf 'adb -s emulator-5554 shell getprop sys.boot_completed\nadb install -r app.apk')"
feed $BLOCK "adb -s emulator-5554 shell getprop sys.boot_completed && adb install -r app.apk"
feed $BLOCK "adb -s R5CT52DF07D shell rm -rf /sdcard/x && adb devices"
# A `VAR=value cmd` prefix applies to that command only — shell says so,
# and the guard has to say the same. The pin does not reach the next line.
feed $BLOCK "$(printf 'ANDROID_SERIAL=emulator-5554 ./gradlew assembleDebug\n./gradlew connectedAndroidTest')"

# --- and what must still be refused once they are judged one by one ---
#
# Splitting is the kind of change that fixes the false refusals by
# refusing nothing at all. Each of these names the way that could happen.
feed $BLOCK "adb devices && adb -s emulator-5554 install -r app.apk && adb -s R5CT52DF07D uninstall dev.smix.fixture"
# `export` does persist across commands, unlike the prefix form above.
# Without this case the fix for R6 invents a new false refusal, and
# `scripts/dev/v2.11-c4-android-propose-e2e.sh` is written this way.
feed $ALLOW "export ANDROID_SERIAL=emulator-5554 && ./gradlew connectedAndroidTest"
# A separator inside quotes is text being typed into a field, not a
# second command. Splitting there would have the guard judge a fragment
# that never runs.
feed $ALLOW 'adb -s emulator-5554 shell input text "note; adb install app.apk"'

SMIX_WAY_CASE='adb install app.apk'
# A refusal that names no alternative gets routed around rather than
# obeyed, so the way out is part of what is under test.
refusal="$(printf '{"tool_name":"Bash","tool_input":{"command":"%s"}}' \
  "$(printf '%s' "$SMIX_WAY_CASE" | sed 's/"/\\"/g')" | bash "$GUARD" 2>&1 || true)"
case "$refusal" in
  *"smix runner up"*) ;;
  *) echo "FAIL: the refusal names no smix command; said: $refusal"; fails=$((fails + 1)) ;;
esac

if [ "$fails" -eq 0 ]; then
  echo "adb-guard: all cases pass"
  exit 0
fi
echo "adb-guard: $fails case(s) failed"
exit 1
