#!/usr/bin/env bash
#
# C9 — the five things a run needs from a handset, driven through smix.
#
# The defects this is the instrument for (a consumer's Android round,
# 2026-09-22): waking the screen, holding it awake, granting a runtime
# permission, reading the resumed activity and reading the crash buffer
# were all reachable only through raw adb, and raw adb against a
# physical serial is refused by smix's own guard — correctly. Four of
# the five had no smix entry at all.
#
# Every judgement here reads the device's own words back, never smix's
# echo of them: `dumpsys power` for the screen, `settings get` for the
# stay-awake bit, `dumpsys package` for the grant, `logcat -b crash` for
# the crash. A command that reported success without the device
# agreeing is the failure this whole checkpoint exists against.
#
# Both ends of every pair are asserted. Asserting only the end state
# lets a device that was already in it pass without anything happening,
# which is a judgement that cannot fail (§14.3).
#
# There is no SKIP path. A missing emulator, a missing apk or a device
# that will not answer are all reasons this cannot judge anything, and a
# gate that cannot judge must be red rather than quiet (open-items I4).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
ALIAS="${SMIX_C9_ANDROID:-sim-smix-android-01}"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"

# No ports anywhere in this script: nothing here starts a runner or
# binds a socket, so there is nothing to ask the OS for. (`gate-port.sh`
# is for the gates that do.)

log()  { printf '[c9-arrange] %s\n' "$*" >&2; }
step() { printf '[c9-arrange] --- %s\n' "$*" >&2; }
fail() { printf '[c9-arrange] FAIL: %s\n' "$*" >&2; exit 1; }

SERIAL="" WE_BOOTED=0 STAY_AWAKE_WAS=""
cleanup() {
  if [ -n "$SERIAL" ]; then
    # Put the stay-awake setting back the way it was found. This script
    # changes it on purpose, and a device left with the screen able to
    # sleep is a device the next run has to wake.
    if [ -n "$STAY_AWAKE_WAS" ]; then
      case "$STAY_AWAKE_WAS" in
        0) adb -s "$SERIAL" shell svc power stayon false >/dev/null 2>&1 || true ;;
        *) adb -s "$SERIAL" shell svc power stayon true >/dev/null 2>&1 || true ;;
      esac
    fi
    # Leave the screen on whatever happened above.
    adb -s "$SERIAL" shell input keyevent KEYCODE_WAKEUP >/dev/null 2>&1 || true
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || fail "no adb on PATH — this judges an Android device and cannot"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
[ -f "$APK" ] || fail "no fixture apk at $APK (assembleDebug in test-fixtures/android-app)"

SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | grep -v '^kevy:' | tail -1)"
if [ -z "$SERIAL" ]; then
  log "nothing is registered as $ALIAS"
  # 2, not 0: nothing was judged. A run that could not look and a run
  # that looked and liked what it saw must not end the same way.
  exit 2
fi

# Never a phone. Every adb below names $SERIAL, and this script puts a
# device to sleep, rewrites a power setting and crashes an app — none of
# which belongs on somebody's own handset.
case "$SERIAL" in
  emulator-[0-9]*) ;;
  *)
    log "$ALIAS resolves to $SERIAL, which is not an emulator — refusing."
    log "this sleeps the screen, rewrites a power setting and crashes an app."
    exit 2
    ;;
esac

if ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  log "booting $ALIAS"
  "$SMIX" sim boot "$ALIAS" >/dev/null 2>&1 || fail "could not boot $ALIAS"
  WE_BOOTED=1
  adb -s "$SERIAL" wait-for-device
fi
step "device: $ALIAS ($SERIAL)"

# Runs a `smix sim` verb and FAILS if it does not succeed.
#
# The first version of this piped through `grep` and ended in `|| true`,
# which threw the exit code away — so a verb could report failure and
# this script would carry on judging the device state alone. A mutation
# that broke the read-back agreement inside smix passed, because the
# device still ended up arranged correctly and nothing here ever asked
# smix whether it thought so. The status is captured before anything
# touches it.
smix_sim() {
  local out status=0
  # `|| status=$?` rather than a bare assignment: under `set -e` a
  # failing command substitution ends the script on the spot, before the
  # next line can read `$?` — so the first version of this exited 1 with
  # no judgement printed at all. A red that says nothing is the shape
  # this suite refuses (a red must carry a verdict, not just a status).
  out="$("$SMIX" sim "$@" 2>&1)" || status=$?
  out="$(printf '%s\n' "$out" | grep -v '^kevy:' || true)"
  if [ "$status" -ne 0 ]; then
    printf '%s\n' "$out" >&2
    fail "smix sim $* exited $status"
  fi
  printf '%s\n' "$out"
}
wakefulness() { adb -s "$SERIAL" shell dumpsys power 2>/dev/null | grep -m1 'mWakefulness=' | tr -d ' \r'; }
stay_setting() { adb -s "$SERIAL" shell settings get global stay_on_while_plugged_in 2>/dev/null | tr -d ' \r'; }
granted() {
  adb -s "$SERIAL" shell dumpsys package "$APPID" 2>/dev/null \
    | grep -m1 'android.permission.CAMERA: granted=' \
    | sed -E 's/.*granted=(true|false).*/\1/' | tr -d ' \r'
}

STAY_AWAKE_WAS="$(stay_setting)"

# The app has to be installed for the permission and crash judgements to
# have a subject at all.
adb -s "$SERIAL" shell pm list packages | grep -q "$APPID" \
  || "$SMIX" sim install "$ALIAS" "$APK" >/dev/null 2>&1 \
  || fail "could not install the fixture"

# --- 1. the screen comes on ------------------------------------------
step "a screen that is off comes on"
# Direct adb: this builds the state under test, and it has to work even
# when the code under test does not.
adb -s "$SERIAL" shell input keyevent KEYCODE_SLEEP >/dev/null 2>&1
sleep 1
before="$(wakefulness)"
# `Dozing` and `Asleep` are both "the screen is not on" — which is the
# state this step needs to build. Measured: the same KEYCODE_SLEEP lands
# in either depending on how the device was last woken, and demanding
# one of them made this red about the setup rather than the subject.
case "$before" in
  mWakefulness=Asleep|mWakefulness=Dozing) ;;
  *) fail "could not put the screen to sleep: read '$before'" ;;
esac
log "  screen-off=yes            ($before)"

smix_sim wake "$ALIAS" >/dev/null
after="$(wakefulness)"
[ "$after" = "mWakefulness=Awake" ] || fail "after 'smix sim wake' the device reads '$after'"
log "  awake=yes                 ($after)"

# --- 2. stay-awake, both ways ----------------------------------------
step "the stay-awake setting is written and reads back"
smix_sim stay-awake "$ALIAS" off >/dev/null
off_reads="$(stay_setting)"
[ "$off_reads" = "0" ] || fail "'stay-awake off' left the setting at '$off_reads', expected 0"
log "  stay-awake-off=0          (settings get stay_on_while_plugged_in)"

smix_sim stay-awake "$ALIAS" on >/dev/null
on_reads="$(stay_setting)"
[ -n "$on_reads" ] && [ "$on_reads" != "0" ] && [ "$on_reads" != "null" ] \
  || fail "'stay-awake on' left the setting at '$on_reads', expected non-zero"
log "  stay-awake-on=$on_reads          (settings get stay_on_while_plugged_in)"

# --- 3. the frontmost app, and that it changes -----------------------
step "the resumed activity is read, and follows what is in front"
adb -s "$SERIAL" shell am start -n "$APPID/.ComposeActivity" >/dev/null 2>&1
sleep 2
front="$(smix_sim frontmost "$ALIAS" --json)"
echo "$front" | grep -q "\"package\":\"$APPID\"" \
  || fail "with the fixture in front, frontmost says: $front"
log "  frontmost-app=$APPID  ($front)"

adb -s "$SERIAL" shell input keyevent KEYCODE_HOME >/dev/null 2>&1
sleep 2
home_front="$(smix_sim frontmost "$ALIAS" --json)"
# Reading a value once proves nothing about whether it was read at all;
# it has to follow the screen.
echo "$home_front" | grep -q "\"package\":\"$APPID\"" \
  && fail "after HOME the fixture is still reported frontmost: $home_front"
log "  frontmost-follows=yes     ($home_front)"

# --- 4. a runtime permission, both ways ------------------------------
step "a permission is granted and revoked, read from the package manager"
smix_sim permission "$ALIAS" "$APPID" camera revoke >/dev/null
revoked="$(granted)"
[ "$revoked" = "false" ] || fail "after revoke the package manager says granted=$revoked"
log "  camera-revoked=true       (dumpsys package: granted=false)"

smix_sim permission "$ALIAS" "$APPID" camera grant >/dev/null
regranted="$(granted)"
[ "$regranted" = "true" ] || fail "after grant the package manager says granted=$regranted"
log "  camera-granted=true       (dumpsys package: granted=true)"

# --- 5. the crash buffer ---------------------------------------------
step "a crash the device records is read back, and an empty buffer is not a failure"
adb -s "$SERIAL" logcat -b crash -c >/dev/null 2>&1
empty_out="$(smix_sim crashes "$ALIAS" --app "$APPID")"
# Exit code on its own line: a read that found nothing must not end a
# run under `set -e`, and that is the first thing a caller will bet on.
if ! "$SMIX" sim crashes "$ALIAS" --app "$APPID" >/dev/null 2>&1; then
  fail "'smix sim crashes' exited non-zero on a device with no crashes"
fi
echo "$empty_out" | grep -q "0 of 0 report" \
  || fail "after clearing the buffer, crashes says: $empty_out"
log "  empty-buffer=0-reports    (and exit 0)"

adb -s "$SERIAL" shell am start -n "$APPID/.MainActivity" >/dev/null 2>&1
sleep 2
adb -s "$SERIAL" shell am crash "$APPID" >/dev/null 2>&1
# The buffer lags the crash by a few seconds — measured 2026-09-23, a
# read one second after `am crash` still showed nothing while the main
# log already had the FATAL line. One look cannot answer a question
# about a transition.
found=""
for _ in $(seq 1 15); do
  sleep 1
  out="$(smix_sim crashes "$ALIAS" --app "$APPID")"
  if echo "$out" | grep -qE "[1-9][0-9]* of [0-9]+ report"; then
    found="$out"
    break
  fi
done
[ -n "$found" ] || fail "15s after crashing $APPID the crash buffer still reports none"
echo "$found" | grep -q "FATAL EXCEPTION" \
  || fail "the report does not carry the device's own words: $found"
log "  crash-read=yes            ($(echo "$found" | tail -1))"

log "C9-ARRANGE-E2E-PASS on $SERIAL (screen, stay-awake, frontmost, permission and crash buffer all read back from the device)"
