#!/usr/bin/env bash
# v16.0-C11: two fingers zoom a picture, drag it, and the button puts it back.
#
# A consumer reproducing a zoom bug in a video player needed a pinch from
# 1x to about 3x, a drag, and a tap on Reset Zoom; smix had only one-finger
# gestures. The fixture's zoom screen (iOS: a UIScrollView, Android: Compose
# detectTransformGestures) shows its scale and offset in `zoom-state`.
#
# Judged on each platform, by what the app says rather than by the runner's
# answer:
#   1. `pinch: { over: zoom-surface, from: 1, to: 3 }` leaves the scale
#      between 2.7 and 3.3. Without the fingers' short run-up to the start
#      it was 2.32 on iOS and 2.62 on Android: the recognizer counts from
#      where it begins, after the touch slop
#   2. a swipe afterwards moves the picture, and Reset Zoom puts it back to 1
#   3. a pinch with `pan:` zooms and moves the picture in the one gesture,
#      the way the drag asked (finger up: iOS offset grows, Android's falls)
#   4. `smix pinch` at a point where a finger would leave the screen is
#      refused, naming the finger, and the scale does not move
#
# SMIX_C11_PLATFORMS=ios or =android runs one leg.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
PLATFORMS="${SMIX_C11_PLATFORMS:-ios android}"
IOS_ALIAS="${SMIX_C11_IOS:-$E2E_IOS}"
ANDROID_ALIAS="${SMIX_C11_ANDROID:-$E2E_ANDROID}"
IOS_APP="jp.golia.smix.fixture"
ANDROID_APP="dev.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
PROJECT="$ROOT/swift-bridge/SmixRunner.xcodeproj"
WORK="$(mktemp -d)"

log()  { printf '[v16.0-c11] %s\n' "$*" >&2; }
fail() { printf '[v16.0-c11] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v16.0-c11] cannot judge: %s\n' "$*" >&2; exit 2; }

DEVICE="" PLATFORM="" APP="" UP=0 IOS_DEVICE="" ANDROID_DEVICE="" WE_BOOTED_IOS=0 WE_BOOTED_ANDROID=0
runner_down() {
  [ "$UP" = 1 ] || return 0
  local said
  if ! said="$("$SMIX" runner down --platform "$PLATFORM" --device "$DEVICE" --runner-port "$PORT" 2>&1)"; then
    log "warning: the $PLATFORM runner on $PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  UP=0
}
cleanup() {
  runner_down
  if [ "$WE_BOOTED_IOS" = 1 ]; then "$SMIX" sim shutdown "$IOS_DEVICE" >/dev/null 2>&1 || true; fi
  if [ "$WE_BOOTED_ANDROID" = 1 ]; then "$SMIX" sim shutdown "$ANDROID_DEVICE" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

state() { # → "scale x y"
  curl -s -m 60 -H "App-Bundle-Id: $APP" "http://127.0.0.1:$PORT/tree" \
    | grep -oE 'scale=[0-9.]+ x=-?[0-9]+ y=-?[0-9]+' | head -1 \
    | sed -E 's/scale=([0-9.]+) x=(-?[0-9]+) y=(-?[0-9]+)/\1 \2 \3/'
}

flow() { # $1 name, rest: steps
  local name="$1"; shift
  { printf 'appId: %s\n---\n' "$APP"; printf '%s\n' "$@"; } >"$WORK/$name.yaml"
  "$SMIX_RUN" --device "$DEVICE" --runner-port "$PORT" "$WORK/$name.yaml" >"$WORK/$name.log" 2>&1 \
    || { tail -15 "$WORK/$name.log" >&2; fail "$PLATFORM: flow '$name' failed"; }
}

between() { python3 -c 'import sys; v,a,b=map(float,sys.argv[1:]); sys.exit(0 if a<=v<=b else 1)' "$@"; }

open_zoom() {
  if [ "$PLATFORM" = ios ]; then
    flow open '- launchApp: { clearState: true, arguments: ["-zoom"] }' \
      '- extendedWaitUntil: { visible: { id: zoom-state }, timeout: 10000 }'
  else
    adb -s "$DEVICE" shell am start -W -S -n "$APP/.ZoomActivity" >/dev/null 2>&1 \
      || fail "android: could not start the zoom screen"
    flow open '- extendedWaitUntil: { visible: { id: zoom-state }, timeout: 10000 }'
  fi
  read -r s _ _ <<<"$(state)"
  [ "${s:-}" = "1.00" ] || cannot_judge "$PLATFORM: the zoom screen did not open at 1x (state: '$(state)')"
}

judge() {
  open_zoom
  log "$PLATFORM: 1. pinch 1x → 3x over the picture"
  flow zoom '- pinch: { over: { id: zoom-surface }, from: 1, to: 3, duration: 600 }'
  sleep 1
  read -r s x y <<<"$(state)"
  log "$PLATFORM: after the pinch: scale=$s x=$x y=$y"
  between "$s" 2.7 3.3 || fail "$PLATFORM: pinched 1 → 3 and the picture is at ${s}x"

  log "$PLATFORM: 2. swipe to drag, then Reset Zoom"
  flow drag '- swipe: { from: "50%,70%", to: "40%,55%" }'
  sleep 1
  read -r s2 x2 y2 <<<"$(state)"
  log "$PLATFORM: after the swipe: scale=$s2 x=$x2 y=$y2"
  [ "$x2 $y2" != "$x $y" ] || fail "$PLATFORM: the swipe after the pinch did not move the zoomed picture"
  flow reset '- tapOn: { id: zoom-reset }'
  sleep 1
  read -r s3 _ _ <<<"$(state)"
  [ "$s3" = "1.00" ] || fail "$PLATFORM: Reset Zoom left the picture at ${s3}x"

  log "$PLATFORM: 3. pinch with a drag in the same gesture"
  flow zoom-pan '- pinch: { over: { id: zoom-surface }, from: 1, to: 3, pan: "0%,-15%", panDuration: 400 }'
  sleep 1
  read -r s4 x4 y4 <<<"$(state)"
  log "$PLATFORM: after the pinch and drag: scale=$s4 x=$x4 y=$y4"
  between "$s4" 2.7 3.3 || fail "$PLATFORM: pinch with pan left the picture at ${s4}x"
  flow reset2 '- tapOn: { id: zoom-reset }'
  sleep 1
  open_zoom
  flow zoom-only '- pinch: { over: { id: zoom-surface }, from: 1, to: 3 }'
  sleep 1
  read -r _ x5 y5 <<<"$(state)"
  if [ "$PLATFORM" = ios ]; then
    [ "$y4" -gt "$y5" ] || fail "ios: fingers dragged up and the visible part did not move down the picture (with pan y=$y4, without y=$y5)"
  else
    [ "$y4" -lt "$y5" ] || fail "android: fingers dragged up and the picture did not move up (with pan y=$y4, without y=$y5)"
  fi
  flow reset3 '- tapOn: { id: zoom-reset }'
  sleep 1

  log "$PLATFORM: 4. a pinch whose finger would leave the screen"
  if said="$("$SMIX" pinch point:5%,50% --from-span 0.1 --to-span 0.6 \
      --device "$DEVICE" --runner-port "$PORT" 2>&1)"; then
    fail "$PLATFORM: a pinch with a finger off the screen was sent: $said"
  fi
  case "$said" in
    *'finger 1 leaves the screen after the pinch'*) : ;;
    *) fail "$PLATFORM: the refusal does not name the finger and the moment: $said" ;;
  esac
  sleep 1
  read -r s6 _ _ <<<"$(state)"
  [ "$s6" = "1.00" ] || fail "$PLATFORM: the refused pinch moved the picture to ${s6}x"
  echo "PINCH-PASS $PLATFORM"
}

ios_leg() {
  command -v xcrun >/dev/null 2>&1 || cannot_judge "no xcrun"
  DEVICE="$("$SMIX" sim resolve "$IOS_ALIAS" 2>/dev/null | tail -1 | tr -d '[:space:]')" || true
  [ -n "$DEVICE" ] || cannot_judge "no simulator resolves '$IOS_ALIAS'"
  [ -d "$FIXTURE" ] || fail "no iOS fixture (bash scripts/dev/build-fixture-app.sh)"
  PLATFORM=ios APP="$IOS_APP"
  if [ "$(simulator_state "$DEVICE")" != Booted ]; then
    with_deadline 300 "$SMIX" sim boot "$DEVICE" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
    IOS_DEVICE="$DEVICE"
    WE_BOOTED_IOS=1
  fi
  "$SMIX" sim install "$DEVICE" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
  "$SMIX" runner up "$DEVICE" --bundle "$IOS_APP" --runner-port "$PORT" --runner-project "$PROJECT" \
    >"$WORK/up.log" 2>&1 || fail "ios runner up: $(tail -5 "$WORK/up.log")"
  UP=1
  judge
  runner_down
}

android_leg() {
  command -v adb >/dev/null 2>&1 || cannot_judge "no adb"
  [ -f "$APK" ] || fail "no fixture apk (bash scripts/dev/build-android-fixture.sh)"
  python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
    || fail "the fixture apk on disk is not the one this tree builds"
  PLATFORM=android APP="$ANDROID_APP"
  if ! DEVICE="$("$SMIX" sim resolve "$ANDROID_ALIAS" 2>/dev/null | tail -1)" || [ -z "$DEVICE" ]; then
    with_deadline 300 "$SMIX" sim boot "$ANDROID_ALIAS" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
    DEVICE="$("$SMIX" sim resolve "$ANDROID_ALIAS" 2>/dev/null | tail -1)" || fail "$ANDROID_ALIAS was booted and does not resolve"
    ANDROID_DEVICE="$DEVICE"
    WE_BOOTED_ANDROID=1
  fi
  case "$DEVICE" in
    emulator-*) : ;;
    *) fail "$ANDROID_ALIAS resolves to '$DEVICE', which is not an emulator — refusing" ;;
  esac
  for _ in $(seq 1 60); do
    adb -s "$DEVICE" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
    sleep 2
  done
  adb -s "$DEVICE" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"
  "$SMIX" runner up "$DEVICE" --platform android --runner-port "$PORT" >"$WORK/up.log" 2>&1 \
    || fail "android runner up: $(tail -5 "$WORK/up.log")"
  UP=1
  judge
  runner_down
}

for p in $PLATFORMS; do
  case "$p" in
    ios) ios_leg ;;
    android) android_leg ;;
    *) fail "unknown platform '$p'" ;;
  esac
done
