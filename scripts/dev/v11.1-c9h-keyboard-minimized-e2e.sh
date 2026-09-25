#!/usr/bin/env bash
# A keyboard wait that cannot succeed says which device setting is why.
#
# A simulator whose com.apple.keyboard.preferences AutomaticMinimizationEnabled
# is on shows no software keyboard over a focused field (K1, measured
# 2026-09-25: the same flow red twice with it on, green at once after
# `defaults delete`). The failure said "timed out", which reads as the app.
#
# On the second smix simulator (sim-smix-03), with the fixture app:
#   * key on:  keyboard-comes-and-goes fails, and its output names the
#     setting and the command that turns it off.
#   * key off: the same flow passes — so the red above was the setting,
#     not the flow.
# The key is written by this script to create the subject and removed on
# every exit; smix itself never writes it.
#
# Exit: 0 judged and passed, 1 judged and failed, 2 could not judge.
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
if ! xcrun simctl list devices 2>/dev/null | grep -q "$UDID.*Booted"; then
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
