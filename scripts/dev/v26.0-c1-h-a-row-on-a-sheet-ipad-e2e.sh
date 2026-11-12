#!/usr/bin/env bash
# v26.0-C1: on an iPad, a tap takes the match a touch reaches.
#
# A consumer's device search comes up as a form sheet on an iPad: a card
# over the list it searches, with the keyboard up. Rows behind the card and
# the one result on it share an identifier. `tapOn: { id: device-row }`
# took the first match — a row behind the card — and the result was never
# picked. On the fixture's -sheet-same-ids screen on an iPad simulator, in
# full-screen mode:
#   1. the tree marks the rows behind the card as covered, shows the rows
#      past the bottom edge as not visible, and leaves the one on the card
#      alone — the list runs past the screen, under the keyboard
#   2. a flow's tapOn picks the row on the card: the card goes and the
#      screen reads "picked Lobby Elevator"
#   3. `smix tap id:device-row` does the same from the command line, and
#      what it says it landed inside leaves out the rows behind the card
#   4. `dispatch: xcui` presses the sheet's button over a same-id one behind
#   5. `smix hide-keyboard` puts the keyboard down with the keyboard's own
#      key, says so, and the sheet stays
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
BUNDLE="jp.golia.smix.fixture"
IPAD_ALIAS="${SMIX_E2E_IPAD:-sim-smix-ipad}"
WORK="$(mktemp -d)"

log()  { printf '[v26.0-c1-h] %s\n' "$*" >&2; }
fail() { printf '[v26.0-c1-h] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v26.0-c1-h] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" WE_BOOTED=0 UP=0
cleanup() {
  local said
  if [ "$UP" = 1 ] && ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$PORT" 2>&1)"; then
    log "warning: the runner on $PORT was not stopped: $(printf '%s' "$said" | tail -1)"
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

UDID="$("$SMIX" sim resolve "$IPAD_ALIAS" 2>/dev/null | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator registered as '$IPAD_ALIAS'"
e2e_yield_if_held "$UDID"
bash "$ROOT/scripts/dev/build-fixture-app.sh" >"$WORK/fixture.log" 2>&1 \
  || fail "fixture build: $(tail -5 "$WORK/fixture.log")"
if [ "$(simulator_state "$UDID")" != Booted ]; then
  "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "sim boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi
xcrun simctl install "$UDID" "$ROOT/test-fixtures/demo-app/build/SmixFixture.app" >"$WORK/install.log" 2>&1 \
  || fail "install: $(cat "$WORK/install.log")"
"$SMIX" runner up "$UDID" --runner-port "$PORT" --bundle "$BUNDLE" >"$WORK/up.log" 2>&1 \
  || fail "runner up: $(tail -5 "$WORK/up.log")"
UP=1

flow() { # <name> <steps> — exit status of the run
  printf 'appId: %s\n---\n- launchApp:\n    stopApp: true\n    arguments: ["-sheet-same-ids"]\n- tapOn: { id: "open-search" }\n- assertVisible: { id: "input-device-search" }\n%s\n' \
    "$BUNDLE" "$2" >"$WORK/$1.yaml"
  # raw run: judged by its exit together with what the app shows
  SMIX_RUNNER_PORT="$PORT" "$SMIX" run --device "$UDID" "$WORK/$1.yaml" >"$WORK/$1.log" 2>&1
}

flow open '- waitForAnimationToEnd' || fail "opening the search sheet: $(tail -3 "$WORK/open.log")"
width="$("$SMIX" tree --device "$UDID" --port "$PORT" --json 2>/dev/null | python3 -c 'import json,sys; print(int(json.load(sys.stdin)["root"]["bounds"]["w"]))')"
[ "$width" -ge 700 ] || cannot_judge "the app is ${width}pt wide — the iPad is in windowed mode, not full screen"
"$SMIX" tree --device "$UDID" --port "$PORT" >"$WORK/tree.txt" 2>&1 || fail "smix tree: $(tail -3 "$WORK/tree.txt")"
# Rows behind the card that the outline still shows as visible must be
# marked covered; the rest are past the bottom edge and shown as not visible.
behind="$(grep -c '✓ id="device-row" label="Lobby Entrance.*(covered — a touch does not reach it)' "$WORK/tree.txt" || true)"
rows="$(grep -c '✓ id="device-row" label="Lobby Entrance' "$WORK/tree.txt" || true)"
past="$(grep -c '· id="device-row" label="Lobby Entrance' "$WORK/tree.txt" || true)"
[ "$rows" -gt 0 ] && [ "$past" -gt 0 ] \
  || cannot_judge "the list behind the sheet does not run past the screen ($rows shown, $past past the edge)"
[ "$behind" = "$rows" ] || fail "only $behind of the $rows rows shown behind the sheet are marked covered"
grep 'id="device-row" label="Lobby Elevator, Main Lobby"' "$WORK/tree.txt" | grep -q covered \
  && fail "the row on the sheet is marked covered"
log "ok    the tree marks the $rows rows behind the sheet as covered, $past past the edge as not visible, and not the one on it"

flow picked '- tapOn: { id: "device-row" }
- assertNotVisible: { id: "input-device-search" }
- assertVisible: "picked Lobby Elevator"' || fail "tapOn did not pick the row on the sheet: $(grep -m1 -E 'error|FAIL' "$WORK/picked.log")"
log "ok    a flow's tapOn picks the row on the sheet"

flow cli '- waitForAnimationToEnd' || fail "reopening the search sheet: $(tail -3 "$WORK/cli.log")"
"$SMIX" tap id:device-row --device "$UDID" --port "$PORT" >"$WORK/tap.log" 2>&1 || fail "smix tap: $(head -2 "$WORK/tap.log")"
i=0
until "$SMIX" find "text:picked Lobby Elevator" --device "$UDID" --port "$PORT" 2>/dev/null | grep -q "exists=true"; do
  i=$((i + 1))
  [ "$i" -lt 10 ] || fail "smix tap did not pick the row on the sheet: $(head -2 "$WORK/tap.log")"
  sleep 0.5
done
aimed="$(grep -m1 'aimed inside:' "$WORK/tap.log" || true)"
case "$aimed" in *device-row*) ;; *) fail "smix tap did not say what it landed inside: $(head -3 "$WORK/tap.log")" ;; esac
case "$aimed" in *"Lobby Entrance"*) fail "what the tap landed inside lists rows behind the sheet: $aimed" ;; esac
log "ok    smix tap id:device-row picks the row on the sheet and lists only the sheet's elements: ${aimed#*: }"

# 4. xcui dispatch: a back button behind the card and a close button on it
# share an identifier; the runner's own lookup takes the one a touch reaches
flow xcui '- tapOn: { id: "sheet-back", dispatch: xcui }
- assertVisible: "closed the sheet"' || fail "dispatch: xcui did not press the button on the sheet: $(grep -m1 -E 'error|FAIL' "$WORK/xcui.log")"
log "ok    dispatch: xcui presses the button on the sheet, not the one behind it"

# 5. hideKeyboard on a sheet: the iPad keyboard's own key, not a touch
# outside the keyboard that lands outside the card and closes it
flow hide '- tapOn: { id: "input-device-search" }' || fail "focusing the field: $(tail -3 "$WORK/hide.log")"
"$SMIX" hide-keyboard --device "$UDID" --port "$PORT" >"$WORK/hk.txt" 2>&1 || fail "hide-keyboard: $(head -2 "$WORK/hk.txt")"
grep -q "dismissed by key:Hide keyboard" "$WORK/hk.txt" || fail "hide-keyboard does not say it used the keyboard's key: $(head -1 "$WORK/hk.txt")"
"$SMIX" find id:input-device-search --device "$UDID" --port "$PORT" 2>&1 | grep -q "exists=true" \
  || fail "the sheet went with the keyboard"
log "ok    hideKeyboard on the sheet uses the keyboard's own key, says so, and leaves the sheet"
log "PASS"
