#!/usr/bin/env bash
# v23.0-C2: a step that needs the app's elements, run while another app is
# in front, fails at once and says so.
#
# On iOS the same tap used to wait while XCUITest looked for the app's
# window — 31 s an attempt, three attempts — and then fail as a driver
# error; on Android it timed out as "element not found". Now:
#   1. iOS: the fixture behind Settings, a tapOn on one of its ids fails
#      with APP_NOT_IN_FOREGROUND inside 5 s, naming Settings in front
#   2. iOS: the same tap after launchApp passes — the check does not
#      refuse an app that is in front
#   3. Android: the same, with the Android fixture behind Settings
#   4. Android: the same tap after launchApp passes
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port IOS_PORT
gate_free_port AND_PORT
IOS_APP="jp.golia.smix.fixture"
AND_APP="dev.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"
# well under the 30 s a single XCUITest attempt took, over the 3 s a poll
# keeps looking for an app a notification is bringing back
WITHIN_MS=6000

log()  { printf '[v23.0-c2-a] %s\n' "$*" >&2; }
fail() { printf '[v23.0-c2-a] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v23.0-c2-a] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" SERIAL="" IOS_UP=0 AND_UP=0 WE_BOOTED_IOS=0 WE_BOOTED_AND=0
cleanup() {
  local said
  if [ "$IOS_UP" = 1 ] && ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" 2>&1)"; then
    log "warning: the iOS runner on $IOS_PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  if [ "$AND_UP" = 1 ] \
    && ! said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$AND_PORT" 2>&1)"; then
    log "warning: the Android runner on $AND_PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  if [ "$WE_BOOTED_IOS" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  if [ "$WE_BOOTED_AND" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

# $1 log, $2 the app said to be in front: the step failed with the code,
# named what is in front, and took less than WITHIN_MS
judge_refusal() {
  local out="$1" front="$2" ms
  grep -q "APP_NOT_IN_FOREGROUND" "$out" || fail "$out: not APP_NOT_IN_FOREGROUND: $(grep -m1 FAIL "$out")"
  grep -q "$front is in front" "$out" || fail "$out: does not name $front in front: $(grep -m1 FAIL "$out")"
  ms="$(grep -m1 -oE 'tapOn → FAILED \([0-9]+ ms\)' "$out" | grep -oE '[0-9]+' || true)"
  [ -n "$ms" ] || fail "$out: no step time"
  [ "$ms" -lt "$WITHIN_MS" ] || fail "$out: the refusal took $ms ms"
  log "   refused in $ms ms"
}

# --- iOS
UDID="$("$SMIX" sim resolve "$E2E_IOS" 2>/dev/null | tail -1 | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$E2E_IOS'"
case "$(simulator_name "$UDID")" in
  sim-smix-*) ;;
  *) cannot_judge "'$E2E_IOS' resolves to $UDID, which is not one of smix's own simulators" ;;
esac
if [ "$(simulator_state "$UDID")" != Booted ]; then
  with_deadline 300 "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED_IOS=1
fi
"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
"$SMIX" runner up "$UDID" --bundle "$IOS_APP" --runner-port "$IOS_PORT" >"$WORK/up.log" 2>&1 \
  || fail "iOS runner up: $(tail -5 "$WORK/up.log")"
IOS_UP=1

printf 'appId: %s\n---\n- tapOn: { id: fixture-detail-link }\n' "$IOS_APP" >"$WORK/ios-tap.yaml"
printf 'appId: %s\n---\n- launchApp\n- tapOn: { id: fixture-detail-link }\n' "$IOS_APP" >"$WORK/ios-back.yaml"

log "1. iOS: a tap on the fixture behind Settings is refused at once"
"$SMIX" sim launch "$UDID" "$IOS_APP" >/dev/null 2>&1 || fail "could not launch the fixture"
"$SMIX" sim launch "$UDID" com.apple.Preferences >/dev/null 2>&1 || fail "could not launch Settings"
sleep 2
# raw run: this run's failure is what is judged
if with_deadline 120 "$SMIX" run --device "$UDID" --runner-port "$IOS_PORT" --no-launch "$WORK/ios-tap.yaml" \
  >"$WORK/ios-tap.log" 2>&1; then
  fail "1: a tap on an app behind Settings passed"
fi
judge_refusal "$WORK/ios-tap.log" com.apple.Preferences

log "2. iOS: after launchApp the same tap passes"
"$SMIX_RUN" --device "$UDID" --runner-port "$IOS_PORT" "$WORK/ios-back.yaml" >"$WORK/ios-back.log" 2>&1 \
  || fail "2: $(grep -m1 FAIL "$WORK/ios-back.log")"

# --- Android
command -v adb >/dev/null 2>&1 || cannot_judge "no adb"
if ! SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tail -1)" || [ -z "$SERIAL" ]; then
  with_deadline 300 "$SMIX" sim boot "$E2E_ANDROID" --no-window >"$WORK/aboot.log" 2>&1 \
    || fail "boot: $(tail -3 "$WORK/aboot.log")"
  SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tail -1)" || fail "$E2E_ANDROID does not resolve"
  WE_BOOTED_AND=1
fi
case "$SERIAL" in emulator-*) : ;; *) fail "$E2E_ANDROID resolves to '$SERIAL' — refusing" ;; esac
for _ in $(seq 1 60); do
  adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done
adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"
"$SMIX" runner up "$SERIAL" --platform android --bundle "$AND_APP" --runner-port "$AND_PORT" >"$WORK/aup.log" 2>&1 \
  || fail "Android runner up: $(tail -5 "$WORK/aup.log")"
AND_UP=1

printf 'appId: %s\n---\n- tapOn: { id: fixture_submit }\n' "$AND_APP" >"$WORK/and-tap.yaml"
printf 'appId: %s\n---\n- launchApp\n- tapOn: { id: fixture_submit }\n' "$AND_APP" >"$WORK/and-back.yaml"

log "3. Android: a tap on the fixture behind Settings is refused at once"
"$SMIX" sim launch "$SERIAL" com.android.settings >/dev/null 2>&1 || fail "could not launch Settings"
sleep 2
# raw run: this run's failure is what is judged
if with_deadline 120 "$SMIX" run --device "$SERIAL" --runner-port "$AND_PORT" --no-launch "$WORK/and-tap.yaml" \
  >"$WORK/and-tap.log" 2>&1; then
  fail "3: a tap on an app behind Settings passed"
fi
judge_refusal "$WORK/and-tap.log" com.android.settings

log "4. Android: after launchApp the same tap passes"
"$SMIX_RUN" --device "$SERIAL" --runner-port "$AND_PORT" "$WORK/and-back.yaml" >"$WORK/and-back.log" 2>&1 \
  || fail "4: $(grep -m1 FAIL "$WORK/and-back.log")"

log "V23.0-C2-A-AN-APP-NOT-IN-FRONT-PASS"
