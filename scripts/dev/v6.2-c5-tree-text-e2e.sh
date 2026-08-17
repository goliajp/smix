#!/usr/bin/env bash
# v6.2-C5: the human `tree` prints text in its own position.
#
# On Android the human-readable tree showed only id + label, so `SUBMIT`
# (which Android carries in `text`, with label empty) was invisible —
# present in --json, gone from the human view (the consumer's ⑤). iOS
# carries its semantics in label/value/title and leaves text empty, so
# the old id+label output was enough there and blind on Android. The fix
# prints text when non-empty; this pins it on a real device.
#
# By empty-predicate (.claude/rule/empty-predicate.md) the gate is
# two-sided: a node whose text has a value must show it in the text
# position (SIDE A), and a node with no text must not grow a `text=`
# ghost (SIDE B) — anchored on a node that is provably in the tree.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
ALIAS="${SMIX_C5_ANDROID:-smix-android}"
PORT="${SMIX_C5_PORT:-22088}"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"

log()  { printf '[c5] %s\n' "$*" >&2; }
step() { printf '[c5] --- %s\n' "$*" >&2; }
fail() { printf '[c5] FAIL: %s\n' "$*" >&2; exit 1; }
skip() { printf '[c5] SKIP: %s\n' "$*" >&2; exit 0; }

cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
command -v adb >/dev/null 2>&1 || skip "no adb — this needs the Android SDK"

SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | grep -v '^kevy:' | tr -d '[:space:]')" || true
[ -n "$SERIAL" ] || skip "no emulator registered as '$ALIAS'"
adb devices 2>/dev/null | grep -q "^$SERIAL[[:space:]]*device" || skip "device $SERIAL not attached"
[ -f "$APK" ] || skip "no Android fixture apk (scripts/dev/build-android-fixture.sh)"
curl -s "http://127.0.0.1:$PORT/health" 2>/dev/null | grep -q smix-android-runner \
  || skip "no Android runner on $PORT"
log "device $SERIAL, runner $PORT"

adb -s "$SERIAL" install -r "$APK" >"$WORK/install.log" 2>&1 || fail "fixture install failed: $(tail -2 "$WORK/install.log")"

launch_fresh() {
  adb -s "$SERIAL" shell am force-stop "$APPID" >/dev/null 2>&1 || true
  printf 'appId: %s\n---\n- launchApp\n' "$APPID" >"$WORK/launch.yaml"
  SMIX_RUNNER_PORT="$PORT" "$SMIX" run --device "$SERIAL" "$WORK/launch.yaml" >/dev/null 2>&1 \
    || fail "could not launch $APPID"
  # launchApp returns before the view hierarchy is laid out; wait for the
  # fixture's own node to appear rather than reading an empty early tree.
  for _ in $(seq 1 15); do
    [ "$(json_count fixture_submit)" -ge 1 ] && return 0
    sleep 1
  done
  fail "fixture_submit never appeared after launch — the app did not come up"
}

human_tree() {
  SMIX_RUNNER_PORT="$PORT" "$SMIX" tree --device "$SERIAL" 2>/dev/null | grep -v '^kevy:'
}
json_count() { # $1 = identifier string; prints count of occurrences in --json
  SMIX_RUNNER_PORT="$PORT" "$SMIX" tree --json --device "$SERIAL" 2>/dev/null \
    | grep -v '^kevy:' | grep -c "\"$1\"" || true
}
human_line() { # $1 = id; prints the human line carrying id="<id>"
  human_tree | grep -F "id=\"$1\"" || true
}

step "presence: fixture_submit and statusBarBackground must be in the --json tree"
launch_fresh
[ "$(json_count fixture_submit)" -ge 1 ] || fail "fixture_submit not in tree — the gate would be reading air"
[ "$(json_count statusBarBackground)" -ge 1 ] || fail "statusBarBackground not in tree — SIDE B anchor missing, its absence check would be vacuous"

step "SIDE A: text with a value shows in the text position, not folded into id"
SUBMIT_LINE="$(human_line fixture_submit)"
[ -n "$SUBMIT_LINE" ] || fail "fixture_submit has no human line"
printf '%s\n' "$SUBMIT_LINE" | grep -q 'text="SUBMIT"' || fail "human line missing text=\"SUBMIT\": $SUBMIT_LINE"
printf '%s\n' "$SUBMIT_LINE" | grep -q 'id="fixture_submit"' || fail "human line missing id=\"fixture_submit\": $SUBMIT_LINE"
printf '%s\n' "$SUBMIT_LINE" | grep -q 'id="SUBMIT"' && fail "text bled into the id position: $SUBMIT_LINE"
# text surfaces even where id and label are both empty (not scraped from them)
human_tree | grep -q 'text="smix fixture"' || fail "an id-less TextView's text (smix fixture) did not surface in the human tree"
log "SIDE A OK: $SUBMIT_LINE"

step "SIDE B: a node with no text must not grow a text= field"
BAR_LINE="$(human_line statusBarBackground)"
[ -n "$BAR_LINE" ] || fail "statusBarBackground has no human line"
printf '%s\n' "$BAR_LINE" | grep -q 'text=' && fail "empty text produced a ghost text= field: $BAR_LINE"
log "SIDE B OK: $BAR_LINE"

log "v6.2-C5 PASS: human tree prints text in its own position; empty text prints nothing"
