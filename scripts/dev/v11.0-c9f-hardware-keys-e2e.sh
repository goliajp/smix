#!/usr/bin/env bash
# `pressKey: lock / volumeUp / volumeDown` — pressed where the device has
# the button, refused by name where it does not.
#
# The runtime skipped all three on every platform, with a reason that is
# true of the iOS simulator only (Z1). An Android runner maps lock to
# KEYCODE_POWER and both volume keys to theirs, and was never asked; a
# flow on a simulator passed with a step that did nothing.
#
# Android (own AVD, the fixture app), judged by reading the device, not
# by smix's own answer:
#   * two volumeUp presses reach AudioService as two ADJUST_RAISE events
#     in `dumpsys audio`'s volume log, two volumeDown as two ADJUST_LOWER.
#     The level itself is reported, not judged: the first press of a burst
#     only raises the volume panel on this image (measured 2026-09-25, API
#     33), so "the level moved" depends on timing between steps and the
#     event count does not.
#   * lock turns the display off: `dumpsys power` Asleep and `dumpsys
#     display` mScreenState=OFF.
#   * afterwards every volume stream it read is back at its value, and the
#     device is awake with no keyguard — checked, not assumed.
#
# iOS (sim-smix-02, the fixture app, this tree's runner): each of the
# three keys fails its flow with the runner's `no_such_button` and a
# reason naming the device; none passes.
#
# Exit: 0 judged and passed, 1 judged and failed, 2 could not judge.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
AND_PORT="$SMIX_RUNNER_PORT"
gate_free_port IOS_PORT
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/e2e-devices.sh"
AND_ALIAS="${SMIX_C9F_ANDROID:-$E2E_ANDROID}"
IOS_ALIAS="${SMIX_C9F_IOS:-$E2E_IOS}"
AND_APPID="dev.smix.fixture"
IOS_APPID="jp.golia.smix.fixture"
AND_APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
IOS_FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
IOS_PROJECT="$ROOT/swift-bridge/SmixRunner.xcodeproj"
WORK="$(mktemp -d)"
STREAMS="2 3 5"

log()  { printf '[c9f-keys] %s\n' "$*" >&2; }
cannot_judge() { printf '[c9f-keys] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }
FAILED=0
fail() { printf '[c9f-keys] FAIL: %s\n' "$*" >&2; FAILED=1; }

SERIAL="" IOS_UDID=""
AND_UPPED=0 AND_WE_BOOTED=0 IOS_UPPED=0 IOS_WE_BOOTED=0
declare -a VOL_BEFORE=()

volume_of() { # $1 stream
  adb -s "$SERIAL" shell cmd media_session volume --stream "$1" --get 2>/dev/null \
    | sed -n 's/.*volume is \([0-9][0-9]*\) in range.*/\1/p'
}

# How many times AudioService has been asked to move a volume in one
# direction since the device's own clock read $2. `dumpsys audio` keeps the
# last forty volume commands it received — a ring, so a total counted
# before and after stops moving once it is full: on 2026-09-25 two presses
# that took the music level from 5 to 3 counted as "8 → 8". Counting only
# entries stamped at or after the moment before the presses does not
# depend on how full the ring is.
# One quoted string: `adb shell` joins its arguments with spaces and the
# device's shell splits them again, so an escaped space reached `date` as
# two arguments and the clock read "09-25" — which every entry of the day
# sorts after (2026-09-25). The shape is checked where it is used.
device_clock() { adb -s "$SERIAL" shell "date '+%m-%d %H:%M:%S'" 2>/dev/null | tr -d '\r'; }
adjust_since() { # $1 ADJUST_RAISE | ADJUST_LOWER  $2 MM-DD HH:MM:SS
  adb -s "$SERIAL" shell dumpsys audio 2>/dev/null \
    | awk -v dir="dir:$1" -v since="$2" \
        'index($0, "adjustSuggestedStreamVolume(") && index($0, dir) && substr($0, 1, 14) >= since { n++ } END { print n + 0 }'
}

screen_state() {
  local wake screen
  wake="$(adb -s "$SERIAL" shell dumpsys power 2>/dev/null | sed -n 's/^ *mWakefulness=\(.*\)$/\1/p' | tr -d '\r')"
  screen="$(adb -s "$SERIAL" shell dumpsys display 2>/dev/null | sed -n 's/^ *mScreenState=\(.*\)$/\1/p' | head -1 | tr -d '\r')"
  printf '%s/%s\n' "$wake" "$screen"
}

keyguard_showing() {
  adb -s "$SERIAL" shell dumpsys window 2>/dev/null | grep -q 'isKeyguardShowing=true'
}

# Wake it, dismiss the keyguard, and put back every stream it read.
restore_android() {
  [ -n "$SERIAL" ] || return 0
  adb -s "$SERIAL" shell input keyevent KEYCODE_WAKEUP >/dev/null 2>&1 || true
  adb -s "$SERIAL" shell wm dismiss-keyguard >/dev/null 2>&1 || true
  local i=0 st
  for st in $STREAMS; do
    if [ -n "${VOL_BEFORE[$i]:-}" ]; then
      adb -s "$SERIAL" shell cmd media_session volume --stream "$st" --set "${VOL_BEFORE[$i]}" >/dev/null 2>&1 || true
    fi
    i=$((i + 1))
  done
}

cleanup() {
  local said
  restore_android
  if [ "$AND_UPPED" = 1 ]; then
    said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$AND_PORT" 2>&1)" \
      || printf '[c9f-keys] warning: the Android runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
  fi
  if [ "$AND_WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$AND_ALIAS" >/dev/null 2>&1 || true; fi
  if [ "$IOS_UPPED" = 1 ]; then
    said="$("$SMIX" runner down --device "$IOS_UDID" --runner-port "$IOS_PORT" 2>&1)" \
      || printf '[c9f-keys] warning: the iOS runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
  fi
  if [ "$IOS_WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$IOS_UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

flow() { # $1 file  $2 appId  $3.. keys
  local file="$1" app="$2"
  shift 2
  {
    printf 'appId: %s\n---\n- launchApp\n' "$app"
    for k in "$@"; do printf -- '- pressKey: %s\n' "$k"; done
  } >"$file"
}

# ---- Android ---------------------------------------------------------
command -v adb >/dev/null 2>&1 || cannot_judge "no adb on PATH"
[ -f "$AND_APK" ] || cannot_judge "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || { printf '[c9f-keys] FAIL: the fixture apk on disk is not the one this tree builds\n' >&2; exit 1; }

if ! SERIAL="$("$SMIX" sim resolve "$AND_ALIAS" 2>/dev/null | tail -1)" \
   || [ -z "$SERIAL" ] || ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  log "booting $AND_ALIAS"
  "$SMIX" sim boot "$AND_ALIAS" >/dev/null 2>&1 || cannot_judge "could not boot $AND_ALIAS"
  AND_WE_BOOTED=1
  SERIAL="$("$SMIX" sim resolve "$AND_ALIAS" 2>/dev/null | tail -1)"
fi
case "$SERIAL" in
  emulator-*) : ;;
  *) cannot_judge "$AND_ALIAS resolves to '$SERIAL', which is not an emulator — refusing" ;;
esac
adb -s "$SERIAL" wait-for-device
for _ in $(seq 1 60); do
  adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done
for st in $STREAMS; do VOL_BEFORE+=("$(volume_of "$st")"); done
for v in "${VOL_BEFORE[@]}"; do
  [ -n "$v" ] || cannot_judge "could not read the volume streams ($STREAMS) on $SERIAL"
done
log "android $SERIAL volumes before (streams $STREAMS): ${VOL_BEFORE[*]}"
[ "$(screen_state)" = "Awake/ON" ] || cannot_judge "the display is not on before the lock step: $(screen_state)"

adb -s "$SERIAL" install -r -g "$AND_APK" >/dev/null 2>&1 || cannot_judge "could not install the fixture on $SERIAL"
"$SMIX" runner up "$SERIAL" --platform android --runner-port "$AND_PORT" >"$WORK/and-up.log" 2>&1 \
  || cannot_judge "the runner would not start on $SERIAL:$AND_PORT: $(tail -3 "$WORK/and-up.log" | tr '\n' ' ')"
AND_UPPED=1

press_and_count() { # $1 key  $2 ADJUST_*
  local since seen
  # A second's gap: presses from a previous step in the same second
  # would otherwise count too.
  sleep 1
  since="$(device_clock)"
  printf '%s' "$since" | grep -Eq '^[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2}$' \
    || cannot_judge "read $SERIAL's clock as '$since', not MM-DD HH:MM:SS — nothing to count from"
  flow "$WORK/$1.yaml" "$AND_APPID" "$1" "$1"
  SMIX_RUNNER_PORT="$AND_PORT" "$SMIX_RUN" --device "$SERIAL" "$WORK/$1.yaml" >"$WORK/$1.log" 2>&1 \
    || { tail -8 "$WORK/$1.log" >&2; fail "android: the flow pressing $1 twice did not pass"; return 0; }
  seen="$(adjust_since "$2" "$since")"
  log "android $1: $seen $2 event(s) since $since; music level $(volume_of 3)"
  [ "$seen" -eq 2 ] \
    || fail "android: two $1 presses reached AudioService as $seen $2 events, not 2"
}
press_and_count volumeUp ADJUST_RAISE
press_and_count volumeDown ADJUST_LOWER

flow "$WORK/lock.yaml" "$AND_APPID" lock
if SMIX_RUNNER_PORT="$AND_PORT" "$SMIX_RUN" --device "$SERIAL" "$WORK/lock.yaml" >"$WORK/lock.log" 2>&1; then
  state=""
  for _ in $(seq 1 10); do
    state="$(screen_state)"
    [ "$state" = "Asleep/OFF" ] && break
    sleep 0.5
  done
  log "android lock: display $state"
  [ "$state" = "Asleep/OFF" ] || fail "android: after pressKey lock the display is $state, not Asleep/OFF"
else
  tail -8 "$WORK/lock.log" >&2
  fail "android: the flow pressing lock did not pass"
fi

restore_android
state=""
for _ in $(seq 1 10); do
  state="$(screen_state)"
  [ "$state" = "Awake/ON" ] && ! keyguard_showing && break
  sleep 0.5
done
[ "$state" = "Awake/ON" ] && ! keyguard_showing \
  || fail "android: could not put the device back awake and unlocked ($state)"
i=0
for st in $STREAMS; do
  now="$(volume_of "$st")"
  [ "$now" = "${VOL_BEFORE[$i]}" ] || fail "android: stream $st is $now after restoring, was ${VOL_BEFORE[$i]}"
  i=$((i + 1))
done
log "android restored: display $state, volumes $(for st in $STREAMS; do printf '%s ' "$(volume_of "$st")"; done)"

# ---- iOS -------------------------------------------------------------
command -v xcrun >/dev/null 2>&1 || cannot_judge "no xcrun — the iOS leg needs a Mac with Xcode"
IOS_UDID="$("$SMIX" sim resolve "$IOS_ALIAS" 2>/dev/null | tail -1 | tr -d '[:space:]')" || true
[ -n "$IOS_UDID" ] || cannot_judge "no simulator resolves '$IOS_ALIAS'"
[ -d "$IOS_FIXTURE" ] || cannot_judge "no iOS fixture — run: bash scripts/dev/build-fixture-app.sh"
if [ "$(simulator_state "$IOS_UDID")" != Booted ]; then
  "$SMIX" sim boot "$IOS_UDID" >"$WORK/ios-boot.log" 2>&1 || cannot_judge "could not boot $IOS_UDID"
  IOS_WE_BOOTED=1
fi
"$SMIX" sim install "$IOS_UDID" "$IOS_FIXTURE" >"$WORK/ios-install.log" 2>&1 \
  || cannot_judge "could not install the iOS fixture: $(tail -3 "$WORK/ios-install.log" | tr '\n' ' ')"
SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" runner up "$IOS_UDID" --bundle "$IOS_APPID" --runner-port "$IOS_PORT" \
  --runner-project "$IOS_PROJECT" >"$WORK/ios-up.log" 2>&1 \
  || cannot_judge "the iOS runner would not start: $(tail -5 "$WORK/ios-up.log" | tr '\n' ' ')"
IOS_UPPED=1

for key in lock volumeUp volumeDown; do
  flow "$WORK/ios-$key.yaml" "$IOS_APPID" "$key"
  rc=0
  # raw run: the refusal under test is a DRIVER_ERROR, which smix-run ends a script on
  out="$(SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" run --device "$IOS_UDID" "$WORK/ios-$key.yaml" 2>&1)" || rc=$?
  if [ "$rc" -eq 0 ]; then
    fail "ios: pressKey $key passed — a simulator has no such button, so the step did nothing"
  elif ! printf '%s' "$out" | grep -q 'no_such_button'; then
    printf '%s\n' "$out" | tail -8 >&2
    fail "ios: pressKey $key failed (exit $rc) without the runner's no_such_button"
  elif ! printf '%s' "$out" | grep -q "pressKey $key:"; then
    printf '%s\n' "$out" | tail -8 >&2
    fail "ios: pressKey $key was refused without its reason reaching the output"
  else
    log "ios $key: refused by name — $(printf '%s' "$out" | grep -o "pressKey $key: [^—]*" | head -1)"
  fi
done

[ "$FAILED" = 0 ] || exit 1
log "PASS — Android pressed all three and the device says so; iOS refused all three by name"
