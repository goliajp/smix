#!/usr/bin/env bash
# v12.2-C4: a run puts an emulator's animation scales back.
#
# `smix run` set the three animation scales to 0 before its first step
# and nothing ever put them back. A consumer's app reads
# `animator_duration_scale == 0` as the reduce-motion preference, so after
# one run their emulator's app stayed in reduced motion for good.
#
# Judged here on the device, from values a run could not mistake for its
# own: 0.5, 1.5 and a key that is absent. Each case reads the scales while
# the run is under way (all three 0) and after it has gone (the values it
# found), for a flow that fails, a run stopped with SIGINT, and a run
# killed outright — whose scales the next run on the device puts back.
#
# The machine record is kept in a directory of this script's own, and the
# device's own scales are put back when it ends.
#
# There is no SKIP path: a gate that cannot judge must be red.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
ALIAS="${SMIX_V122_C4_ANDROID:-$E2E_ANDROID}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
KEYS=(window_animation_scale transition_animation_scale animator_duration_scale)

log()  { printf '[v12.2-c4] %s\n' "$*" >&2; }
fail() { printf '[v12.2-c4] FAIL: %s\n' "$*" >&2; exit 1; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0 WORK="" ORIGINAL="" RUN_PID=""
scales() {
  local k
  for k in "${KEYS[@]}"; do
    printf '%s ' "$(adb -s "$SERIAL" shell settings get global "$k" | tr -d '\r')"
  done
}
set_scales() {
  local i=0 v
  for v in "$@"; do
    if [ "$v" = null ]; then
      adb -s "$SERIAL" shell settings delete global "${KEYS[$i]}" >/dev/null
    else
      adb -s "$SERIAL" shell settings put global "${KEYS[$i]}" "$v"
    fi
    i=$((i + 1))
  done
}
cleanup() {
  if [ -n "$RUN_PID" ] && kill -0 "$RUN_PID" 2>/dev/null; then kill "$RUN_PID" 2>/dev/null || true; fi
  if [ -n "$ORIGINAL" ]; then
    # shellcheck disable=SC2086
    set_scales $ORIGINAL || printf '[v12.2-c4] warning: the scales were not put back to %s\n' "$ORIGINAL" >&2
  fi
  [ -n "$WORK" ] && [ -z "${KEEP_WORK:-}" ] && rm -rf "$WORK"
  if [ "$WE_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
      >/dev/null 2>&1 || printf '[v12.2-c4] warning: the runner was not stopped\n' >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || fail "no adb on PATH — this judges an Android device and cannot"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
[ -f "$APK" ] || fail "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"

if ! SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" \
   || [ -z "$SERIAL" ] || ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  log "booting $ALIAS"
  with_deadline 300 "$SMIX" sim boot "$ALIAS" >/dev/null 2>&1 || fail "could not boot $ALIAS"
  WE_BOOTED=1
  SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" || fail "$ALIAS was booted and does not resolve"
fi
case "$SERIAL" in
  emulator-*) : ;;
  # This writes the device's global settings; a phone's are its owner's.
  *) fail "$ALIAS resolves to '$SERIAL', which is not an emulator — refusing" ;;
esac
with_deadline 120 adb -s "$SERIAL" wait-for-device || fail "$SERIAL did not come up"
for _ in $(seq 1 60); do
  adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done

ORIGINAL="$(scales)"
log "the device's own scales: $ORIGINAL"
adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"
if ! curl -s -m 5 "http://localhost:$PORT/health" >/dev/null 2>&1; then
  "$SMIX" runner up "$ALIAS" --platform android --runner-port "$PORT" >/dev/null 2>&1 \
    || fail "the runner would not start on $SERIAL:$PORT"
  WE_UPPED=1
fi

WORK="$(mktemp -d)"
export SMIX_MACHINE_DIR="$WORK/machine"
# Fails after six seconds, which is the window the scales are read in.
cat >"$WORK/wait.yaml" <<EOF
appId: $APPID
---
- launchApp
- extendedWaitUntil:
    visible: "text that is never on this screen"
    timeout: 6000
EOF
cat >"$WORK/quick.yaml" <<EOF
appId: $APPID
---
- launchApp
EOF
SET="0.5 1.5 null"
WANT="0.5 1.5 null "
start_run() {
  SMIX_RUNNER_PORT="$PORT" "$SMIX" run --platform android --device "$SERIAL" "$WORK/$1.yaml" >"$WORK/$1.log" 2>&1 &
  RUN_PID=$!
}
# Whatever spelling the device uses for zero, three of them.
all_zero() { [ "$(printf '%s' "$1" | tr ' ' '\n' | grep -cE '^0(\.0)?$')" = 3 ]; }
quiet_while_running() {
  local seen=""
  for _ in $(seq 1 20); do
    seen="$(scales)"
    all_zero "$seen" && return 0
    sleep 0.5
  done
  fail "$1: the scales were not quietened while the run was under way (read $seen)"
}

# shellcheck disable=SC2086
set_scales $SET
[ "$(scales)" = "$WANT" ] || fail "could not set the scales to $SET (read $(scales))"

log "--- a flow that fails"
start_run wait
quiet_while_running "failing flow"
rc=0; wait "$RUN_PID" || rc=$?; RUN_PID=""
[ "$rc" -ne 0 ] || fail "the failing flow passed — this case is about a run that fails"
[ "$(scales)" = "$WANT" ] || { tail -5 "$WORK/wait.log" >&2; fail "after a failed flow the scales are $(scales), not $WANT"; }
log "  quiet while it ran, $WANT after"

log "--- a run stopped with SIGINT"
start_run wait
quiet_while_running "interrupted run"
kill -INT "$RUN_PID"
wait "$RUN_PID" || true; RUN_PID=""
[ "$(scales)" = "$WANT" ] || { tail -5 "$WORK/wait.log" >&2; fail "after SIGINT the scales are $(scales), not $WANT"; }
log "  quiet while it ran, $WANT after"

log "--- a run killed outright, then the next run"
start_run wait
quiet_while_running "killed run"
kill -KILL "$RUN_PID"
wait "$RUN_PID" 2>/dev/null || true; RUN_PID=""
left="$(scales)"
all_zero "$left" || fail "a killed run cannot put anything back, and yet the scales are $left"
start_run quick
rc=0; wait "$RUN_PID" || rc=$?; RUN_PID=""
[ "$rc" -eq 0 ] || { tail -5 "$WORK/quick.log" >&2; fail "the run after the killed one failed (exit $rc)"; }
[ "$(scales)" = "$WANT" ] || fail "the run after a killed one left $(scales), not the $WANT the killed run found"
log "  the killed run left $left; the next run put back $WANT"

log "V12.2-C4-E2E-PASS on $SERIAL"
