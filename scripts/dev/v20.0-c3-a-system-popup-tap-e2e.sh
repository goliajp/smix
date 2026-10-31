#!/usr/bin/env bash
# v20.0-C3: a `tapOn` that names a button on a system popup presses it.
#
# A notification prompt belongs to SpringBoard, so the app's tree has no
# "Allow" in it and `tapOn: "Allow"` failed with ELEMENT_NOT_FOUND; a
# consumer answered it with two CLI calls between flow steps instead. And
# `setPermissions: { notifications: allow }`, the documented way round the
# prompt, fails on iOS 27 simulators with simctl's "Operation not
# permitted". Judged on the fixture, whose "notifications" label is the
# system's own reading of the app's authorization:
#   1. a fresh install reads "unasked" (so the answers below are this run's)
#   2. "Ask notifications" then `tapOn: "Allow"` → the app reads "allowed",
#      and the run says it pressed a button on SpringBoard's popup
#   3. after a reinstall, `tapOn: "Don't Allow"` (straight apostrophe, the
#      popup's is typographic) → the app reads "denied"
#   4. `setPermissions: { notifications: allow }` fails saying to answer the
#      prompt with tapOn, or passes and the app reads "allowed"
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
IOS_ALIAS="${SMIX_C3_IOS:-$E2E_IOS}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port IOS_PORT
APPID="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
PROJECT="${SMIX_C3_RUNNER_PROJECT:-$ROOT/swift-bridge/SmixRunner.xcodeproj}"
WORK="$(mktemp -d)"

log()  { printf '[v20.0-c3] %s\n' "$*" >&2; }
fail() { printf '[v20.0-c3] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v20.0-c3] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" WE_BOOTED=0 WE_UPPED=0
cleanup() {
  if [ "$WE_UPPED" = 1 ]; then
    if ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" 2>&1)"; then
      printf '[v20.0-c3] warning: the iOS runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
    fi
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v xcrun >/dev/null 2>&1 || cannot_judge "no xcrun"
UDID="$("$SMIX" sim resolve "$IOS_ALIAS" 2>/dev/null | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$IOS_ALIAS'"
[ -d "$FIXTURE" ] || fail "no iOS fixture at $FIXTURE (bash scripts/dev/build-fixture-app.sh)"

if [ "$(simulator_state "$UDID")" != Booted ]; then
  "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi

# A reinstall is what forgets the app's notification answer: simctl cannot
# reset it (that refusal is check 4).
fresh() {
  "$SMIX" sim uninstall "$UDID" "$APPID" >/dev/null 2>&1 || true
  "$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
}
fresh
SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" runner up "$UDID" --bundle "$APPID" --runner-port "$IOS_PORT" \
  --runner-project "$PROJECT" >"$WORK/up.log" 2>&1 || fail "runner up: $(tail -5 "$WORK/up.log")"
WE_UPPED=1

run_flow() {
  printf 'appId: %s\n---\n%s\n' "$APPID" "$1" >"$WORK/flow.yaml"
  # raw run: check 4 judges a refusal by its words, which the wrapper would end the script on
  "$SMIX" run --device "$UDID" --runner-port "$IOS_PORT" "$WORK/flow.yaml" >"$WORK/out" 2>&1
}

log "1+2. unasked, then Allow"
run_flow '- launchApp
- assertVisible: "notifications unasked"
- tapOn: "Ask notifications"
- tapOn: "Allow"
- assertVisible: "notifications allowed"' || fail "Allow: $(grep -E 'FAIL|error' "$WORK/out" | head -5)"
grep -q "pressed \"Allow\" on com.apple.springboard's popup" "$WORK/out" \
  || fail "Allow was pressed without the run saying which popup it answered: $(grep WARN "$WORK/out")"

log "3. after a reinstall, Don't Allow"
fresh
run_flow "- launchApp
- assertVisible: \"notifications unasked\"
- tapOn: \"Ask notifications\"
- tapOn: \"Don't Allow\"
- assertVisible: \"notifications denied\"" || fail "Don't Allow: $(grep -E 'FAIL|error' "$WORK/out" | head -5)"

log "4. setPermissions notifications"
fresh
if run_flow '- launchApp
- setPermissions:
    notifications: allow'; then
  run_flow '- launchApp
- assertVisible: "notifications allowed"' \
    || fail "setPermissions passed and the app does not read allowed: $(grep -E 'FAIL|error' "$WORK/out" | head -3)"
  log "  setPermissions granted notifications on this runtime"
else
  grep -q 'tapOn: "Allow"' "$WORK/out" \
    || fail "setPermissions notifications failed without saying what to do: $(grep -E 'FAIL|error' "$WORK/out" | head -5)"
  log "  refused, and says to answer the prompt with tapOn"
fi

log "V20.0-C3-SYSTEM-POPUP-TAP-PASS"
