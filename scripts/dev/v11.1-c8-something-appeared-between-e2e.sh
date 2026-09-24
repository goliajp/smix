#!/usr/bin/env bash
# Something that appeared between two steps, and a control that leaves on
# its own — on both platforms.
#
# The defects this is the instrument for (a consumer's round, 2026-09-23):
# a promise that "nothing stands over the picture" while a recording
# opens could only be checked after the fact, by which time a loading
# state that flashed for 200 ms was gone and the check passed on a run
# that showed exactly what it forbids; and a player's controls, which
# take themselves away a few seconds after the picture is touched, were
# missed by a wait followed by a tap.
#
# Each fixture has a `Flash` button (a moment after the press, an overlay
# stands over the screen for 400 ms and goes, then `done` appears), a
# `Quiet` button (the same time, no overlay, then `done`), and a `Reveal`
# button (shows a button that takes itself away after 3 s and records,
# on the app's own clock, how long after it appeared a press came).
#
#   flash    neverVisible overlay during [tap Flash, wait for done]   must FAIL, naming when and which step
#   quiet    neverVisible overlay during [tap Quiet, wait for done]   must pass, saying how often it looked
#   tap      tap Reveal → tap the vanishing button                   the app counts the press
#   wait-tap tap Reveal → wait for it → tap it (the consumer's shape) the app counts the press
#
# The two press legs print the app's own "appeared → pressed" time, which
# is what the advice in the guide ("tap it; do not wait for it first")
# rests on.
#
# The quiet leg is not a formality: a watch that never looked would pass
# it too, which is why the pass has to say how many looks there were.
#
# Exit: 0 judged and passed, 1 judged and failed, 2 could not judge.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/gate-port.sh"
source "$ROOT/scripts/lib/deadline.sh"
AND_PORT="$SMIX_RUNNER_PORT"
gate_free_port IOS_PORT
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/e2e-devices.sh"
AND_ALIAS="${SMIX_C8_ANDROID:-$E2E_ANDROID}"
IOS_ALIAS="${SMIX_C8_IOS:-$E2E_IOS}"
AND_APPID="dev.smix.fixture"
IOS_APPID="jp.golia.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
APP="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
WORK="$(mktemp -d)"
KEEP="${SMIX_C8_KEEP:-}"   # a directory to copy each flow's log into, for reading afterwards

log()  { printf '[c8-between] %s\n' "$*" >&2; }
fail() { printf '[c8-between] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c8-between] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

SERIAL="" UDID="" AND_UPPED=0 IOS_UPPED=0 WE_BOOTED=0 IOS_WE_BOOTED=0
cleanup() {
  local said
  if [ "$AND_UPPED" = 1 ]; then
    said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$AND_PORT" 2>&1)" \
      || printf '[c8-between] warning: the Android runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
  fi
  if [ "$IOS_UPPED" = 1 ]; then
    said="$("$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" 2>&1)" \
      || printf '[c8-between] warning: the iOS runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$AND_ALIAS" >/dev/null 2>&1 || true; fi
  if [ "$IOS_WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || cannot_judge "no adb on PATH"
[ -f "$APK" ] || cannot_judge "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
[ -d "$APP" ] || cannot_judge "no iOS fixture — run: bash scripts/dev/build-fixture-app.sh"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"

FAILED=0
DENSITY=""

# One flow. $1 platform, $2 device, $3 port, $4 label, $5 flow file.
# Sets RC and OUT.
run_flow() {
  RC=0
  OUT="$(SMIX_RUNNER_PORT="$3" with_deadline 120 "$SMIX_RUN" --device "$2" "$5" 2>&1)" || RC=$?
  [ "$RC" = "$DEADLINE_STATUS" ] && cannot_judge "$1 $4: smix run did not answer within 120 s"
  printf '%s\n' "$OUT" > "$WORK/$1-$4.log"
  if [ -n "$KEEP" ]; then mkdir -p "$KEEP" && cp "$WORK/$1-$4.log" "$KEEP/"; fi
  true
}

judge_flash() { # $1 platform
  local when
  # `|| true`: a flow that did not produce the line is judged below; under
  # pipefail an empty grep would otherwise end the script here, red with
  # no verdict printed.
  when="$(printf '%s' "$OUT" | grep -o 'appeared after [0-9]* ms, [^—]*' | head -1 || true)"
  if [ "$RC" != 0 ] && [ -n "$when" ] \
     && printf '%s' "$when" | grep -qE 'while step [0-9]+ of 2 \((tapOn|extendedWaitUntil)\)'; then
    log "  $1 flash: failed, as it should — $when"
  else
    printf '[c8-between] FAIL: %s flash: expected to fail naming the time and the inner step, exited %s — %s\n' \
      "$1" "$RC" "$(printf '%s' "$OUT" | tail -4 | tr '\n' ' ')" >&2
    FAILED=1
  fi
}

judge_quiet() { # $1 platform
  local line looks
  line="$(printf '%s' "$OUT" | grep -o 'not seen — watched [0-9]* times over [0-9]* ms, longest gap [0-9]* ms' | head -1 || true)"
  looks="$(printf '%s' "$line" | awk '{print $5}')"
  if [ "$RC" = 0 ] && [ -n "$line" ] && [ "${looks:-0}" -ge 1 ]; then
    log "  $1 quiet: passed, as it should — $line"
    DENSITY="$DENSITY $1: $line;"
  else
    printf '[c8-between] FAIL: %s quiet: expected to pass having looked at least once, exited %s — %s\n' \
      "$1" "$RC" "$(printf '%s' "$OUT" | tail -4 | tr '\n' ' ')" >&2
    FAILED=1
  fi
}

# The press count and the latency, read from the app's own labels in the
# tree — not from smix's account of having pressed.
judge_vanish() { # $1 platform, $2 device, $3 port, $4 presses id, $5 latency id, $6 leg name
  local tree presses latency
  tree="$(SMIX_RUNNER_PORT="$3" with_deadline 60 "$SMIX" tree --device "$2" --port "$3" --json 2>/dev/null)" \
    || cannot_judge "$1 $6: could not read the tree afterwards"
  presses="$(printf '%s' "$tree" | python3 -c "
import json,sys
def walk(n):
    yield n
    for c in n.get('children',[]): yield from walk(c)
t=json.load(sys.stdin); t=t.get('root',t)
for n in walk(t):
    if n.get('identifier')=='$4': print(n.get('label') or n.get('value') or n.get('text') or '')
")"
  latency="$(printf '%s' "$tree" | python3 -c "
import json,sys
def walk(n):
    yield n
    for c in n.get('children',[]): yield from walk(c)
t=json.load(sys.stdin); t=t.get('root',t)
for n in walk(t):
    if n.get('identifier')=='$5': print(n.get('label') or n.get('value') or n.get('text') or '')
")"
  if [ "$RC" = 0 ] && [ "$presses" = "presses 1" ]; then
    log "  $1 $6: pressed, as it should — app says '$presses', '$latency' ms after it appeared"
    DENSITY="$DENSITY $1 $6: $latency;"
  else
    printf '[c8-between] FAIL: %s %s: expected the app to count one press, it says %s (flow exited %s) — %s\n' \
      "$1" "$6" "'${presses:-nothing}'" "$RC" "$(printf '%s' "$OUT" | tail -3 | tr '\n' ' ')" >&2
    FAILED=1
  fi
}

# $1 file, $2 appId, $3 platform, $4 overlay id, $5 button, $6 done id
write_watch() {
  {
    printf 'appId: %s\n---\n' "$2"
    [ "$3" = ios ] && ios_open
    printf -- '- neverVisible:\n    id: %s\n    during:\n' "$4"
    printf -- '      - tapOn:\n          id: %s\n' "$5"
    printf -- '      - extendedWaitUntil:\n          visible:\n            id: %s\n          timeout: 5000\n' "$6"
  } > "$1"
}

# $1 file, $2 appId, $3 platform, $4 reveal id, $5 vanishing id
write_vanish() {
  {
    printf 'appId: %s\n---\n' "$2"
    [ "$3" = ios ] && ios_open
    printf -- '- tapOn:\n    id: %s\n' "$4"
    printf -- '- tapOn:\n    id: %s\n' "$5"
  } > "$1"
}

# The consumer's shape: wait for it, then tap it. Measured beside the
# plain tap, so the advice to drop the wait carries a number.
# $1 file, $2 appId, $3 platform, $4 reveal id, $5 vanishing id
write_wait_then_tap() {
  {
    printf 'appId: %s\n---\n' "$2"
    [ "$3" = ios ] && ios_open
    printf -- '- tapOn:\n    id: %s\n' "$4"
    printf -- '- extendedWaitUntil:\n    visible:\n      id: %s\n    timeout: 3000\n' "$5"
    printf -- '- tapOn:\n    id: %s\n' "$5"
  } > "$1"
}

ios_open() {
  cat <<'OPEN'
- stopApp
- launchApp
- scrollUntilVisible:
    element:
      id: fixture-detail-link
    direction: DOWN
- tapOn:
    id: fixture-detail-link
- tapOn:
    id: fixture-watch-link
- assertVisible:
    id: watch-flash
OPEN
}

# ---- Android ----------------------------------------------------------
# Boot by alias first, then ask for the serial: the alias resolves only to
# an AVD that is running.
if ! SERIAL="$("$SMIX" sim resolve "$AND_ALIAS" 2>/dev/null | tail -1)" \
   || [ -z "$SERIAL" ] || ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  log "booting $AND_ALIAS"
  with_deadline 300 "$SMIX" sim boot "$AND_ALIAS" >/dev/null 2>&1 || cannot_judge "could not boot $AND_ALIAS"
  WE_BOOTED=1
  SERIAL="$("$SMIX" sim resolve "$AND_ALIAS" 2>/dev/null | tail -1)"
fi
case "$SERIAL" in
  emulator-*) : ;;
  *) cannot_judge "$AND_ALIAS resolves to '$SERIAL', which is not an emulator — refusing" ;;
esac
with_deadline 120 adb -s "$SERIAL" wait-for-device || cannot_judge "$SERIAL did not come up"
for _ in $(seq 1 60); do
  adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done
with_deadline 180 adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture on $SERIAL"
with_deadline 300 "$SMIX" runner up "$SERIAL" --platform android --runner-port "$AND_PORT" --force >/dev/null 2>&1 \
  || cannot_judge "the Android runner would not start on $SERIAL:$AND_PORT"
AND_UPPED=1

and_fresh() { # a fresh WatchActivity: -S stops the app first
  with_deadline 60 adb -s "$SERIAL" shell am start -S -W -n "$AND_APPID/.WatchActivity" >/dev/null 2>&1 \
    || fail "could not start the watch screen on $SERIAL"
  sleep 2
}

log "--- Android ($SERIAL)"
write_watch "$WORK/a-flash.yaml" "$AND_APPID" android watch_overlay watch_flash watch_done
write_watch "$WORK/a-quiet.yaml" "$AND_APPID" android watch_overlay watch_quiet watch_done
write_vanish "$WORK/a-vanish.yaml" "$AND_APPID" android watch_reveal watch_vanishing
and_fresh; run_flow android "$SERIAL" "$AND_PORT" flash "$WORK/a-flash.yaml"; judge_flash android
and_fresh; run_flow android "$SERIAL" "$AND_PORT" quiet "$WORK/a-quiet.yaml"; judge_quiet android
write_wait_then_tap "$WORK/a-wait-tap.yaml" "$AND_APPID" android watch_reveal watch_vanishing
and_fresh; run_flow android "$SERIAL" "$AND_PORT" vanish "$WORK/a-vanish.yaml"
judge_vanish android "$SERIAL" "$AND_PORT" watch_presses watch_latency tap
and_fresh; run_flow android "$SERIAL" "$AND_PORT" wait-tap "$WORK/a-wait-tap.yaml"
judge_vanish android "$SERIAL" "$AND_PORT" watch_presses watch_latency wait-then-tap

# ---- iOS --------------------------------------------------------------
if ! UDID="$("$SMIX" sim resolve "$IOS_ALIAS" 2>/dev/null | tail -1)" || [ -z "$UDID" ]; then
  cannot_judge "no iOS simulator registered as $IOS_ALIAS"
fi
if ! xcrun simctl list devices -j | python3 -c "import json,sys;d=json.load(sys.stdin);sys.exit(0 if any(x['udid']=='$UDID' and x['state']=='Booted' for r in d['devices'].values() for x in r) else 1)"; then
  log "booting $IOS_ALIAS"
  with_deadline 300 "$SMIX" sim boot "$UDID" >/dev/null 2>&1 || cannot_judge "could not boot $IOS_ALIAS"
  IOS_WE_BOOTED=1
fi
with_deadline 180 "$SMIX" sim install "$UDID" "$APP" >/dev/null 2>&1 || fail "could not install the iOS fixture on $UDID"
with_deadline 600 "$SMIX" runner up "$UDID" --bundle "$IOS_APPID" --runner-port "$IOS_PORT" >/dev/null 2>&1 \
  || cannot_judge "the iOS runner would not start on $UDID:$IOS_PORT"
IOS_UPPED=1

log "--- iOS ($UDID)"
write_watch "$WORK/i-flash.yaml" "$IOS_APPID" ios watch-overlay watch-flash watch-done
write_watch "$WORK/i-quiet.yaml" "$IOS_APPID" ios watch-overlay watch-quiet watch-done
write_vanish "$WORK/i-vanish.yaml" "$IOS_APPID" ios watch-reveal watch-vanishing
run_flow ios "$UDID" "$IOS_PORT" flash "$WORK/i-flash.yaml"; judge_flash ios
run_flow ios "$UDID" "$IOS_PORT" quiet "$WORK/i-quiet.yaml"; judge_quiet ios
write_wait_then_tap "$WORK/i-wait-tap.yaml" "$IOS_APPID" ios watch-reveal watch-vanishing
run_flow ios "$UDID" "$IOS_PORT" vanish "$WORK/i-vanish.yaml"
judge_vanish ios "$UDID" "$IOS_PORT" watch-presses watch-latency tap
run_flow ios "$UDID" "$IOS_PORT" wait-tap "$WORK/i-wait-tap.yaml"
judge_vanish ios "$UDID" "$IOS_PORT" watch-presses watch-latency wait-then-tap

log "measured:$DENSITY"
[ "$FAILED" = 0 ] || exit 1
log "C8-BETWEEN-E2E-PASS (an overlay that flashed between two steps failed, naming when and which step; a quiet span passed, saying how often it looked; a control that leaves on its own was pressed — on both platforms)"
