#!/usr/bin/env bash
# v20.0-C2: a foldable simulator folds, turns, is photographed and is touched
# on the screen that is lit.
#
# An iPhone Duo has two panels and only one is lit: the outer one folded,
# the inner one unfolded. smix photographed the dark one, sent every touch
# to the main (outer) panel, could not fold the device at all, and took
# `setOrientation` for done while nothing turned.
#
# Judged on a Duo simulator, against the fixture's rotating screen (its
# target sits in the top-leading corner, so a touch landing mirrored or on
# the wrong panel misses it), for each posture:
#   1. `smix sim posture` returns and devicectl, asked separately, reads the
#      panel the posture lights as the lit one
#   2. `smix sim screenshot` gives a picture the size of the lit panel, and
#      not a black one
#   3. every facing the posture reaches: `setOrientation` passes and a tap on
#      the target arrives (the press counter goes up by one)
#   4. the facing the posture cannot reach is refused by name
#
# SMIX_C2_DUO names the Duo (default sim-smix-duo). Without one this cannot
# judge and says so (exit 2). It is a second smix simulator beside the
# tier's own, so it runs alone (see `SECOND_SIM` in e2e-lanes.py), and it
# is shut down afterwards when this script booted it.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
SECOND_SIM="${SMIX_C2_DUO:-sim-smix-duo}"
APP="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
PROJECT="$ROOT/swift-bridge/SmixRunner.xcodeproj"
WORK="$(mktemp -d)"

log()  { printf '[v20.0-c2] %s\n' "$*" >&2; }
fail() { printf '[v20.0-c2] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v20.0-c2] cannot judge: %s\n' "$*" >&2; exit 2; }

DEVICE="" UP=0 WE_BOOTED=0 FOUND_POSTURE=""
cleanup() {
  if [ "$UP" = 1 ]; then
    local said
    if ! said="$("$SMIX" runner down --device "$DEVICE" --runner-port "$PORT" 2>&1)"; then
      log "warning: the runner on $PORT was not stopped:"
      printf '%s\n' "$said" | tail -3 >&2
    fi
  fi
  if [ "$WE_BOOTED" = 1 ]; then
    "$SMIX" sim shutdown "$DEVICE" >/dev/null 2>&1 || true
  elif [ -n "$FOUND_POSTURE" ]; then
    "$SMIX" sim posture "$DEVICE" "$FOUND_POSTURE" >/dev/null 2>&1 || true
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

# lit <udid> → "<displayId> <primary 0|1> <width>x<height>" of the lit panel
lit() {
  xcrun devicectl device info displays --device "$1" --json-output - 2>/dev/null | python3 -c '
import json, sys
d = json.load(sys.stdin)["result"]["displays"]
on = [x for x in d if x.get("active")]
if len(on) != 1:
    print("none")
else:
    x = on[0]
    print(x["displayId"], int(x["primary"]), "%dx%d" % tuple(x["nativeSize"]))'
}

# distinct_bytes <png> → how many byte values its decompressed pixel data
# holds. A dark panel's picture holds 4; anything on screen gives hundreds.
# Not the file size: a light screen compresses to a few hundred KB too.
distinct_bytes() {
  python3 - "$1" <<'PY'
import struct, sys, zlib
data = open(sys.argv[1], "rb").read()
i, idat = 8, b""
while i < len(data):
    n, kind = struct.unpack(">I4s", data[i:i + 8])
    if kind == b"IDAT":
        idat += data[i + 8:i + 8 + n]
    i += 12 + n
print(len(set(zlib.decompress(idat))))
PY
}

presses() {
  "$SMIX" tree --device "$DEVICE" --runner-port "$PORT" 2>/dev/null \
    | sed -n 's/.*label="presses \([0-9]*\)".*/\1/p' | head -1
}

flow() {
  printf 'appId: %s\n---\n%s\n' "$APP" "$1" >"$WORK/step.yaml"
  "$SMIX_RUN" --device "$DEVICE" --runner-port "$PORT" "$WORK/step.yaml" >"$WORK/step.log" 2>&1
}

DEVICE="$("$SMIX" sim resolve "$SECOND_SIM" 2>/dev/null | tail -1 | tr -d '[:space:]')" || true
[ -n "$DEVICE" ] || cannot_judge "no simulator resolves '$SECOND_SIM' (create an iPhone Duo and register it under that name)"
case "$(simulator_name "$DEVICE")" in
  sim-smix-*) ;;
  *) cannot_judge "'$SECOND_SIM' resolves to $DEVICE, which is not one of smix's own simulators" ;;
esac
if [ "$(simulator_state "$DEVICE")" != Booted ]; then
  with_deadline 300 "$SMIX" sim boot "$DEVICE" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi
read -r _ primary _ <<<"$(lit "$DEVICE")"
[ "$primary" = 1 ] && FOUND_POSTURE=closed || FOUND_POSTURE=open

"$SMIX" sim install "$DEVICE" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
"$SMIX" runner up "$DEVICE" --bundle "$APP" --runner-port "$PORT" --runner-project "$PROJECT" \
  >"$WORK/up.log" 2>&1 || fail "runner up: $(tail -5 "$WORK/up.log")"
UP=1

judge_posture() {
  local posture="$1" want_primary="$2" unreachable="$3"
  shift 3
  log "$posture: 1. fold, read back separately"
  "$SMIX" sim posture "$DEVICE" "$posture" >"$WORK/posture.log" 2>&1 \
    || fail "sim posture $posture: $(tail -3 "$WORK/posture.log")"
  local id primary size
  read -r id primary size <<<"$(lit "$DEVICE")"
  [ "$primary" = "$want_primary" ] || fail "$posture: devicectl reads display $id (primary=$primary) lit"

  log "$posture: 2. a screenshot of the lit panel ($size)"
  "$SMIX" sim screenshot "$DEVICE" "$WORK/shot.png" >/dev/null 2>&1 || fail "$posture: sim screenshot failed"
  local w h kinds
  w="$(sips -g pixelWidth "$WORK/shot.png" | awk '/pixelWidth/ {print $2}')"
  h="$(sips -g pixelHeight "$WORK/shot.png" | awk '/pixelHeight/ {print $2}')"
  kinds="$(distinct_bytes "$WORK/shot.png")"
  [ "${w}x${h}" = "$size" ] || [ "${h}x${w}" = "$size" ] \
    || fail "$posture: the screenshot is ${w}x${h} and the lit panel is $size"
  [ "$kinds" -gt 16 ] || fail "$posture: the ${w}x${h} screenshot is a dark panel ($kinds byte values in its pixels)"

  log "$posture: 3. each reachable facing turns and the corner target is touched"
  # upright first: the picker turns the whole device, so a facing set in
  # the other posture carries over a fold, and the fixture's list is
  # short enough in landscape to leave its "Open rotating" off screen
  flow '- launchApp: { clearState: true }
- setOrientation: PORTRAIT
- tapOn: "Open rotating"
- extendedWaitUntil: { visible: { id: rotating-press }, timeout: 10000 }' \
    || fail "$posture: could not open the rotating screen: $(tail -5 "$WORK/step.log")"
  local facing before after
  for facing in "$@"; do
    flow "- setOrientation: $facing" || fail "$posture $facing: $(tail -5 "$WORK/step.log")"
    before="$(presses)"
    flow '- tapOn: { id: rotating-press }' || fail "$posture $facing: tap: $(tail -5 "$WORK/step.log")"
    after="$(presses)"
    [ -n "$before" ] && [ "$after" = "$((before + 1))" ] \
      || fail "$posture $facing: presses went from '$before' to '$after' — the touch did not arrive"
    log "  $facing: presses $before → $after"
  done

  log "$posture: 4. $unreachable is refused by name"
  # `smix run` itself, not the judged wrapper: this refusal is the verdict,
  # and the wrapper ends the script on a driver error as not one
  printf 'appId: %s\n---\n- setOrientation: %s\n' "$APP" "$unreachable" >"$WORK/step.yaml"
  # raw run: the refusal is what is judged, read from its message below
  if "$SMIX" run --device "$DEVICE" --runner-port "$PORT" "$WORK/step.yaml" >"$WORK/step.log" 2>&1; then
    fail "$posture: setOrientation $unreachable passed, and this posture has no such facing"
  fi
  grep -q "cannot face that way" "$WORK/step.log" \
    || fail "$posture $unreachable: refused without saying why: $(tail -5 "$WORK/step.log")"
  flow '- tapOn: { id: rotating-exit }' || true
}

judge_posture closed 1 PORTRAIT_UPSIDE_DOWN PORTRAIT LANDSCAPE_LEFT LANDSCAPE_RIGHT PORTRAIT
judge_posture open 0 LANDSCAPE_LEFT PORTRAIT LANDSCAPE_RIGHT PORTRAIT

log "V20.0-C2-FOLDABLE-PASS"
