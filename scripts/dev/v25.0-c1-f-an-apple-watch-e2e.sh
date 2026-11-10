#!/usr/bin/env bash
# v25.0-C1: the runner on an Apple Watch.
#
# The runner built for iOS and visionOS only; a watch simulator was turned
# away by xcodebuild. It now has a watchOS target. On a real watch
# simulator, the watch fixture (a button that counts, thirty rows):
#   1. runner up; the screen reads, with the button and its count
#   2. a tap on the button presses it
#   3. rotateCrown brings the last row in; a scroll back up, then a tap,
#      presses the button again — the scroll stops only once the row is
#      clear of the clock, where a tap reaches it
#   4. what a watch does not have is refused by name: a key other than
#      home, and text recognition
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
APP="jp.golia.smix.watchfixture"
WORK="$(mktemp -d)"

log()  { printf '[v25.0-c1-f] %s\n' "$*" >&2; }
fail() { printf '[v25.0-c1-f] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v25.0-c1-f] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" UP=0 WE_BOOTED=0
cleanup() {
  local said
  if [ "$UP" = 1 ] && ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$PORT" 2>&1)"; then
    log "warning: the runner on $PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

# $1 id → the element's name, empty when absent
name_of() {
  "$SMIX" describe --json --device "$UDID" --port "$PORT" 2>/dev/null | python3 -c "
import json, sys
for e in json.load(sys.stdin)['elements']:
    if e.get('id') == sys.argv[1]:
        print(e.get('name') or '')
" "$1"
}
# $1 id, $2 wanted name: a watch redraws about a second after a press
wait_name() {
  for _ in $(seq 1 12); do
    [ "$(name_of "$1")" = "$2" ] && return 0
    sleep 0.25
  done
  return 1
}

UDID="$("$SMIX" sim resolve "$E2E_WATCH" 2>/dev/null | tail -1 | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$E2E_WATCH'"
case "$(simulator_name "$UDID")" in
  sim-smix-*) ;;
  *) cannot_judge "'$E2E_WATCH' resolves to $UDID, which is not one of smix's own simulators" ;;
esac
bash "$ROOT/scripts/dev/build-device-fixture.sh" watch "$WORK" >"$WORK/build.log" 2>&1 \
  || fail "the watch fixture did not build: $(tail -3 "$WORK/build.log")"
if [ "$(simulator_state "$UDID")" != Booted ]; then
  with_deadline 300 "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi
"$SMIX" sim install "$UDID" "$WORK/SmixWatchFixture.app" >"$WORK/install.log" 2>&1 \
  || fail "install: $(tail -3 "$WORK/install.log")"
xcrun simctl terminate "$UDID" "$APP" >/dev/null 2>&1 || true
"$SMIX" sim launch "$UDID" "$APP" >/dev/null || fail "could not launch the fixture"

log "1. runner up, and the screen reads"
with_deadline 900 "$SMIX" runner up "$UDID" --bundle "$APP" --runner-port "$PORT" \
  --runner-project "$ROOT/swift-bridge/SmixRunner.xcodeproj" >"$WORK/up.log" 2>&1 \
  || fail "runner up: $(tail -5 "$WORK/up.log")"
UP=1
wait_name watch-count "pressed 0" || fail "1: the count reads '$(name_of watch-count)', not 'pressed 0'"
[ "$(name_of watch-press)" = "Press" ] || fail "1: the button reads '$(name_of watch-press)'"

log "2. a tap presses the button"
"$SMIX" tap --id watch-press --device "$UDID" --port "$PORT" >"$WORK/tap.log" 2>&1 \
  || fail "2: $(tail -3 "$WORK/tap.log")"
wait_name watch-count "pressed 1" || fail "2: after a tap the count reads '$(name_of watch-count)'"

log "3. the crown down to the last row, a scroll back, a tap"
cat >"$WORK/crown.yaml" <<YAML
appId: $APP
---
- assertNotVisible: { id: watch-row-30 }
- rotateCrown: { rotations: 6, velocity: 3 }
- assertVisible: { id: watch-row-30 }
- scrollUntilVisible: { element: { id: watch-press }, direction: UP }
- tapOn: { id: watch-press }
- assertVisible: "pressed 2"
YAML
with_deadline 240 "$SMIX_RUN" --device "$UDID" --runner-port "$PORT" --no-launch "$WORK/crown.yaml" \
  >"$WORK/crown.log" 2>&1 || fail "3: $(grep -m1 -E 'FAIL|error' "$WORK/crown.log")"

log "4. a key a watch has no button for, and text recognition, are refused by name"
rc=0; "$SMIX" press-key return --device "$UDID" --port "$PORT" >"$WORK/key.log" 2>&1 || rc=$?
[ "$rc" != 0 ] || fail "4: pressKey return passed on a watch"
grep -q "an Apple Watch has no keyboard" "$WORK/key.log" || fail "4: the key refusal says: $(head -2 "$WORK/key.log")"
rc=0; "$SMIX" find --ocr-text Press --device "$UDID" --port "$PORT" >"$WORK/ocr.log" 2>&1 || rc=$?
[ "$rc" != 0 ] || fail "4: an OCR lookup answered on a watch: $(cat "$WORK/ocr.log")"
grep -q "watchOS has no text recognition" "$WORK/ocr.log" || fail "4: the OCR refusal says: $(head -2 "$WORK/ocr.log")"

log "PASS"
