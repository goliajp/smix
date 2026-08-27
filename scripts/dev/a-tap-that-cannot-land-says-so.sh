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
UDID="${1:?usage: a-tap-that-cannot-land-says-so.sh <udid> <port>}"
PORT="${2:?}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
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
"$SMIX" runner up "$UDID" --runner-port "$PORT" --bundle "$APP" --force >/dev/null 2>&1 \
  || fail "the runner would not come up on $UDID:$PORT"
sleep 2

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
OUT="$("$SMIX" tap 'id:fixture-submit' --device "$UDID" --port "$PORT" 2>&1)"
RC=$?
[ "$RC" -eq 0 ] && fail "the tap behind the alert exited 0 — this is the defect, unfixed"
grep -qi 'cannot be touched' <<<"$OUT" \
  || fail "the refusal does not say why: $(head -3 <<<"$OUT" | tr '\n' ' ')"
grep -qi 'on top of it' <<<"$OUT" \
  || fail "the refusal does not say what to do about it"
MID="$(result)"
[ "$MID" = "$MARK" ] && fail "the app changed anyway — the tap was dispatched despite the refusal"

# Half two: with the modal gone the same tap must actually work. A rule
# that refused everything would satisfy half one on its own.
"$SMIX" tap 'id:fixture-alert-confirm' --device "$UDID" --port "$PORT" >/dev/null 2>&1 \
  || fail "could not dismiss the alert — its own button must stay tappable"
sleep 2
"$SMIX" tap 'id:fixture-submit' --device "$UDID" --port "$PORT" >/dev/null 2>&1 \
  || fail "the same tap was refused with nothing covering it"
sleep 1
AFTER="$(result)"
[ "$AFTER" = "$MARK" ] || fail "the tap was allowed but the app did not move (result=$AFTER)"

echo "a-tap-that-cannot-land-says-so: behind the alert the tap was refused by name and the app did not move; with the alert dismissed the same tap set the result to $MARK"
