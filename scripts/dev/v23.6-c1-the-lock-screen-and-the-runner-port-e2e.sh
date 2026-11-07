#!/usr/bin/env bash
# v23.6-C1: a lock screen that came back, and a runner not on the default
# port — on a real emulator.
#
# Once its PIN has been entered an emulator's user stays unlocked, but the
# lock screen comes back whenever the screen goes off. `sim boot
# --unlock-pin` judged by the user alone, so it answered `booted` with the
# lock screen up and nothing entered; an app launched under it was then
# reported as being in the background behind the home screen. And a
# runner brought up on a port of its own was dialled on another one by
# every verb not given `--port`.
#
# With the emulator's own PIN set for the run, judged by what the device
# says (`dumpsys window`, `dumpsys user`):
#   1. a flow run under the lock screen fails saying the lock screen is
#      showing, not that the app is in the background
#   2. `sim boot` without the PIN refuses the device the lock screen is
#      over; with a wrong one it says it is still locked
#   3. with the right one it answers once the lock screen is gone
#   4. a verb given no port reaches the runner on the port `runner up`
#      was given
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
ALIAS="${SMIX_C1_ANDROID:-$E2E_ANDROID}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
PIN="7351"
WRONG="2468"
WORK="$(mktemp -d)"

log()  { printf '[v23.6-c1] %s\n' "$*" >&2; }
fail() { printf '[v23.6-c1] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v23.6-c1] cannot judge: %s\n' "$*" >&2; exit 2; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0 WE_SET_PIN=0 WAS_DISABLED=""
cleanup() {
  if [ "$WE_SET_PIN" = 1 ]; then
    adb -s "$SERIAL" shell input keyevent KEYCODE_WAKEUP >/dev/null 2>&1 || true
    if ! said="$(adb -s "$SERIAL" shell locksettings clear --old "$PIN" 2>&1)"; then
      printf '[v23.6-c1] warning: the PIN set for this run was not cleared on %s:\n%s\n' "$SERIAL" "$said" >&2
    fi
    [ -z "$WAS_DISABLED" ] || adb -s "$SERIAL" shell locksettings set-disabled "$WAS_DISABLED" >/dev/null 2>&1 || true
    # the lock screen a run left up stays up once its PIN is gone
    adb -s "$SERIAL" shell wm dismiss-keyguard >/dev/null 2>&1 || true
  fi
  if [ "$WE_UPPED" = 1 ]; then
    if ! said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" 2>&1)"; then
      printf '[v23.6-c1] warning: the runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
    fi
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || fail "no adb on PATH — this judges an Android emulator and cannot"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
[ -f "$APK" ] || fail "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"
SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tr -d '[:space:]')" || true
[ -n "$SERIAL" ] || fail "no emulator registered as '$ALIAS' — register one, or set SMIX_C1_ANDROID"
case "$SERIAL" in
  emulator-*) ;;
  *) fail "'$ALIAS' resolves to $SERIAL, which is not an emulator. This sets a lock-screen PIN \
with adb and will not do that to a phone." ;;
esac

if ! adb devices | grep -q "^${SERIAL}[[:space:]]*device"; then
  "$SMIX" sim boot "$SERIAL" --no-window >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi

keyguard() { adb -s "$SERIAL" shell dumpsys window | sed -n 's/^ *isKeyguardShowing=//p' | tr -d '\r'; }
user_state() { adb -s "$SERIAL" shell dumpsys user | sed -n 's/^ *State: //p' | head -1 | tr -d '\r '; }
wakefulness() { adb -s "$SERIAL" shell dumpsys power | sed -n 's/^ *mWakefulness=//p' | head -1 | tr -d '\r '; }
# relock: the screen off and on again, which brings the lock screen back
# over a user that stays unlocked
relock() {
  adb -s "$SERIAL" shell input keyevent KEYCODE_SLEEP
  local i=0
  until [ "$(wakefulness)" = Asleep ]; do
    i=$((i + 1)); [ "$i" -lt 40 ] || cannot_judge "the screen did not go off within 10 s of KEYCODE_SLEEP"
    sleep 0.25
  done
  adb -s "$SERIAL" shell input keyevent KEYCODE_WAKEUP
  i=0
  until [ "$(keyguard)" = true ]; do
    i=$((i + 1)); [ "$i" -lt 40 ] || cannot_judge "the lock screen did not come back within 10 s of the screen going off and on"
    sleep 0.25
  done
  [ "$(user_state)" = RUNNING_UNLOCKED ] \
    || cannot_judge "the user is $(user_state) under the lock screen, not RUNNING_UNLOCKED — not the case this judges"
}

"$SMIX" sim wake "$SERIAL" >"$WORK/wake.log" 2>&1 || fail "wake: $(tail -3 "$WORK/wake.log")"
[ "$(keyguard)" = false ] || cannot_judge "$SERIAL shows a lock screen before this run set a PIN"
adb -s "$SERIAL" shell dumpsys lock_settings | grep -q "CredentialType: NONE" \
  || cannot_judge "$SERIAL already has a lock-screen credential, which is not this run's to replace"
WAS_DISABLED="$(adb -s "$SERIAL" shell locksettings get-disabled | tr -d '\r ')"
adb -s "$SERIAL" shell locksettings set-pin "$PIN" >/dev/null || fail "could not set a PIN on $SERIAL"
WE_SET_PIN=1
adb -s "$SERIAL" shell dumpsys lock_settings | grep -q "CredentialType: PIN" || fail "the PIN set for this run did not take"

"$SMIX" sim install "$SERIAL" "$APK" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
SMIX_RUNNER_PORT="$PORT" "$SMIX" runner up "$SERIAL" --runner-port "$PORT" >"$WORK/up.log" 2>&1 \
  || fail "runner up: $(tail -5 "$WORK/up.log")"
WE_UPPED=1

# ---- 1. an app under the lock screen ----
relock
printf 'appId: %s\n---\n- launchApp\n- assertVisible: { id: fixture_submit }\n' "$APPID" >"$WORK/fixture.yaml"
# raw run: the failure is the verdict here, read from what smix said, not a run that could not judge
rc=0; SMIX_RUNNER_PORT="$PORT" "$SMIX" run --device "$SERIAL" --runner-port "$PORT" "$WORK/fixture.yaml" \
  >"$WORK/run.out" 2>"$WORK/run.err" || rc=$?
log "a flow under the lock screen: exit $rc, $(grep -o 'FAIL \[[A-Z_]*\]' "$WORK/run.err" | head -1)"
[ "$rc" != 0 ] || fail "a flow under the lock screen passed"
grep -q "the lock screen is showing over it" "$WORK/run.err" \
  || { tail -4 "$WORK/run.err" >&2; fail "the failure does not say the lock screen is over the app"; }
[ "$(keyguard)" = true ] || cannot_judge "the lock screen went away during the flow"

# ---- 2. without the PIN, and with a wrong one ----
rc=0; "$SMIX" sim boot "$SERIAL" >"$WORK/nopin.log" 2>&1 || rc=$?
log "sim boot without the PIN: exit $rc"
[ "$rc" != 0 ] || fail "sim boot answered for a device its lock screen is over, given no PIN"
grep -q "shows its lock screen again" "$WORK/nopin.log" || { cat "$WORK/nopin.log" >&2; fail "the refusal does not say the lock screen is back"; }
rc=0; "$SMIX" sim boot "$SERIAL" --unlock-pin "$WRONG" >"$WORK/wrong.log" 2>&1 || rc=$?
log "sim boot with a wrong PIN: exit $rc"
[ "$rc" != 0 ] || fail "sim boot answered with a wrong PIN"
grep -q "is still locked" "$WORK/wrong.log" || { cat "$WORK/wrong.log" >&2; fail "a wrong PIN is not reported as still locked"; }
! grep -q "$WRONG" "$WORK/wrong.log" || fail "the PIN given is in what sim boot printed"

# ---- 3. with the right one ----
relock
"$SMIX" sim boot "$SERIAL" --unlock-pin "$PIN" >"$WORK/pin.log" 2>&1 || fail "sim boot with the PIN: $(tail -3 "$WORK/pin.log")"
shown="$(keyguard)" state="$(user_state)"
log "sim boot with the PIN: $(grep -c '^unlocked:' "$WORK/pin.log") unlock line, then isKeyguardShowing=$shown, user $state"
grep -q "^unlocked: $SERIAL" "$WORK/pin.log" || fail "sim boot did not say it entered the PIN"
[ "$shown" = false ] || fail "sim boot answered with the lock screen still showing"
[ "$state" = RUNNING_UNLOCKED ] || fail "the user is $state after the PIN"

# ---- 4. no port given ----
[ "$PORT" != 28080 ] && [ "$PORT" != 22087 ] || cannot_judge "the free port is a default one ($PORT), so a dial to it proves nothing"
rc=0; env -u SMIX_RUNNER_PORT "$SMIX" long-press "point:50%,40%" --device "$SERIAL" >"$WORK/press.log" 2>&1 || rc=$?
log "long-press with no port and no platform, the runner on $PORT: exit $rc"
[ "$rc" = 0 ] || { cat "$WORK/press.log" >&2; fail "a verb given no port did not reach the runner on $PORT"; }
adb -s "$SERIAL" shell input keyevent KEYCODE_HOME >/dev/null 2>&1 || true

log "V23.6-C1-E2E-PASS"
