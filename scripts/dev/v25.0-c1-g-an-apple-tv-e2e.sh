#!/usr/bin/env bash
# v25.0-C1: the runner on an Apple TV.
#
# The runner built for iOS and visionOS only; a TV simulator was turned
# away by xcodebuild. It now builds for tvOS, and every act goes through
# the remote: a television takes no touch. On a real TV simulator, the TV
# fixture (three buttons in a row, a label naming the last one selected):
#   1. runner up; the screen reads, and one button holds the focus
#   2. the remote's arrows move the focus
#   3. a tap on a button moves the focus onto it and selects it
#   4. return selects the focused button; a double tap selects twice
#   5. what a television does not have is refused by name: a key the remote
#      has no button for, and the Digital Crown
#
# A television runs alone on its machine: with anything else booted this
# cannot judge, rather than squeeze in beside it.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
APP="jp.golia.smix.tvfixture"
WORK="$(mktemp -d)"

log()  { printf '[v25.0-c1-g] %s\n' "$*" >&2; }
fail() { printf '[v25.0-c1-g] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v25.0-c1-g] cannot judge: %s\n' "$*" >&2; exit 2; }

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
# the id of the element holding the focus, empty when none does
focused() {
  "$SMIX" tree --json --device "$UDID" --port "$PORT" 2>/dev/null | python3 -c "
import json, sys
t = json.load(sys.stdin); t = t.get('root', t)
def walk(n):
    if n.get('hasFocus') and n.get('identifier'):
        return n['identifier']
    for c in n.get('children', []):
        r = walk(c)
        if r: return r
print(walk(t) or '')
"
}
# $1 id, $2 wanted name
wait_name() {
  for _ in $(seq 1 12); do
    [ "$(name_of "$1")" = "$2" ] && return 0
    sleep 0.25
  done
  return 1
}

UDID="$("$SMIX" sim resolve "$E2E_TV" 2>/dev/null | tail -1 | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$E2E_TV'"
case "$(simulator_name "$UDID")" in
  sim-smix-*) ;;
  *) cannot_judge "'$E2E_TV' resolves to $UDID, which is not one of smix's own simulators" ;;
esac
if [ "$(simulator_state "$UDID")" != Booted ]; then
  "$SMIX" sim budget --need tv >"$WORK/budget.log" 2>&1 \
    || cannot_judge "a television runs alone: $(grep -m1 'no room' "$WORK/budget.log")"
fi
bash "$ROOT/scripts/dev/build-device-fixture.sh" tv "$WORK" >"$WORK/build.log" 2>&1 \
  || fail "the TV fixture did not build: $(tail -3 "$WORK/build.log")"
if [ "$(simulator_state "$UDID")" != Booted ]; then
  with_deadline 300 "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi
"$SMIX" sim install "$UDID" "$WORK/SmixTVFixture.app" >"$WORK/install.log" 2>&1 \
  || fail "install: $(tail -3 "$WORK/install.log")"
xcrun simctl terminate "$UDID" "$APP" >/dev/null 2>&1 || true
"$SMIX" sim launch "$UDID" "$APP" >/dev/null || fail "could not launch the fixture"

log "1. runner up; the screen reads, and a button holds the focus"
with_deadline 900 "$SMIX" runner up "$UDID" --bundle "$APP" --runner-port "$PORT" \
  --runner-project "$ROOT/swift-bridge/SmixRunner.xcodeproj" >"$WORK/up.log" 2>&1 \
  || fail "runner up: $(tail -5 "$WORK/up.log")"
UP=1
wait_name tv-count "pressed none 0" || fail "1: the label reads '$(name_of tv-count)'"
start="$(focused)"
case "$start" in tv-left|tv-middle|tv-right) ;; *) fail "1: the focus is on '$start', not on a button" ;; esac

log "2. the arrows move the focus (from $start)"
"$SMIX" press-key arrowLeft --device "$UDID" --port "$PORT" >/dev/null 2>&1 || true
"$SMIX" press-key arrowLeft --device "$UDID" --port "$PORT" >/dev/null 2>&1 || true
sleep 0.5
[ "$(focused)" = tv-left ] || fail "2: after two lefts the focus is on '$(focused)'"
"$SMIX" press-key arrowRight --device "$UDID" --port "$PORT" >"$WORK/right.log" 2>&1 \
  || fail "2: $(tail -2 "$WORK/right.log")"
sleep 0.5
[ "$(focused)" = tv-middle ] || fail "2: after a right the focus is on '$(focused)', not tv-middle"

log "3. a tap moves the focus onto its target and selects it"
"$SMIX" tap --id tv-right --device "$UDID" --port "$PORT" >"$WORK/tap.log" 2>&1 \
  || fail "3: $(tail -3 "$WORK/tap.log")"
wait_name tv-count "pressed right 1" || fail "3: the label reads '$(name_of tv-count)'"

log "4. return selects the focused button; a double tap selects twice"
"$SMIX" press-key return --device "$UDID" --port "$PORT" >/dev/null 2>&1 || fail "4: return was refused"
wait_name tv-count "pressed right 2" || fail "4: after return the label reads '$(name_of tv-count)'"
printf 'appId: %s\n---\n- doubleTapOn: { id: tv-left }\n- assertVisible: "pressed left 4"\n' "$APP" >"$WORK/double.yaml"
with_deadline 120 "$SMIX_RUN" --device "$UDID" --runner-port "$PORT" --no-launch "$WORK/double.yaml" \
  >"$WORK/double.log" 2>&1 || fail "4: $(grep -m1 -E 'FAIL|error' "$WORK/double.log")"

log "5. a key the remote has no button for, and the crown, are refused by name"
rc=0; "$SMIX" press-key tab --device "$UDID" --port "$PORT" >"$WORK/key.log" 2>&1 || rc=$?
[ "$rc" != 0 ] || fail "5: pressKey tab passed on a television"
grep -q "an Apple TV takes its remote's buttons only" "$WORK/key.log" \
  || fail "5: the key refusal says: $(head -2 "$WORK/key.log")"
rc=0; "$SMIX" rotate-crown 1 --device "$UDID" --port "$PORT" >"$WORK/crown.log" 2>&1 || rc=$?
[ "$rc" != 0 ] || fail "5: rotateCrown passed on a television"
grep -q "an Apple TV has no Digital Crown" "$WORK/crown.log" \
  || fail "5: the crown refusal says: $(head -2 "$WORK/crown.log")"

log "PASS"
