#!/usr/bin/env bash
# A hideKeyboard sent while the keyboard is still on its way in closes it.
#
# On a simulator with com.apple.keyboard.preferences
# AutomaticMinimizationEnabled on, the keyboard sits still below the screen
# for about a second after typing and then appears on it in one step. 12.0.0
# took one look, saw it below the screen, answered "minimized" without
# trying anything, and the keyboard arrived and stayed up — a consumer's
# flows went red three runs of three, and so did this script against that
# runner, five of five.
#
# On the second smix simulator (sim-smix-03), with the fixture app:
#   * premise: right after typing, the keyboard is below the screen; when
#     it is not, there is nothing on its way in to judge
#   * three times: tap, type, hideKeyboard, then the keyboard read from the
#     tree two and a half seconds later is not on the screen
# The key is written by this script to create the subject and removed on
# every exit; smix itself never writes it.
#
# Exit: 0 judged and passed, 1 judged and failed, 2 could not judge.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
source "$ROOT/scripts/lib/e2e-devices.sh"
UDID="${SMIX_C1_IOS:-$E2E_IOS_SECOND}"
APPID="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
PROJECT="$ROOT/swift-bridge/SmixRunner.xcodeproj"
WORK="$(mktemp -d)"
DOMAIN="com.apple.keyboard.preferences"
KEY="AutomaticMinimizationEnabled"

log()  { printf '[c1-way-in] %s\n' "$*" >&2; }
fail() { printf '[c1-way-in] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c1-way-in] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

UPPED=0 WE_BOOTED=0
cleanup() {
  xcrun simctl spawn "$UDID" defaults delete "$DOMAIN" "$KEY" >/dev/null 2>&1 || true
  if [ "$UPPED" = 1 ]; then
    said="$("$SMIX" runner down --device "$UDID" --runner-port "$PORT" 2>&1)" \
      || printf '[c1-way-in] warning: the runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -d "$FIXTURE" ] || cannot_judge "no iOS fixture — run: bash scripts/dev/build-fixture-app.sh"
if [ "$(simulator_state "$UDID")" != Booted ]; then
  "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || cannot_judge "could not boot $UDID"
  WE_BOOTED=1
fi
"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 \
  || cannot_judge "could not install the fixture: $(tail -3 "$WORK/install.log" | tr '\n' ' ')"
xcrun simctl spawn "$UDID" defaults write "$DOMAIN" "$KEY" -bool true \
  || cannot_judge "could not write $KEY"
[ "$(xcrun simctl spawn "$UDID" defaults read "$DOMAIN" "$KEY" 2>/dev/null)" = "1" ] \
  || cannot_judge "$KEY did not read back as 1"
SMIX_RUNNER_PORT="$PORT" "$SMIX" runner up "$UDID" --bundle "$APPID" --runner-port "$PORT" \
  --runner-project "$PROJECT" >"$WORK/up.log" 2>&1 \
  || cannot_judge "the runner would not start: $(tail -5 "$WORK/up.log" | tr '\n' ' ')"
UPPED=1

# The keyboard's top edge from the tree, or "none". Read from the runner's
# own tree rather than a `find`, which only counts what is on the screen
# and so reads a keyboard below it as absent.
keyboard_y() {
  curl -s -m 30 -H "App-Bundle-Id: $APPID" "http://127.0.0.1:$PORT/tree" | python3 -c '
import json, sys
found = []
def walk(n):
    if n.get("rawType") == "keyboard":
        found.append(int(n["bounds"]["y"]))
    for c in n.get("children", []):
        walk(c)
walk(json.load(sys.stdin))
print(found[0] if found else "none")'
}
screen_h() {
  curl -s -m 30 -H "App-Bundle-Id: $APPID" "http://127.0.0.1:$PORT/tree" \
    | python3 -c 'import json,sys; b=json.load(sys.stdin)["bounds"]; print(int(b["y"] + b["h"]))'
}

cat > "$WORK/type.yaml" <<YAML
appId: $APPID
---
- launchApp
- tapOn: { dispatch: 'xcui', id: 'fixture-input' }
- inputText: { text: 'x', id: 'fixture-input' }
YAML
cat > "$WORK/type-and-hide.yaml" <<YAML
appId: $APPID
---
- launchApp
- tapOn: { dispatch: 'xcui', id: 'fixture-input' }
- inputText: { text: 'x', id: 'fixture-input' }
- hideKeyboard
YAML

# The premise: a keyboard below the screen right after typing.
SMIX_RUNNER_PORT="$PORT" "$SMIX_RUN" --device "$UDID" --platform ios "$WORK/type.yaml" >"$WORK/premise.log" 2>&1 \
  || cannot_judge "the typing flow failed: $(tail -3 "$WORK/premise.log" | tr '\n' ' ')"
H="$(screen_h)"
y0="$(keyboard_y)"
case "$y0" in
  none) cannot_judge "no keyboard in the tree right after typing, so nothing is on its way in" ;;
esac
[ "$y0" -ge "$H" ] || cannot_judge "right after typing the keyboard was already on the screen (y=$y0 of $H), so nothing is on its way in"
log "premise: right after typing the keyboard sat at y=$y0, below a screen $H tall"

for i in 1 2 3; do
  SMIX_RUNNER_PORT="$PORT" "$SMIX_RUN" --device "$UDID" --platform ios "$WORK/type-and-hide.yaml" >"$WORK/run-$i.log" 2>&1 \
    || { tail -8 "$WORK/run-$i.log" >&2; fail "run $i: the flow failed"; }
  sleep 2.5
  y="$(keyboard_y)"
  if [ "$y" != none ] && [ "$y" -lt "$H" ]; then
    fail "run $i: hideKeyboard answered ok and 2.5 s later the keyboard is on the screen at y=$y"
  fi
  log "run $i: hideKeyboard ok, and 2.5 s later the keyboard is ${y/none/gone}"
done

log "PASS"
