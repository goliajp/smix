#!/usr/bin/env bash
# Asking for an app that is not on the device must be refused, and the
# runner must still be there afterwards.
#
# XCUITest's `.activate()` does not fail on a bundle that is not
# installed. It waits for an app that will never come to the front, and
# it waits on the main actor -- so every later request that needs the app
# waits with it, `/health` goes on answering `ok:true` about a runner
# that can no longer do anything, and eventually XCTest's watchdog kills
# the test.
#
# Measured on 2026-08-29: one flow in the release corpus named an Android
# package. `foreground` hung for the whole of XCTest's budget, the runner
# died, and the twenty-three flows after it reported `runner
# unreachable` -- true, and about the wrong thing. Twenty-four red for
# one question that should have been answered before it was asked.
#
# The runner cannot defend itself once `.activate()` is called, so the
# host asks simctl first. Both halves are checked: the refusal has to
# name the condition, AND the runner has to survive it. Refusing
# everything satisfies the first alone; the second is the one that was
# actually broken.
set -uo pipefail
UDID="${1:?usage: a-foreground-that-cannot-happen-says-so.sh <udid> [port]}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=/dev/null
. "$ROOT/scripts/lib/gate-port.sh"
PORT="${2:-$SMIX_RUNNER_PORT}"
SMIX="$ROOT/target/release/smix"
APP="jp.golia.smix.fixture"
# An identifier no simulator carries. Deliberately the Android fixture's
# package, because that is the mistake this gate exists for.
ABSENT="dev.smix.fixture"

fail() { echo "a-foreground-that-cannot-happen-says-so: FAIL"; echo "  - $*"; exit 1; }

if ! "$SMIX" runner up "$UDID" --runner-port "$PORT" --bundle "$APP" --force >/dev/null 2>&1; then
  sleep 5
  "$SMIX" runner up "$UDID" --runner-port "$PORT" --bundle "$APP" --force >/dev/null 2>&1 \
    || fail "the runner would not come up on $UDID:$PORT, twice"
fi
# A runner left behind is not a tidiness problem. The next gate that
# brings one up on this sim gets a second xcodebuild test session against
# the same device, the two terminate each other's runner app, and
# `Activate` hangs -- measured 2026-08-29: 23 of the 26 corpus flows red,
# every one of them saying `runner unreachable`, which is true and about
# the wrong thing.
#
# So teardown is checked, and it is loud. The first version of this in
# the sibling gate passed `--port`, which `runner down` does not take;
# the argument error went to /dev/null behind `|| true`, and a teardown
# that did nothing read exactly like one that worked.
teardown() {
  local rc=$?
  local out
  if ! out="$("$SMIX" runner down --device "$UDID" 2>&1)"; then
    echo "a-foreground-that-cannot-happen-says-so: FAIL"
    echo "  - the runner was not taken down; it stays on $UDID and the next gate"
    echo "    to bring one up there will fight it. What down said:"
    printf '%s\n' "$out" | tail -3 | sed 's/^/      /'
    exit 1
  fi
  exit $rc
}
trap teardown EXIT

FLOW="$(mktemp -t smix-absent-app-XXXXXX).yaml"
printf 'appId: %s\n---\n- launchApp\n' "$ABSENT" > "$FLOW"
OUT="$("$SMIX" run --device "$UDID" --port "$PORT" "$FLOW" 2>&1)"
rm -f "$FLOW"

echo "$OUT" | grep -q "$ABSENT" \
  || fail "the refusal does not name the bundle that is missing:
  $(echo "$OUT" | grep -v '^kevy:' | tail -3)"
echo "$OUT" | grep -qi "not installed" \
  || fail "the refusal does not say the app is not installed -- an author
  reading it cannot tell this from any other failure:
  $(echo "$OUT" | grep -v '^kevy:' | tail -3)"
echo "  refused, and named the condition"

# The half that was broken. `/tree` needs the main actor; a wedged runner
# answers `/health` and nothing else, so health is not the witness here.
"$SMIX" tree --device "$UDID" --port "$PORT" --json >/dev/null 2>&1 \
  || fail "the runner no longer answers /tree -- the refusal did not
  prevent the wedge, and every later step will fail about the wrong thing"
echo "  the runner is still drivable afterwards"

echo "a-foreground-that-cannot-happen-says-so: PASS"
