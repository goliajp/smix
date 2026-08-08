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
SERIAL="${SMIX_ANDROID_SERIAL:-emulator-5554}"
PORT="${SMIX_ANDROID_CLEAR_PORT:-22094}"
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
FIELD_ID="search_src_text"

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
adb -s "$SERIAL" shell am force-stop com.android.settings >/dev/null 2>&1 || true
adb -s "$SERIAL" shell am start -n com.android.settings/.Settings >/dev/null 2>&1
sleep 3
curl -sf --max-time 10 -X POST "$R/tap-by-id" -H 'Content-Type: application/json' \
  --data '{"id":"search_action_bar"}' >/dev/null || true
sleep 2
curl -sf --max-time 10 -X POST "$R/tap-by-id" -H 'Content-Type: application/json' \
  --data "{\"id\":\"$FIELD_ID\"}" >/dev/null || true
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
