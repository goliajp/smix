#!/usr/bin/env bash
# v24.0-C1 G5/G10: text a consumer could see and not reach on a profile
# screen.
#
# Their rows were views that speak as one element, the title and the
# value together ("Time Zone, JST"), so `text:` with the title matched
# nothing — and the failure listed the first ten elements, the row being
# further down, and offered near names only from those ten. OCR had missed
# a gray row and a red button. On a real simulator, on the fixture's
# `-profile` screen:
#   1. `text:Time Zone` fails, offers "Time Zone, JST" first with the
#      regex that matches the title, and names text from the whole screen
#   2. that regex reaches the row
#   3. `ocrText:` reaches the gray row and the red button, and `text:` the
#      red button
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
APP="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
WORK="$(mktemp -d)"

log()  { printf '[v24.0-c1-b] %s\n' "$*" >&2; }
fail() { printf '[v24.0-c1-b] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v24.0-c1-b] cannot judge: %s\n' "$*" >&2; exit 2; }

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

result() {
  "$SMIX" describe --json --device "$UDID" --port "$PORT" 2>/dev/null | python3 -c "
import json, sys
for e in json.load(sys.stdin)['elements']:
    if e.get('id') == 'profile_result':
        print(e.get('name') or '')
"
}

# $1 selector, $2 what the app should then say
reaches() {
  "$SMIX" tap "$1" --device "$UDID" --port "$PORT" >"$WORK/tap.log" 2>&1 \
    || fail "$1: $(head -3 "$WORK/tap.log")"
  sleep 0.5
  local got
  got="$(result)"
  [ "$got" = "$2" ] || fail "$1: the app says '$got', not '$2'"
}

UDID="$("$SMIX" sim resolve "$E2E_IOS" 2>/dev/null | tail -1 | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$E2E_IOS'"
case "$(simulator_name "$UDID")" in
  sim-smix-*) ;;
  *) cannot_judge "'$E2E_IOS' resolves to $UDID, which is not one of smix's own simulators" ;;
esac
if [ "$(simulator_state "$UDID")" != Booted ]; then
  with_deadline 300 "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi
"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
"$SMIX" runner up "$UDID" --bundle "$APP" --runner-port "$PORT" >"$WORK/up.log" 2>&1 \
  || fail "runner up: $(tail -5 "$WORK/up.log")"
UP=1
xcrun simctl terminate "$UDID" "$APP" >/dev/null 2>&1 || true
"$SMIX" sim launch "$UDID" "$APP" -- -profile >/dev/null || fail "could not launch the fixture"
for _ in $(seq 1 20); do
  [ "$(result)" = "nothing yet" ] && break
  sleep 0.5
done
[ "$(result)" = "nothing yet" ] || cannot_judge "the -profile screen never showed"

log "1. text with the title alone fails, and says what would match"
if "$SMIX" tap "text:Time Zone" --device "$UDID" --port "$PORT" >"$WORK/miss.log" 2>&1; then
  fail "1: text:Time Zone reached something on a screen whose row says 'Time Zone, JST'"
fi
first="$(grep -m1 -A1 '^  suggestions:' "$WORK/miss.log" | tail -1 || true)"
[ -n "$first" ] || fail "1: the failure offers no suggestion at all: $(head -2 "$WORK/miss.log")"
case "$first" in
  *'"Time Zone, JST"'*'regex: "^Time Zone\\b"'*) ;;
  *) fail "1: the first suggestion is not the row and its regex: $first" ;;
esac
grep -q '"About"' "$WORK/miss.log" || fail "1: the failure does not name text from the whole screen"

log "2. the regex it offers reaches the row"
printf 'appId: %s\n---\n- tapOn:\n    text: { regex: "^Time Zone\\\\b" }\n' "$APP" >"$WORK/regex.yaml"
with_deadline 120 "$SMIX_RUN" --device "$UDID" --runner-port "$PORT" --no-launch "$WORK/regex.yaml" \
  >"$WORK/regex.log" 2>&1 || fail "2: $(grep -m1 -E 'FAIL|error' "$WORK/regex.log")"
sleep 0.5
[ "$(result)" = "opened Time Zone" ] || fail "2: the app says '$(result)', not 'opened Time Zone'"

log "3. OCR reaches the gray row and the red button, and text the red button"
# each touch changes what the app says, so a miss cannot pass on the last one
reaches "ocrText:Sign in on Apple Watch" "opened Sign in on Apple Watch"
reaches "ocrText:Log out and clear all cache" "logged out"
reaches "text:Sign in on Apple Watch" "opened Sign in on Apple Watch"
reaches "text:Log out and clear all cache" "logged out"

log "PASS"
