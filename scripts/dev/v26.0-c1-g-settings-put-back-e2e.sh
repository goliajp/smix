#!/usr/bin/env bash
# v26.0-C1: setEnvironment changes a device's settings and puts them back.
#
# A consumer wanted each flow to name the appearance, text size, language,
# time zone and network it needs, and the device put back afterwards — also
# after a run that was killed. On an Android emulator and an iOS simulator:
#   1. Android: all five are set while the flow runs, and put back when it
#      ends in a failure
#   2. Android: a run killed after setEnvironment leaves the device changed;
#      `smix status` names the owner and the values from before, and
#      `smix cleanup --owner` puts them back
#   3. iOS: appearance and text size are set on the simulator, the language
#      and time zone reach the app's process, and the simulator is put back
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port ANDROID_PORT
gate_free_port IOS_PORT
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
OWNER="v26-c1-g-$$"
WORK="$(mktemp -d)"

log()  { printf '[v26.0-c1-g] %s\n' "$*" >&2; }
fail() { printf '[v26.0-c1-g] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v26.0-c1-g] cannot judge: %s\n' "$*" >&2; exit 2; }

SERIAL="" UDID="" WE_BOOTED_ANDROID=0 WE_BOOTED_IOS=0 ANDROID_UP=0 IOS_UP=0 RUN_PID=""
cleanup() {
  local said
  if [ -n "$RUN_PID" ]; then kill -9 "$RUN_PID" 2>/dev/null || true; fi
  # whatever this left changed is this owner's to put back
  "$SMIX" cleanup --owner "$OWNER" >/dev/null 2>&1 || true
  if [ "$ANDROID_UP" = 1 ] && ! said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$ANDROID_PORT" 2>&1)"; then
    log "warning: the Android runner was not stopped: $(printf '%s' "$said" | tail -1)"
  fi
  if [ "$IOS_UP" = 1 ] && ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" 2>&1)"; then
    log "warning: the iOS runner was not stopped: $(printf '%s' "$said" | tail -1)"
  fi
  if [ "$WE_BOOTED_ANDROID" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  if [ "$WE_BOOTED_IOS" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -f "$APK" ] || fail "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tr -d '[:space:]')" || true
if [ -z "$SERIAL" ]; then
  "$SMIX" sim boot "$E2E_ANDROID" >"$WORK/boot.log" 2>&1 || fail "emulator boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED_ANDROID=1
  SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" | tr -d '[:space:]')"
fi
case "$SERIAL" in
  emulator-*) ;;
  *) fail "'$E2E_ANDROID' resolves to $SERIAL, which is not an emulator" ;;
esac
"$SMIX" sim install "$SERIAL" "$APK" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
ANDROID_SERIAL="$SERIAL" "$SMIX" runner up "$SERIAL" --platform android --runner-port "$ANDROID_PORT" \
  >"$WORK/up.log" 2>&1 || fail "Android runner up: $(tail -5 "$WORK/up.log")"
ANDROID_UP=1

# The five settings as the device reports them, one line.
android_state() {
  printf '%s|%s|%s|%s|%s' \
    "$(adb -s "$SERIAL" shell cmd uimode night | tr -d '\r')" \
    "$(adb -s "$SERIAL" shell settings get system font_scale | tr -d '\r')" \
    "$(adb -s "$SERIAL" shell getprop persist.sys.timezone | tr -d '\r')" \
    "$(adb -s "$SERIAL" shell cmd connectivity airplane-mode | tr -d '\r')" \
    "$(adb -s "$SERIAL" shell cmd locale get-app-locales dev.smix.fixture | tr -d '\r')"
}
before="$(android_state)"
log "Android before: $before"
case "$before" in
  "Night mode: dark"*|*"|enabled|"*) cannot_judge "the emulator is already dark or offline; nothing here would change" ;;
esac

# 1. set while running, put back after a failure
cat >"$WORK/all.yaml" <<FLOW
appId: dev.smix.fixture
---
- launchApp
- setEnvironment:
    appearance: dark
    textSize: extraLarge
    locale: ja-JP
    timeZone: America/New_York
    network: offline
- extendedWaitUntil:
    visible: "words this screen does not have"
    timeout: 8000
FLOW
rc=0
# raw run: the failure is arranged; what the device holds during and after is judged
SMIX_RUNNER_PORT="$ANDROID_PORT" "$SMIX" run "$WORK/all.yaml" --device "$SERIAL" --platform android \
  --runner-port "$ANDROID_PORT" >"$WORK/all.log" 2>&1 &
RUN_PID=$!
i=0
until grep -q "setEnvironment → ok" "$WORK/all.log" 2>/dev/null; do
  i=$((i + 1))
  [ "$i" -lt 120 ] || fail "setEnvironment did not finish: $(tail -3 "$WORK/all.log")"
  sleep 0.5
done
during="$(android_state)"
wait "$RUN_PID" || rc=$?
RUN_PID=""
log "Android during: $during"
[ "$during" = "Night mode: yes|1.15|America/New_York|enabled|Locales for dev.smix.fixture for user 0 are [ja-JP]" ] \
  || fail "the settings were not all set while the flow ran: $during"
[ "$rc" = 3 ] || fail "the flow was arranged to fail (exit 3), got $rc: $(tail -3 "$WORK/all.log")"
[ "$(android_state)" = "$before" ] || fail "after a failed run the device reads $(android_state), not $before"
log "ok    Android: all five set while running, put back after the failure"

# 2. a killed run, put back by its owner's cleanup
cat >"$WORK/killed.yaml" <<FLOW
appId: dev.smix.fixture
---
- launchApp
- setEnvironment:
    appearance: dark
    textSize: extraLarge
- extendedWaitUntil:
    visible: "words this screen does not have"
    timeout: 60000
FLOW
# raw run: killed on purpose
SMIX_OWNER="$OWNER" SMIX_RUNNER_PORT="$ANDROID_PORT" "$SMIX" run "$WORK/killed.yaml" --device "$SERIAL" \
  --platform android --runner-port "$ANDROID_PORT" >"$WORK/killed.log" 2>&1 &
RUN_PID=$!
i=0
until grep -q "setEnvironment → ok" "$WORK/killed.log" 2>/dev/null; do
  i=$((i + 1))
  [ "$i" -lt 120 ] || fail "setEnvironment did not finish: $(tail -3 "$WORK/killed.log")"
  sleep 0.5
done
kill -9 "$RUN_PID"
wait "$RUN_PID" 2>/dev/null || true
RUN_PID=""
left="$(android_state)"
[ "$left" != "$before" ] || cannot_judge "the killed run left nothing changed"
"$SMIX" status >"$WORK/status.txt" 2>&1 || true
grep -q "environment changed by $OWNER: appearance (was \"no\"), textSize (was \"1.0\")" "$WORK/status.txt" \
  || fail "smix status does not say who changed what: $(grep "$SERIAL" "$WORK/status.txt")"
"$SMIX" cleanup --owner "$OWNER" >"$WORK/cleanup.log" 2>&1 || fail "cleanup --owner: $(tail -3 "$WORK/cleanup.log")"
[ "$(android_state)" = "$before" ] || fail "after cleanup --owner the device reads $(android_state), not $before"
log "ok    Android: a killed run's settings are named by smix status and put back by cleanup --owner"

# 3. iOS
UDID="$("$SMIX" sim resolve "$E2E_IOS" 2>/dev/null | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator registered as '$E2E_IOS'"
e2e_yield_if_held "$UDID"
pgrep -x Simulator >/dev/null && cannot_judge "Simulator.app is open; its appearance switch reaches every simulator"
bash "$ROOT/scripts/dev/build-fixture-app.sh" >"$WORK/fixture.log" 2>&1 \
  || fail "fixture build: $(tail -5 "$WORK/fixture.log")"
if [ "$(simulator_state "$UDID")" != Booted ]; then
  "$SMIX" sim boot "$UDID" >"$WORK/iboot.log" 2>&1 || fail "sim boot: $(tail -3 "$WORK/iboot.log")"
  WE_BOOTED_IOS=1
fi
xcrun simctl install "$UDID" "$ROOT/test-fixtures/demo-app/build/SmixFixture.app" >"$WORK/iinstall.log" 2>&1 \
  || fail "install: $(cat "$WORK/iinstall.log")"
"$SMIX" runner up "$UDID" --runner-port "$IOS_PORT" --bundle jp.golia.smix.fixture >"$WORK/iup.log" 2>&1 \
  || fail "iOS runner up: $(tail -5 "$WORK/iup.log")"
IOS_UP=1
ios_state() { printf '%s|%s' "$(xcrun simctl ui "$UDID" appearance)" "$(xcrun simctl ui "$UDID" content_size)"; }
ibefore="$(ios_state)"
cat >"$WORK/ios.yaml" <<FLOW
appId: jp.golia.smix.fixture
---
- setEnvironment:
    appearance: $([ "${ibefore%%|*}" = dark ] && echo light || echo dark)
    textSize: extraExtraLarge
    locale: ja-JP
    timeZone: America/New_York
- launchApp
- extendedWaitUntil:
    visible: "words this screen does not have"
    timeout: 8000
FLOW
# raw run: the failure is arranged; what the simulator and the app hold is judged
SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" run "$WORK/ios.yaml" --device "$UDID" >"$WORK/ios.log" 2>&1 &
RUN_PID=$!
i=0
until grep -q "launchApp → ok" "$WORK/ios.log" 2>/dev/null; do
  i=$((i + 1))
  [ "$i" -lt 240 ] || fail "the iOS flow did not launch the app: $(tail -3 "$WORK/ios.log")"
  sleep 0.5
done
iduring="$(ios_state)"
app="$(pgrep -f "Devices/$UDID/.*/SmixFixture.app/SmixFixture" | head -1 || true)"
args="$(ps -o args= -p "${app:-0}" 2>/dev/null || true)"
tz="$(ps -Eww -o command= -p "${app:-0}" 2>/dev/null | tr ' ' '\n' | grep '^TZ=' || true)"
wait "$RUN_PID" || true
RUN_PID=""
log "iOS before: $ibefore  during: $iduring"
[ "$iduring" != "$ibefore" ] && [ "${iduring#*|}" = extra-extra-large ] || fail "the simulator was not set while the flow ran: $iduring"
case "$args" in
  *"-AppleLanguages (ja-JP) -AppleLocale ja_JP"*) ;;
  *) fail "the app was not launched with the language: ${args:-no process found}" ;;
esac
[ "$tz" = "TZ=America/New_York" ] || fail "the app was not launched with the time zone: ${tz:-none}"
[ "$(ios_state)" = "$ibefore" ] || fail "after the run the simulator reads $(ios_state), not $ibefore"
log "ok    iOS: appearance and text size set and put back; the app launched with the language and time zone"
log "PASS"
