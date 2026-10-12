#!/usr/bin/env bash
# v15.1-C8: text an app keeps from accessibility is waited for by pixels.
#
# A consumer's toast ("Screenshot saved") was in the app's log and in no
# tree smix read, on iOS 27. Its fade-out is scheduled with a delay the
# moment it appears, and `UIView.animate(withDuration:delay:)` sets the end
# alpha, 0, at once: the card is on screen and not accessible the whole
# time. The fixture's toast screen shows the card that way
# (`-toast-animated`) and without the animation. The animated one:
#   - a wait for its text fails, and the failure says why text can be
#     missing from every tree and names `ocrText:`
#   - a wait for it with `ocrText:` finds it
# and the card without the animation, for contrast, is found by its text.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
IOS_ALIAS="${SMIX_C8_IOS:-$E2E_IOS}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port IOS_PORT
APPID="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
PROJECT="${SMIX_C8_RUNNER_PROJECT:-$ROOT/swift-bridge/SmixRunner.xcodeproj}"
WORK="$(mktemp -d)"

log()  { printf '[v15.1-c8] %s\n' "$*" >&2; }
fail() { printf '[v15.1-c8] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v15.1-c8] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" WE_BOOTED=0 WE_UPPED=0
cleanup() {
  if [ "$WE_UPPED" = 1 ]; then
    if ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" 2>&1)"; then
      printf '[v15.1-c8] warning: the iOS runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
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
"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" runner up "$UDID" --bundle "$APPID" --runner-port "$IOS_PORT" \
  --runner-project "$PROJECT" >"$WORK/up.log" 2>&1 || fail "runner up: $(tail -5 "$WORK/up.log")"
WE_UPPED=1

run_flow() { # $1 name, $2 launch arguments, $3 the wait step. Prints the exit code.
  printf 'appId: %s\n---\n- launchApp:\n    arguments: %s\n- tapOn: { id: toast-show }\n%s\n' "$APPID" "$2" "$3" >"$WORK/$1.yaml"
  local rc=0
  SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX_RUN" --device "$UDID" "$WORK/$1.yaml" >"$WORK/$1.log" 2>&1 || rc=$?
  echo "$rc"
}
BY_TEXT="- extendedWaitUntil:
    visible: \"Screenshot saved\"
    timeout: 4000"
BY_PIXELS="- extendedWaitUntil:
    visible: { ocrText: \"Screenshot saved\" }
    timeout: 6000"

rc="$(run_flow plain '["-toast", "-toast-passthrough"]' "$BY_TEXT")"
[ "$rc" = 0 ] || { tail -12 "$WORK/plain.log" >&2; fail "the ordinary toast was not found by its text — the screen this measures did not happen"; }
log "an ordinary toast: found by its text"

rc="$(run_flow listed-text '["-toast", "-toast-animated", "-toast-passthrough"]' "$BY_TEXT")"
[ "$rc" != 0 ] || fail "the toast with its fade-out scheduled on arrival was found by its text — the case this measures did not happen"
grep -q "alpha 0" "$WORK/listed-text.log" && grep -q "ocrText" "$WORK/listed-text.log" \
  || { tail -15 "$WORK/listed-text.log" >&2; fail "the failed wait does not say why the text is in no tree, or does not name ocrText"; }
log "faded out in advance: the wait by text failed, and said why and what to use"

rc="$(run_flow listed-pixels '["-toast", "-toast-animated", "-toast-passthrough"]' "$BY_PIXELS")"
[ "$rc" = 0 ] || { tail -12 "$WORK/listed-pixels.log" >&2; fail "the toast with its fade-out scheduled was not found by ocrText"; }
log "faded out in advance: found by ocrText"
log "V15.1-C8-E2E-PASS"
