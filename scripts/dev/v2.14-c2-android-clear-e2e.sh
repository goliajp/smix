#!/usr/bin/env bash
# v2.14-C2: the Android runner empties a field in one request, exactly.
#
# `fill` clears the field first now, and on Android that used to mean
# fifty `/press-key DELETE` posts from the host — fifty sequential round
# trips over the adb forward, on every fill. Worse than slow: fifty
# deletes do not empty a field holding more than fifty characters, so
# the new text landed after the remainder while the caller was told its
# value had been replaced.
#
# So `/clear-text` does it device-side. Two paths, and the difference
# matters enough that the response says which ran:
#   - `set-text`: the focused node's ACTION_SET_TEXT, exact at any length
#   - `key-events`: the fallback when no focused editable node answers,
#     which deletes a bounded number of characters and can leave a
#     longer field partly filled
#
# The length case is the one worth a device: it is what the old path got
# wrong, and no unit test can tell you whether a real EditText emptied.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
# A serial from the caller, or the ledger's answer — never a port
# somebody else's emulator may be sitting on.
SERIAL="${SMIX_ANDROID_SERIAL:-}"
if [ -z "$SERIAL" ]; then
  SERIAL="$(bash "$(cd "$(dirname "$0")/../.." && pwd)/scripts/dev/pick-dev-emulator.sh")" || exit 1
fi
# A port of this gate's own, so a bystander runner cannot turn it red.
. "$ROOT/scripts/lib/gate-port.sh"
PORT="${SMIX_ANDROID_CLEAR_PORT:-$SMIX_RUNNER_PORT}"
R="http://127.0.0.1:$PORT"
WORK="$(mktemp -d)"

log()  { printf '[c2-clear] %s\n' "$*" >&2; }
step() { printf '[c2-clear] --- %s\n' "$*" >&2; }
fail() { printf '[c2-clear] FAIL: %s\n' "$*" >&2; exit 1; }
skip() { printf '[c2-clear] SKIP: %s\n' "$*" >&2; exit 0; }

started=0
cleanup() {
  if [ "$started" = 1 ]; then
    if ! "$SMIX" runner down --platform android --device "$SERIAL" \
         --runner-port "$PORT" >"$WORK/down.log" 2>&1; then
      printf '[c2-clear] the runner was not stopped:\n' >&2
      tail -3 "$WORK/down.log" >&2
    fi
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
command -v adb >/dev/null 2>&1 || skip "no adb — this needs an Android emulator"
adb devices | grep -q "^${SERIAL}[[:space:]]*device$" \
  || skip "no emulator at $SERIAL — start one (emulator -avd sim-smix-android-01), or set SMIX_ANDROID_SERIAL"

# The field this drives. Settings' search box is on every image and is a
# plain EditText, which is what makes it a fair stand-in for an app's
# own field; the recorder e2e drives the same one.
# The fixture's field, not the system Settings search box.
#
# `search_src_text` is a resource id belonging to whatever version of
# Settings the emulator happens to run, and on this one it does not
# exist — so the tap missed, nothing took focus, and this gate reported
# a typing failure that was not one. A gate that names a system app's
# internals has taken that app's version as a contract; the same reason
# the iOS corpus grew a portable tier at C4a.
FIELD_ID="fixture_input"
# One field, so focusing and typing are the same element.
FOCUS_ID="${FOCUS_ID:-fixture_input}"

field_text() {
  curl -s --max-time 10 "$R/tree" | python3 -c '
import json, sys
def walk(n):
    if "'"$FIELD_ID"'" in (n.get("identifier") or ""):
        print(n.get("text") or "")
        raise SystemExit
    for c in n.get("children", []):
        walk(c)
walk(json.load(sys.stdin))
'
}

step "1. runner up"
"$SMIX" runner up "$SERIAL" --platform android --runner-port "$PORT" \
  > "$WORK/up.log" 2>&1 || { tail -20 "$WORK/up.log"; fail "runner did not come up"; }
started=1
log "runner answering on $PORT"

step "2. no focused field — the fallback path, and it says so"
adb -s "$SERIAL" shell input keyevent KEYCODE_HOME >/dev/null 2>&1
sleep 1
OUT="$(curl -s --max-time 10 -X POST -H 'content-type: application/json' -d '{}' "$R/clear-text")"
echo "$OUT" | grep -q '"method":"key-events"' \
  || fail "with nothing focused this must fall back and name the path: $OUT"
log "key-events, named"

step "3. a focused field, longer than the old fifty-delete bound"
adb -s "$SERIAL" shell am force-stop dev.smix.fixture >/dev/null 2>&1 || true
adb -s "$SERIAL" shell am start -n dev.smix.fixture/.MainActivity >/dev/null 2>&1
sleep 3
# A tap that found nothing fails here, naming the id.
#
# Both of these discarded their result with `|| true`. The second one
# missed — `search_src_text` does not exist in this Android version's
# Settings — so nothing took focus, 120 characters went nowhere, and the
# failure surfaced three steps later as "the field holds 0 characters".
# That message sent the investigation at the typing path, which was
# working. `saw_node` in the reply is what separated "the tap failed"
# from "the tap landed and did nothing".
tap_id() {
  local out
  out="$(curl -s --max-time 10 -X POST "$R/tap-by-id" \
    -H 'Content-Type: application/json' --data "{\"id\":\"$1\"}")"
  # `ok`, not `saw_node`.
  #
  # `saw_node` answers "did the a11y path resolve it", and `/tap-by-id`
  # has two paths: a `touch` reply is `ok:true` with `saw_node:false`
  # and the tap landed. Asserting on `saw_node` failed a tap that
  # worked — the first draft of this check read a path marker as a hit
  # marker. `ok` is the field that answers whether it was tapped.
  case "$out" in
    *'"ok":true'*) ;;
    *) fail "no node with id $1 on screen — the subject does not have it: $out" ;;
  esac
}
tap_id "$FOCUS_ID"
sleep 2
tap_id "$FIELD_ID"
sleep 2

LONG="$(python3 -c 'print("x" * 120)')"
curl -sf --max-time 15 -X POST "$R/input-text" -H 'Content-Type: application/json' \
  --data "{\"text\":\"$LONG\"}" >/dev/null || fail "could not type into $FIELD_ID"
sleep 2
BEFORE="$(field_text)"
[ "${#BEFORE}" -ge 100 ] \
  || fail "expected a field holding ~120 characters, got ${#BEFORE} — the typing did not land, so this proves nothing"
log "field holds ${#BEFORE} characters"

step "4. one request empties it"
OUT="$(curl -s --max-time 15 -X POST -H 'content-type: application/json' -d '{}' "$R/clear-text")"
echo "$OUT" | grep -q '"method":"set-text"' \
  || fail "a focused editable field must take the exact path: $OUT"
sleep 1
AFTER="$(field_text)"
# Emptied means the app shows its hint again, not the typed text. Assert
# on the absence of what was typed rather than on the hint's wording,
# which is a locale away from being a different string.
case "$AFTER" in
  *xxxxx*) fail "the field still holds typed text after clear: ${AFTER:0:40}…" ;;
esac
log "emptied at 120 characters, in one request"

printf 'v2.14-C2 ANDROID-CLEAR-E2E-PASS\n'
