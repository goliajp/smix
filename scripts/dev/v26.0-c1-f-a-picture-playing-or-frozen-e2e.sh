#!/usr/bin/env bash
# v26.0-C1: assertVideoPlaying / assertVideoFrozen on both platforms.
#
# A consumer whose product is live video could only see that a stream was
# playing by looking at screenshots. On each fixture's playing screen — a
# picture redrawn every frame above a still button — on an iOS simulator
# and an Android emulator:
#   1. assertVideoPlaying on the picture passes, with a firstChange phase
#   2. assertVideoFrozen on the button passes
#   3. assertVideoFrozen on the picture fails, saying it changed
#   4. assertVideoPlaying on the button fails, saying it did not change
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port IOS_PORT
gate_free_port ANDROID_PORT
WORK="$(mktemp -d)"

log()  { printf '[v26.0-c1-f] %s\n' "$*" >&2; }
fail() { printf '[v26.0-c1-f] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v26.0-c1-f] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" SERIAL="" WE_BOOTED_IOS=0 WE_BOOTED_ANDROID=0 IOS_UP=0 ANDROID_UP=0
cleanup() {
  local said
  if [ "$IOS_UP" = 1 ] && ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" 2>&1)"; then
    log "warning: the iOS runner was not stopped: $(printf '%s' "$said" | tail -1)"
  fi
  if [ "$ANDROID_UP" = 1 ] && ! said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$ANDROID_PORT" 2>&1)"; then
    log "warning: the Android runner was not stopped: $(printf '%s' "$said" | tail -1)"
  fi
  if [ "$WE_BOOTED_IOS" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  if [ "$WE_BOOTED_ANDROID" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

# judge <platform> <verb> <id> <within> <want: pass|fail> <device> <port> [--platform android]
judge() {
  local plat="$1" verb="$2" id="$3" within="$4" want="$5" device="$6" port="$7"
  shift 7
  printf 'appId: %s\n---\n- assertVisible: { id: "playing-press" }\n- %s: { id: "%s", within: %s }\n' \
    "$APP_ID" "$verb" "$id" "$within" >"$WORK/v.yaml"
  local rc=0
  # raw run: the verdict and the words of a failure are both judged
  SMIX_RUNNER_PORT="$port" "$SMIX" run "$WORK/v.yaml" --device "$device" "$@" --format json \
    >"$WORK/v.out" 2>&1 || rc=$?
  case "$want" in
    pass)
      [ "$rc" = 0 ] || fail "$plat $verb on $id was to pass, exit $rc: $(grep -m1 'error:' "$WORK/v.out")"
      if [ "$verb" = assertVideoPlaying ]; then
        grep -q '"name":"firstChange"' "$WORK/v.out" || fail "$plat $verb passed with no firstChange phase"
      fi
      ;;
    fail)
      [ "$rc" = 3 ] || fail "$plat $verb on $id was to fail (exit 3), exit $rc"
      local says="did not change"
      [ "$verb" = assertVideoFrozen ] && says="the picture changed"
      grep -q "$says" "$WORK/v.out" || fail "$plat $verb on $id failed without saying '$says': $(grep -m1 'error:' "$WORK/v.out")"
      ;;
  esac
  log "ok    $plat $verb on $id: $want"
}

# iOS
UDID="$("$SMIX" sim resolve "$E2E_IOS" 2>/dev/null | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator registered as '$E2E_IOS'"
e2e_yield_if_held "$UDID"
bash "$ROOT/scripts/dev/build-fixture-app.sh" >"$WORK/fixture.log" 2>&1 \
  || fail "fixture build: $(tail -5 "$WORK/fixture.log")"
if [ "$(simulator_state "$UDID")" != Booted ]; then
  "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "sim boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED_IOS=1
fi
xcrun simctl install "$UDID" "$ROOT/test-fixtures/demo-app/build/SmixFixture.app" >"$WORK/install.log" 2>&1 \
  || fail "install: $(cat "$WORK/install.log")"
"$SMIX" runner up "$UDID" --runner-port "$IOS_PORT" --bundle jp.golia.smix.fixture >"$WORK/up.log" 2>&1 \
  || fail "iOS runner up: $(tail -5 "$WORK/up.log")"
IOS_UP=1
APP_ID="jp.golia.smix.fixture"
printf 'appId: %s\n---\n- launchApp:\n    stopApp: true\n    arguments: ["-playing"]\n' "$APP_ID" >"$WORK/open.yaml"
SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX_RUN" --device "$UDID" "$WORK/open.yaml" >"$WORK/open.log" 2>&1 \
  || fail "opening the iOS playing screen: $(tail -3 "$WORK/open.log")"
judge iOS assertVideoPlaying playing-picture 5s pass "$UDID" "$IOS_PORT"
judge iOS assertVideoFrozen playing-press 2s pass "$UDID" "$IOS_PORT"
judge iOS assertVideoFrozen playing-picture 2s fail "$UDID" "$IOS_PORT"
judge iOS assertVideoPlaying playing-press 2s fail "$UDID" "$IOS_PORT"

# Android
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
[ -f "$APK" ] || fail "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tr -d '[:space:]')" || true
if [ -z "$SERIAL" ]; then
  "$SMIX" sim boot "$E2E_ANDROID" >"$WORK/aboot.log" 2>&1 || fail "emulator boot: $(tail -3 "$WORK/aboot.log")"
  WE_BOOTED_ANDROID=1
  SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" | tr -d '[:space:]')"
fi
case "$SERIAL" in
  emulator-*) ;;
  *) fail "'$E2E_ANDROID' resolves to $SERIAL, which is not an emulator" ;;
esac
"$SMIX" sim install "$SERIAL" "$APK" >"$WORK/ainstall.log" 2>&1 || fail "install: $(tail -3 "$WORK/ainstall.log")"
ANDROID_SERIAL="$SERIAL" "$SMIX" runner up "$SERIAL" --platform android --runner-port "$ANDROID_PORT" \
  >"$WORK/aup.log" 2>&1 || fail "Android runner up: $(tail -5 "$WORK/aup.log")"
ANDROID_UP=1
APP_ID="dev.smix.fixture"
adb -s "$SERIAL" shell am start -W -n "$APP_ID/.PlayingActivity" >"$WORK/astart.log" 2>&1 \
  || fail "starting PlayingActivity: $(tail -3 "$WORK/astart.log")"
judge Android assertVideoPlaying playing-picture 5s pass "$SERIAL" "$ANDROID_PORT" --platform android
judge Android assertVideoFrozen playing-press 2s pass "$SERIAL" "$ANDROID_PORT" --platform android
judge Android assertVideoFrozen playing-picture 2s fail "$SERIAL" "$ANDROID_PORT" --platform android
judge Android assertVideoPlaying playing-press 2s fail "$SERIAL" "$ANDROID_PORT" --platform android
log "PASS"
