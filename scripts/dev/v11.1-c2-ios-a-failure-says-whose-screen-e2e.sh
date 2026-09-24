#!/usr/bin/env bash
# A failure says whose screen it happened on (iOS).
#
# The Android leg holds the consumer's symptom (the status bar filling the
# failure's first ten). iOS never had that — the status bar belongs to
# SpringBoard and is not in the app's tree — but it had the other two gaps:
# no line saying whose screen it was, and a list of ten with no word that
# it was a sample. Both platforms are judged by v11.1-c2-judge.py, so they
# answer the same sentence.
#
# Usage: v11.1-c2-ios-a-failure-says-whose-screen-e2e.sh <simulator udid>
#        (or SMIX_E2E_UDID)
# Exit 0 judged and passed, 1 judged and failed, 2 could not judge.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
UDID="${1:-${SMIX_E2E_UDID:-}}"
APP="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
JUDGE="$ROOT/scripts/dev/v11.1-c2-judge.py"
WORK="$(mktemp -d)"

log()  { printf '[c2-ios-whose-screen] %s\n' "$*" >&2; }
fail() { printf '[c2-ios-whose-screen] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c2-ios-whose-screen] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

[ -n "$UDID" ] || cannot_judge "usage: $0 <simulator udid> (or SMIX_E2E_UDID)"
xcrun simctl list devices booted 2>/dev/null | grep -q "$UDID" \
  || cannot_judge "$UDID is not a booted simulator"
[ -d "$FIXTURE" ] || fail "no fixture app — run: bash scripts/dev/build-fixture-app.sh"

WE_UPPED=0
cleanup() {
  if [ "$WE_UPPED" = 1 ]; then
    local said
    if ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$PORT" 2>&1)"; then
      printf '[c2-ios-whose-screen] warning: the runner on %s:%s was not stopped:\n%s\n' \
        "$UDID" "$PORT" "$(printf '%s' "$said" | tail -3)" >&2
    fi
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

xcrun simctl install "$UDID" "$FIXTURE" || fail "could not install the fixture"
"$SMIX" runner up "$UDID" --bundle "$APP" --runner-port "$PORT" --force >/dev/null 2>&1 \
  || fail "the runner would not start on $UDID:$PORT"
WE_UPPED=1

flow="$WORK/ios.yaml"
printf 'appId: %s\n---\n- launchApp\n- assertVisible: { id: "c2_nothing_carries_this_id" }\n' "$APP" > "$flow"
rc=0
out="$(SMIX_RUNNER_PORT="$PORT" "$SMIX_RUN" --device "$UDID" "$flow" 2>&1)" || rc=$?
[ "$rc" != 0 ] || fail "a step that cannot pass passed"
printf '%s\n' "$out" | python3 "$JUDGE" ios "$APP" >&2 \
  || fail "the failure does not say whose screen it happened on"

log "C2-IOS-WHOSE-SCREEN-E2E-PASS on $UDID (a failure names the app and counts what it cut)"
