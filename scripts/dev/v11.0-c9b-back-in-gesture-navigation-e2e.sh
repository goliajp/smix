#!/usr/bin/env bash
# v11.0-C9b: under gesture navigation, `back` closes the system share
# sheet — by the flow verb, by `pressKey: back`, and by `smix press-key
# back`.
#
# A consumer (feedback #8) had a share sheet open on a phone in gesture
# navigation and no way out: there is no back button on screen to
# `tapOn: { id: back }` (that element exists only in three-button
# navigation), `smix press-key` did not know the name `back`, and the
# flow's `back` verb is where they did not look. Three hand-copied key
# tables had drifted apart and none had `back`.
#
# The verdict is read from the device's window stack, not from smix:
# before, the share sheet's package has focus and its chooser activity
# is in the stack; after, the fixture has focus and the chooser is gone.
# smix saying it went back is not the evidence — that is what is under
# test.
#
# The emulator is switched to gesture navigation for the run and put
# back to what it was, whatever happens: the original overlay is read
# first and the trap that restores it is installed before anything
# changes.
#
# Exit: 0 judged and passed, 1 judged and failed, 2 could not judge.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/gate-port.sh"
source "$ROOT/scripts/lib/deadline.sh"
PORT="$SMIX_RUNNER_PORT"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/e2e-devices.sh"
ALIAS="${SMIX_C9B_ANDROID:-$E2E_ANDROID}"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"
GESTURAL="com.android.internal.systemui.navbar.gestural"

log()  { printf '[c9b-back] %s\n' "$*" >&2; }
fail() { printf '[c9b-back] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c9b-back] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

# Neither stops reading early: an awk that exits at its first match
# closes the pipe on a writer still writing, and under pipefail that
# SIGPIPE becomes the function's status and `set -e` ends the script
# without a word — which is what the first run of this did.
focused_pkg() { # the package whose window has focus
  local dump
  dump="$(with_deadline 30 adb -s "$SERIAL" shell dumpsys window 2>/dev/null | tr -d '\r')" || return 0
  printf '%s\n' "$dump" \
    | awk '!done && /mCurrentFocus=/ {for (i = 1; i <= NF; i++) if (!done && $i ~ /\//) {split($i, a, "/"); sub(/}$/, "", a[1]); print a[1]; done = 1}}'
}
# Only `Hist` entries are the stack. The dump also names the last paused
# activity, and a chooser that has finished stays there (`t-1 f`) — the
# first version counted that line and called a closed sheet open.
chooser_count() { # chooser activities in the stack
  local dump
  dump="$(with_deadline 30 adb -s "$SERIAL" shell dumpsys activity activities 2>/dev/null | tr -d '\r')" \
    || { echo "<no answer>"; return 0; }
  printf '%s\n' "$dump" | grep -c 'Hist .*ChooserActivity' || true
}

SERIAL="" WE_BOOTED=0 UPPED=0 NAV_TOUCHED=0 OPENED=0 ORIG_OVERLAY="" ORIG_MODE=""
cleanup() {
  local said now
  # A sheet this run opened and a failure left up would sit over the
  # next run's app; `launchApp: stopApp` does not close another
  # package's window. Only after this run opened one, which is only
  # ever on the emulator checked below.
  if [ "$OPENED" = 1 ] && [ "$(chooser_count)" != 0 ]; then
    with_deadline 30 adb -s "$SERIAL" shell input keyevent 4 >/dev/null 2>&1 || true
  fi
  if [ "$UPPED" = 1 ]; then
    said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" 2>&1)" \
      || printf '[c9b-back] warning: the runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
  fi
  if [ "$NAV_TOUCHED" = 1 ]; then
    # Back to the overlay that was on, or — when none was, which is the
    # stock emulator image's three-button default — to none.
    if [ -n "$ORIG_OVERLAY" ]; then
      with_deadline 30 adb -s "$SERIAL" shell cmd overlay enable-exclusive --category "$ORIG_OVERLAY" \
        >/dev/null 2>&1 || true
    else
      with_deadline 30 adb -s "$SERIAL" shell cmd overlay disable "$GESTURAL" >/dev/null 2>&1 || true
    fi
    # The setting follows the overlay a few seconds later.
    now="<no answer>"
    for _ in $(seq 1 30); do
      now="$(with_deadline 30 adb -s "$SERIAL" shell settings get secure navigation_mode 2>/dev/null | tr -d '\r')" \
        || now="<no answer>"
      [ "$now" = "$ORIG_MODE" ] && break
      sleep 1
    done
    if [ "$now" = "$ORIG_MODE" ]; then
      printf '[c9b-back] navigation restored: %s (navigation_mode=%s)\n' "${ORIG_OVERLAY:-no overlay}" "$now" >&2
    else
      printf '[c9b-back] WARNING: navigation NOT restored — navigation_mode is %s, was %s (overlay before: %s)\n' \
        "$now" "$ORIG_MODE" "${ORIG_OVERLAY:-none}" >&2
    fi
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$ALIAS" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || cannot_judge "no adb on PATH"
[ -f "$APK" ] || cannot_judge "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"

# Boot by alias first, then ask for the serial: the alias resolves only to
# an AVD that is running.
if ! SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" \
   || [ -z "$SERIAL" ] || ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  log "booting $ALIAS"
  with_deadline 300 "$SMIX" sim boot "$ALIAS" >/dev/null 2>&1 || cannot_judge "could not boot $ALIAS"
  WE_BOOTED=1
  SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)"
fi
case "$SERIAL" in
  emulator-*) : ;;
  *) cannot_judge "$ALIAS resolves to '$SERIAL', which is not an emulator — refusing" ;;
esac
with_deadline 120 adb -s "$SERIAL" wait-for-device || cannot_judge "$SERIAL did not come up"
for _ in $(seq 1 60); do
  adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done

# ---- what the navigation is now, before anything changes it ----------
ORIG_MODE="$(with_deadline 30 adb -s "$SERIAL" shell settings get secure navigation_mode | tr -d '\r')" \
  || cannot_judge "could not read navigation_mode on $SERIAL"
ORIG_OVERLAY="$(with_deadline 30 adb -s "$SERIAL" shell cmd overlay list | tr -d '\r' \
  | awk '/\[x\] com\.android\.internal\.systemui\.navbar\./ {print $2; exit}')" || true
log "navigation before: ${ORIG_OVERLAY:-no overlay} (navigation_mode=$ORIG_MODE)"

NAV_TOUCHED=1
with_deadline 30 adb -s "$SERIAL" shell cmd overlay enable-exclusive --category "$GESTURAL" >/dev/null \
  || cannot_judge "could not switch $SERIAL to gesture navigation"
MODE=""
for _ in $(seq 1 30); do
  MODE="$(adb -s "$SERIAL" shell settings get secure navigation_mode | tr -d '\r')"
  [ "$MODE" = 2 ] && break
  sleep 1
done
[ "$MODE" = 2 ] || cannot_judge "navigation_mode is $MODE after switching to gesture navigation, not 2"
log "navigation now: gestural (navigation_mode=$MODE)"

with_deadline 180 adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture on $SERIAL"
with_deadline 300 "$SMIX" runner up "$SERIAL" --platform android --runner-port "$PORT" --force >/dev/null 2>&1 \
  || cannot_judge "the runner would not start on $SERIAL:$PORT"
UPPED=1


# Nothing of a previous run's may be up: every judgement below is
# "the chooser was there and now is not".
n="$(chooser_count)"
[ "$n" = 0 ] || cannot_judge "a chooser is already in $SERIAL's stack ($n) before this run opened one"

cat >"$WORK/open-share.yaml" <<FLOW
appId: $APPID
---
- launchApp:
    stopApp: true
- tapOn:
    label: open-share
FLOW
cat >"$WORK/back-verb.yaml" <<FLOW
appId: $APPID
---
- back
FLOW
cat >"$WORK/back-key.yaml" <<FLOW
appId: $APPID
---
- pressKey: back
FLOW

open_share() {
  local out pkg
  OPENED=1
  out="$(SMIX_RUNNER_PORT="$PORT" with_deadline 120 "$SMIX_RUN" --device "$SERIAL" "$WORK/open-share.yaml" 2>&1)" \
    || fail "could not open the share sheet: $(printf '%s' "$out" | tail -5)"
  for _ in $(seq 1 20); do
    pkg="$(focused_pkg)"
    [ -n "$pkg" ] && [ "$pkg" != "$APPID" ] && [ "$(chooser_count)" -gt 0 ] && break
    sleep 0.5
  done
  [ "$(chooser_count)" -gt 0 ] || fail "the share sheet never came up (focus: $pkg)"
  [ "$pkg" != "$APPID" ] || fail "the chooser is in the stack but the fixture still has focus"
  SHEET_PKG="$pkg"
}

judge_closed() { # judge_closed <how>
  local pkg="" n=""
  for _ in $(seq 1 20); do
    pkg="$(focused_pkg)"
    n="$(chooser_count)"
    [ "$pkg" = "$APPID" ] && [ "$n" = 0 ] && break
    sleep 0.5
  done
  log "  $1: before=$SHEET_PKG after=$pkg chooser-in-stack=$n"
  [ "$pkg" = "$APPID" ] || fail "$1: the fixture did not get focus back (focus: $pkg)"
  [ "$n" = 0 ] || fail "$1: the chooser is still in the stack ($n)"
}

SHEET_PKG=""

log "--- flow verb: - back"
open_share
out="$(SMIX_RUNNER_PORT="$PORT" with_deadline 120 "$SMIX_RUN" --device "$SERIAL" "$WORK/back-verb.yaml" 2>&1)" \
  || fail "- back: the flow failed: $(printf '%s' "$out" | tail -5)"
judge_closed "- back"

log "--- flow key: pressKey: back"
open_share
out="$(SMIX_RUNNER_PORT="$PORT" with_deadline 120 "$SMIX_RUN" --device "$SERIAL" "$WORK/back-key.yaml" 2>&1)" \
  || fail "pressKey: back: the flow failed: $(printf '%s' "$out" | tail -5)"
judge_closed "pressKey: back"

log "--- CLI: smix press-key back"
open_share
out="$(with_deadline 120 "$SMIX" press-key back --device "$SERIAL" --port "$PORT" 2>&1)" \
  || fail "smix press-key back failed: $(printf '%s' "$out" | tail -5)"
judge_closed "smix press-key back"

log "C9B-BACK-E2E-PASS on $SERIAL (gesture navigation; the share sheet closed three ways, read from the window stack)"
