#!/usr/bin/env bash
# A keyboard wait that ran out on a simulator with the minimization
# setting on names that setting and the way to turn it off.
#
# On 2026-09-25 a simulator with com.apple.keyboard.preferences
# AutomaticMinimizationEnabled on showed no software keyboard over a
# focused field (the same flow red twice with it on, green at once after
# `defaults delete`). That was never reproduced: on iOS 27 a keyboard
# comes up with it on. So this script first proves the setting keeps the
# keyboard down on the simulator in front of it, and cannot judge when it
# does not.
#
# On the second smix simulator (sim-smix-03), with the fixture app:
#   * key on:  keyboard-comes-and-goes fails, and its output names the
#     setting and the command that turns it off.
#   * key off: the same flow passes — so the red above was the setting,
#     not the flow.
# The key is written by this script to create the subject and removed on
# every exit; smix itself never writes it.
#
# Exit: 0 judged and passed, 1 judged and failed, 2 could not judge —
# including a simulator on which the key no longer keeps the keyboard down.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
source "$ROOT/scripts/lib/e2e-devices.sh"
UDID="${SMIX_C9H_IOS:-$E2E_IOS_SECOND}"
APPID="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
PROJECT="$ROOT/swift-bridge/SmixRunner.xcodeproj"
FLOW="$ROOT/scripts/release/stress-corpus/keyboard-comes-and-goes.yaml"
WORK="$(mktemp -d)"
DOMAIN="com.apple.keyboard.preferences"
KEY="AutomaticMinimizationEnabled"

log()  { printf '[c9h-keyboard] %s\n' "$*" >&2; }
cannot_judge() { printf '[c9h-keyboard] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }
FAILED=0
fail() { printf '[c9h-keyboard] FAIL: %s\n' "$*" >&2; FAILED=1; }

UPPED=0 WE_BOOTED=0
cleanup() {
  xcrun simctl spawn "$UDID" defaults delete "$DOMAIN" "$KEY" >/dev/null 2>&1 || true
  if [ "$UPPED" = 1 ]; then
    said="$("$SMIX" runner down --device "$UDID" --runner-port "$PORT" 2>&1)" \
      || printf '[c9h-keyboard] warning: the runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -f "$FLOW" ] || cannot_judge "no flow at $FLOW"
[ -d "$FIXTURE" ] || cannot_judge "no iOS fixture — run: bash scripts/dev/build-fixture-app.sh"
if [ "$(simulator_state "$UDID")" != Booted ]; then
  "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || cannot_judge "could not boot $UDID"
  WE_BOOTED=1
fi
"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 \
  || cannot_judge "could not install the fixture: $(tail -3 "$WORK/install.log" | tr '\n' ' ')"
SMIX_RUNNER_PORT="$PORT" "$SMIX" runner up "$UDID" --bundle "$APPID" --runner-port "$PORT" \
  --runner-project "$PROJECT" >"$WORK/up.log" 2>&1 \
  || cannot_judge "the runner would not start: $(tail -5 "$WORK/up.log" | tr '\n' ' ')"
UPPED=1

run_flow() {
  # raw run: the "on" leg expects a failure and reads its wording; the "off" leg is the control
  SMIX_RUNNER_PORT="$PORT" "$SMIX" run "$FLOW" --device "$UDID" --platform ios \
    --runner-port "$PORT" 2>&1
}

# The subject, read back before it is relied on: a write that did not
# land would make the "on" leg a second "off" leg.
xcrun simctl spawn "$UDID" defaults write "$DOMAIN" "$KEY" -bool true \
  || cannot_judge "could not write $KEY"
[ "$(xcrun simctl spawn "$UDID" defaults read "$DOMAIN" "$KEY" 2>/dev/null)" = "1" ] \
  || cannot_judge "$KEY did not read back as 1"

# The setting read back is not the subject; a keyboard it keeps down is.
# On the iOS 26.5 and 27 simulators the key reads 1 and a focused field
# still shows the full software keyboard — after relaunching the app and
# after rebooting the simulator — so the "on" leg would be a second "off"
# leg, and its pass would be called a failure of smix. Ask the device
# first: focus the field and see whether a keyboard comes up.
cat > "$WORK/premise.yaml" <<YAML
appId: $APPID
---
- launchApp:
    appId: $APPID
    clearState: true
- tapOn: { id: "fixture-input" }
- extendedWaitUntil:
    visible: { role: "keyboard" }
    timeout: 5000
YAML
prc=0
# raw run: only a pass (the keyboard came up) or a TIMEOUT on the keyboard wait answers the premise; anything else cannot judge it
SMIX_RUNNER_PORT="$PORT" "$SMIX" run "$WORK/premise.yaml" --device "$UDID" --platform ios \
  --runner-port "$PORT" >"$WORK/premise.log" 2>&1 || prc=$?
if [ "$prc" = 0 ]; then
  runtime="iOS $(xcrun simctl spawn "$UDID" sw_vers -productVersion 2>/dev/null || echo unknown)"
  cannot_judge "with $KEY = 1 a focused field on $UDID ($runtime) still shows the software keyboard, so there is no minimized keyboard here to be named"
elif ! grep -q 'TIMEOUT' "$WORK/premise.log" || ! grep -q 'extendedWaitUntil' "$WORK/premise.log"; then
  cannot_judge "the premise flow failed (exit $prc) for a reason other than the keyboard wait running out: $(tail -3 "$WORK/premise.log" | tr '\n' ' ')"
fi
log "premise: with $KEY = 1 no keyboard came up over the focused field"

rc=0; out="$(run_flow)" || rc=$?
if [ "$rc" = 0 ]; then
  fail "on: the flow passed with the keyboard minimized — the subject is not what this script thinks"
elif ! printf '%s' "$out" | grep -q "$KEY"; then
  fail "on: the flow failed (exit $rc) and its output does not name $KEY"
  printf '%s\n' "$out" | tail -15 >&2
elif ! printf '%s' "$out" | grep -q "defaults delete $DOMAIN $KEY"; then
  fail "on: the setting is named but not the way to turn it off"
else
  log "on: failed (exit $rc), naming the setting and the way back"
fi

xcrun simctl spawn "$UDID" defaults delete "$DOMAIN" "$KEY" >/dev/null 2>&1 \
  || cannot_judge "could not delete $KEY"
rc=0; out="$(run_flow)" || rc=$?
if [ "$rc" != 0 ]; then
  fail "off: the same flow failed (exit $rc) — the red above was not only the setting"
  printf '%s\n' "$out" | tail -15 >&2
else
  log "off: passed"
fi

[ "$FAILED" = 0 ] || exit 1
log "PASS"
