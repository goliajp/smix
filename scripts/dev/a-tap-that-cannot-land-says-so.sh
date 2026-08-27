#!/usr/bin/env bash
# A tap at something a user cannot reach must fail, and say why.
#
# SwiftUI leaves what is behind a modal in the accessibility tree, and the
# presentation swallows touches aimed there. Measured before this gate
# existed: with an alert open, `smix tap id:fixture-submit` exited 0 and the
# app did not move; closing the alert made the same tap work. smix reported
# success for something nobody could have touched.
#
# Both halves are checked, because either alone can be satisfied by a rule
# that is simply wrong in one direction: refusing everything passes the
# first, allowing everything passes the second.
set -uo pipefail
UDID="${1:?usage: a-tap-that-cannot-land-says-so.sh <udid> [port]}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# A free port from the OS, not the default one.
#
# `runner up` defaults to 22087, and a gate that takes that default is red
# whenever anything else on the machine holds it — another checkout, a
# developer's session, a runner orphaned by a crash. It would fail at
# startup, before judging anything, and read as smix being broken rather
# than as two things wanting the same socket. `gate-port.sh` exports
# SMIX_RUNNER_PORT, which `--runner-port` reads through clap's env, so one
# export reaches startup, every command, and teardown alike.
. "$ROOT/scripts/lib/gate-port.sh"
PORT="${2:-$SMIX_RUNNER_PORT}"
SMIX="$ROOT/target/release/smix"
APP="jp.golia.smix.fixture"
fail() { echo "a-tap-that-cannot-land-says-so: FAIL"; echo "  - $*"; exit 1; }

result() {
  "$SMIX" tree --device "$UDID" --port "$PORT" --json 2>/dev/null \
    | grep -v '^kevy:' \
    | python3 -c '
import sys, json
d = json.load(sys.stdin); root = d.get("root", d)
def w(n):
    if n.get("identifier") == "fixture-result":
        print(n.get("label") or n.get("text") or "")
    for c in (n.get("children") or []): w(c)
w(root)'
}

"$SMIX" sim terminate "$UDID" "$APP" >/dev/null 2>&1
# One retry, because back-to-back runs of this gate stop and start the same
# simulator faster than XCUITest reattaches, and the first `runner up` then
# refuses. Retrying a startup is not the same as retrying a verdict: what is
# being waited for here is the harness, and every assertion below still has
# to hold on the first attempt.
if ! "$SMIX" runner up "$UDID" --runner-port "$PORT" --bundle "$APP" --force >/dev/null 2>&1; then
  sleep 5
  "$SMIX" runner up "$UDID" --runner-port "$PORT" --bundle "$APP" --force >/dev/null 2>&1 \
    || fail "the runner would not come up on $UDID:$PORT, twice"
fi
# And put the subject back on screen. `sim terminate` above closed it, and
# on a fixed port the previous runner had left it foregrounded — so this
# passed for the wrong reason until the port became one the OS picks. A
# gate that needs the last run to have tidied up is not a gate.
#
# Then wait for the SESSION, not for the runner. `runner up` returning 0
# means its server answers; it does not mean the app binding is drivable,
# and `/tree`答 unreachable for a while after. A consumer taught us that
# distinction — `/health` says 200 while `/tree` says 000 — and this gate
# was reading the first as if it were the second, failing one run in three
# on "the subject is not on screen" when the subject was simply not
# reachable yet.
"$SMIX" sim launch "$UDID" "$APP" >/dev/null 2>&1
READY=0
for _ in $(seq 1 30); do
  if "$SMIX" find 'id:fixture-input' --device "$UDID" --port "$PORT" 2>/dev/null \
      | grep -v '^kevy:' | grep -q 'exists=true'; then
    READY=1
    break
  fi
  sleep 1
done
[ "$READY" = 1 ] \
  || fail "$APP never became drivable on $PORT — the runner answered but its session did not"

MARK="gate-$$"
"$SMIX" fill 'id:fixture-input' --text "$MARK" --device "$UDID" --port "$PORT" >/dev/null 2>&1 \
  || fail "could not type into fixture-input — the subject is not on screen"

# The stimulus, proved before either verdict is read. Without a filled
# field the submit button changes nothing, and "blocked" and "worked"
# print the same thing.
BEFORE="$(result)"
[ "$BEFORE" = "$MARK" ] && fail "the result already reads the marker before anything was submitted"

"$SMIX" tap 'id:fixture-open-alert' --device "$UDID" --port "$PORT" >/dev/null 2>&1 \
  || fail "could not open the alert"
sleep 2

# Half one: refused, by name.
# `kevy:` AOF lines share stdout with the verdict, and grepping the lot for
# the refusal's words found them in a replay log instead. Every other gate
# here filters them; this one did not, and read a store's chatter as an
# answer about the screen.
OUT="$("$SMIX" tap 'id:fixture-submit' --device "$UDID" --port "$PORT" 2>&1 | grep -v '^kevy:')"
RC=$?
[ "$RC" -eq 0 ] && fail "the tap behind the alert exited 0 — this is the defect, unfixed"
grep -qi 'cannot be touched' <<<"$OUT" \
  || fail "the refusal does not say why: $(head -3 <<<"$OUT" | tr '\n' ' ')"
grep -qi 'on top of it' <<<"$OUT" \
  || fail "the refusal does not say what to do about it"
MID="$(result)"
[ "$MID" = "$MARK" ] && fail "the app changed anyway — the tap was dispatched despite the refusal"

# `find` keeps saying it exists — it does — and adds the second fact, so a
# reader learns it there rather than from a tap refused a moment later.
FOUND="$("$SMIX" find 'id:fixture-submit' --device "$UDID" --port "$PORT" 2>&1 | grep -v '^kevy:')"
grep -q 'exists=true' <<<"$FOUND" \
  || fail "find stopped reporting the element as existing — it is there, and \
saying otherwise is a different lie: $FOUND"
grep -q 'reachable=false' <<<"$FOUND" \
  || fail "find did not mention that it cannot be reached: $FOUND"
# And the modal's own control must NOT carry that line, or it means nothing.
INSIDE="$("$SMIX" find 'id:fixture-alert-confirm' --device "$UDID" --port "$PORT" 2>&1 | grep -v '^kevy:')"
grep -q 'reachable=false' <<<"$INSIDE" \
  && fail "the alert's own button was reported unreachable: $INSIDE"

# Half two: with the modal gone the same tap must actually work. A rule
# that refused everything would satisfy half one on its own.
"$SMIX" tap 'id:fixture-alert-confirm' --device "$UDID" --port "$PORT" >/dev/null 2>&1 \
  || fail "could not dismiss the alert — its own button must stay tappable"
sleep 2
"$SMIX" tap 'id:fixture-submit' --device "$UDID" --port "$PORT" >/dev/null 2>&1 \
  || fail "the same tap was refused with nothing covering it"
# Wait for the result to change rather than for a second to pass. A fixed
# sleep here read the label before SwiftUI had re-rendered it and reported
# "the tap was allowed but the app did not move" — which is what a real
# failure would also say, so the two were indistinguishable.
AFTER=""
for _ in $(seq 1 20); do
  AFTER="$(result)"
  [ "$AFTER" = "$MARK" ] && break
  sleep 0.5
done
[ "$AFTER" = "$MARK" ] || fail "the tap was allowed but the app did not move (result=$AFTER)"

echo "a-tap-that-cannot-land-says-so: behind the alert the tap was refused by name and the app did not move; with the alert dismissed the same tap set the result to $MARK"
# Explicit. Without it the script's status is whatever the last command
# happened to leave — and the retry loop above ends on a `[ ]` test whose
# result is the loop's exit condition, not the gate's verdict. It printed
# the success line and exited 1, which is the worst of both: a gate that
# passes and reports failure teaches everyone to ignore its exit code.
exit 0
