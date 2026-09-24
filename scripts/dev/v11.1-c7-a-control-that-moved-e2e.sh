#!/usr/bin/env bash
# A control that moved, and one that did not — on both platforms.
#
# The defect this is the instrument for (a consumer's round, 2026-09-23):
# a flow could say "this is visible" in each state and nothing about
# whether the layout jumped between them, so a promise that the PTZ panel
# "does not shift by a pixel" could only ever be partly proved.
# `rememberBounds` / `assertBoundsUnchanged` are the pair that says it.
#
# Each fixture has an anchor, a "Move" button that pushes the anchor down
# by exactly 8 device-independent pixels (a spacer above it grows — a
# layout move, not a paint), and a "Stay" button that moves nothing. Three
# flows per platform, each from a fresh screen:
#
#   stay            remember → press Stay → assert         must pass
#   move            remember → press Move → assert         must FAIL, printing both boxes
#   move within 8   remember → press Move → assert within: 8   must pass
#
# The middle one failing is the verdict, and the third passing is what
# proves the comparison is in device-independent pixels: on Android the
# move is 8 × density physical pixels (21 at 2.625), which `within: 8`
# would refuse if the boxes were compared raw.
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
AND_ALIAS="${SMIX_C7_ANDROID:-$E2E_ANDROID}"
IOS_ALIAS="${SMIX_C7_IOS:-$E2E_IOS}"
AND_APPID="dev.smix.fixture"
IOS_APPID="jp.golia.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
APP="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
WORK="$(mktemp -d)"
KEEP="${SMIX_C7_KEEP:-}"   # a directory to copy each flow's log into, for reading afterwards

log()  { printf '[c7-moved] %s\n' "$*" >&2; }
fail() { printf '[c7-moved] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c7-moved] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

SERIAL="" UDID="" AND_UPPED=0 IOS_UPPED=0 WE_BOOTED=0 IOS_WE_BOOTED=0
cleanup() {
  local said
  if [ "$AND_UPPED" = 1 ]; then
    said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$AND_PORT" 2>&1)" \
      || printf '[c7-moved] warning: the Android runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
  fi
  if [ "$IOS_UPPED" = 1 ]; then
    said="$("$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" 2>&1)" \
      || printf '[c7-moved] warning: the iOS runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
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

# One flow, one verdict. $1 platform, $2 device, $3 port, $4 label,
# $5 the flow file, $6 expected ("pass" | "fail").
judge() {
  local label="$4" flow="$5" rc=0 out
  out="$(SMIX_RUNNER_PORT="$3" with_deadline 120 "$SMIX" run --device "$2" "$flow" 2>&1)" || rc=$?
  [ "$rc" = "$DEADLINE_STATUS" ] && cannot_judge "$1 $label: smix run did not answer within 120 s"
  printf '%s\n' "$out" > "$WORK/$1-$label.log"
  [ -n "$KEEP" ] && mkdir -p "$KEEP" && cp "$WORK/$1-$label.log" "$KEEP/"
  case "$6" in
    pass)
      if [ "$rc" = 0 ]; then log "  $1 $label: pass (as it should)"
      else printf '[c7-moved] FAIL: %s %s: expected to pass, exited %s — %s\n' "$1" "$label" "$rc" \
             "$(printf '%s' "$out" | tail -4 | tr '\n' ' ')" >&2; FAILED=1; fi ;;
    fail)
      # Both boxes printed, AND the change it reports is the 8 points the
      # fixture moves the anchor by. Without the second half an iOS run
      # passed on a 4-point move: the fixture's stack was centred, and a
      # verdict that only asked "did it fail" could not see it.
      local largest
      largest="$(printf '%s' "$out" | grep -o 'the largest change is [0-9.]*' | head -1 | awk '{print $NF}')"
      if [ "$rc" != 0 ] && printf '%s' "$out" | grep -q 'was x=' && printf '%s' "$out" | grep -q 'now x=' \
         && python3 -c "import sys; sys.exit(0 if abs(float('${largest:-nan}') - 8.0) <= 0.5 else 1)" 2>/dev/null; then
        log "  $1 $label: failed with both boxes, moved $largest (as it should): $(printf '%s' "$out" | grep -o 'was x=[^;]*' | head -1)"
      else printf '[c7-moved] FAIL: %s %s: expected to fail, print both boxes and report a move of 8 (reported %s), exited %s — %s\n' "$1" "$label" "${largest:-nothing}" "$rc" \
             "$(printf '%s' "$out" | tail -4 | tr '\n' ' ')" >&2; FAILED=1; fi ;;
  esac
}


# Write one flow: open the screen, remember the anchor, press, assert.
# $1 file, $2 appId, $3 platform (the opening differs), $4 target id,
# $5 the button to press, $6 `within` (empty for exact).
write_flow() {
  {
    printf 'appId: %s\n---\n' "$2"
    if [ "$3" = ios ]; then
      cat <<'OPEN'
- stopApp
- launchApp
- scrollUntilVisible:
    element:
      id: fixture-detail-link
    direction: DOWN
- tapOn:
    id: fixture-detail-link
- assertVisible:
    id: fixture-detail
OPEN
    fi
    printf -- '- rememberBounds:\n    id: %s\n    as: anchor\n' "$4"
    printf -- '- tapOn:\n    id: %s\n' "$5"
    printf -- '- assertBoundsUnchanged:\n    id: %s\n    was: anchor\n' "$4"
    [ -n "$6" ] && printf -- '    within: %s\n' "$6"
    true
  } > "$1"
}

# ---- Android ----------------------------------------------------------
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

and_fresh() { # a fresh BoundsActivity: -S stops the app first, so the anchor is back up
  with_deadline 60 adb -s "$SERIAL" shell am start -S -W -n "$AND_APPID/.BoundsActivity" >/dev/null 2>&1 \
    || fail "could not start the bounds screen on $SERIAL"
  sleep 2
}

log "--- Android ($SERIAL)"
write_flow "$WORK/a-stay.yaml" "$AND_APPID" android bounds_target bounds_stay ""
write_flow "$WORK/a-move.yaml" "$AND_APPID" android bounds_target bounds_move ""
write_flow "$WORK/a-move8.yaml" "$AND_APPID" android bounds_target bounds_move 8
and_fresh; judge android "$SERIAL" "$AND_PORT" stay "$WORK/a-stay.yaml" pass
and_fresh; judge android "$SERIAL" "$AND_PORT" move "$WORK/a-move.yaml" fail
and_fresh; judge android "$SERIAL" "$AND_PORT" move-within-8 "$WORK/a-move8.yaml" pass

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
write_flow "$WORK/i-stay.yaml" "$IOS_APPID" ios fixture-bounds-target fixture-bounds-stay ""
write_flow "$WORK/i-move.yaml" "$IOS_APPID" ios fixture-bounds-target fixture-bounds-move ""
write_flow "$WORK/i-move8.yaml" "$IOS_APPID" ios fixture-bounds-target fixture-bounds-move 8
judge ios "$UDID" "$IOS_PORT" stay "$WORK/i-stay.yaml" pass
judge ios "$UDID" "$IOS_PORT" move "$WORK/i-move.yaml" fail
judge ios "$UDID" "$IOS_PORT" move-within-8 "$WORK/i-move8.yaml" pass

[ "$FAILED" = 0 ] || exit 1
log "C7-MOVED-E2E-PASS (a control that moved 8 device-independent pixels failed with both boxes on both platforms; one that stayed, and the same move at within: 8, passed)"
