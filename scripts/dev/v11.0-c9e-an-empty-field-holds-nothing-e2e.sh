#!/usr/bin/env bash
# An empty field holds nothing, even while it shows its hint.
#
# Since API 26 an empty EditText reports its hint as the accessibility
# node's text and sets isShowingHintText. The runner read `node.text` as
# the field's content in four places, so an empty search field held the
# seven characters of "Search…": `/clear-text` answered
# `field_not_empty held:7` about a field with nothing in it, and the
# release gate's `--force-key-events` leg died on it (AB1, 2026-09-25).
#
# This asks the runner and the tree about the fixture's `type here`
# field, empty, filled, and cleared again:
#
#   * `/clear-text` on the empty field answers ok, holding 0;
#   * both trees — accessibility and the app's probe — carry the hint as
#     `placeholderValue`, and no `text`;
#   * `text: "type here"` still finds the field — the hint is what a
#     person reads there, and maestro's hintText and iOS both match it;
#   * after a fill the tree's `text` is what was typed, and after a
#     clear the hint is back as `placeholderValue` and `text` is gone.
#
# The verdict on "filled" is the app's own result label after Submit,
# not the field's value: a read of the field is the path under test.
#
# Exit: 0 judged and passed, 1 judged and failed, 2 could not judge.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/e2e-devices.sh"
ALIAS="${SMIX_C9E_ANDROID:-$E2E_ANDROID}"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"
HINT="type here"
TYPED="smixc9e"

log()  { printf '[c9e-empty] %s\n' "$*" >&2; }
cannot_judge() { printf '[c9e-empty] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }
FAILED=0
fail() { printf '[c9e-empty] FAIL: %s\n' "$*" >&2; FAILED=1; }

SERIAL="" UPPED=0 WE_BOOTED=0
cleanup() {
  local said
  if [ "$UPPED" = 1 ]; then
    said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" 2>&1)" \
      || printf '[c9e-empty] warning: the runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$ALIAS" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || cannot_judge "no adb on PATH"
[ -f "$APK" ] || cannot_judge "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || { printf '[c9e-empty] FAIL: the fixture apk on disk is not the one this tree builds\n' >&2; exit 1; }

if ! SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" \
   || [ -z "$SERIAL" ] || ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  log "booting $ALIAS"
  "$SMIX" sim boot "$ALIAS" >/dev/null 2>&1 || cannot_judge "could not boot $ALIAS"
  WE_BOOTED=1
  SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)"
fi
case "$SERIAL" in
  emulator-*) : ;;
  *) cannot_judge "$ALIAS resolves to '$SERIAL', which is not an emulator — refusing" ;;
esac
adb -s "$SERIAL" wait-for-device
for _ in $(seq 1 60); do
  adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done
adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || cannot_judge "could not install the fixture on $SERIAL"
"$SMIX" runner up "$SERIAL" --platform android --runner-port "$PORT" >"$WORK/up.log" 2>&1 \
  || cannot_judge "the runner would not start on $SERIAL:$PORT: $(tail -3 "$WORK/up.log" | tr '\n' ' ')"
UPPED=1
adb -s "$SERIAL" shell am start -S -W -n "$APPID/.MainActivity" >/dev/null 2>&1 \
  || cannot_judge "could not start the fixture on $SERIAL"
sleep 2

# The fixture's field, as one reader's tree describes it: its text and
# its placeholderValue, tab-separated, "-" for absent. $1 is the reader,
# a11y by default; the fixture carries the probe, so `probe` asks the
# other reader the same question.
field() {
  local tree
  tree="$("$SMIX" tree --device "$SERIAL" --port "$PORT" --reader "${1:-a11y}" --json 2>/dev/null)" \
    || { echo "unreadable"; return 0; }
  TREE_JSON="$tree" python3 - <<'PY'
import json, os
d = json.loads(os.environ["TREE_JSON"])
hits = []
def walk(n):
    if (n.get("identifier") or "").split("/")[-1] == "fixture_input":
        hits.append(n)
    for c in n.get("children", []):
        walk(c)
walk(d["root"])
if len(hits) != 1:
    print(f"found {len(hits)} fixture_input nodes")
else:
    n = hits[0]
    print(f"{n.get('text') or '-'}\t{n.get('placeholderValue') or '-'}")
PY
}

clear_text() {
  curl -s -m 20 -X POST -H 'content-type: application/json' -d '{}' \
    "http://127.0.0.1:$PORT/clear-text" || echo "no answer"
}

"$SMIX" tap --device "$SERIAL" --port "$PORT" id:fixture_input >"$WORK/tap.log" 2>&1 \
  || cannot_judge "could not focus the field: $(tail -3 "$WORK/tap.log" | tr '\n' ' ')"
sleep 1

# 1. Empty: clearing it holds nothing.
said="$(clear_text)"
log "empty  /clear-text → $said"
case "$said" in
  *'"ok":true'*'"held":0'*|*'"held":0'*'"ok":true'*) : ;;
  *) fail "/clear-text on the empty field did not answer ok holding 0: $said" ;;
esac

# 2. Empty: the hint is a placeholder, not content.
got="$(field)"
log "empty  tree → text/placeholderValue = $(printf '%s' "$got" | tr '\t' '/')"
[ "$got" = "-	$HINT" ] || fail "the empty field's tree node is '$got', want no text and placeholderValue '$HINT'"
got="$(field probe)"
log "empty  probe tree → text/placeholderValue = $(printf '%s' "$got" | tr '\t' '/')"
[ "$got" = "-	$HINT" ] || fail "the probe's tree node is '$got', want no text and placeholderValue '$HINT'"

# 3. The hint still finds the field.
out="$("$SMIX" find --device "$SERIAL" --port "$PORT" "text:$HINT" 2>&1)" || true
case "$out" in
  *"exists=true"*|*'"exists":true'*) log "text:\"$HINT\" finds the field" ;;
  *) fail "text:\"$HINT\" no longer finds the empty field: $(printf '%s' "$out" | tail -2 | tr '\n' ' ')" ;;
esac

# 4. Filled: the tree's text is what was typed, and the app received it.
"$SMIX" fill --device "$SERIAL" --port "$PORT" --text "$TYPED" id:fixture_input >"$WORK/fill.log" 2>&1 \
  || fail "fill into the empty field failed: $(tail -3 "$WORK/fill.log" | tr '\n' ' ')"
got="$(field)"
log "filled tree → text/placeholderValue = $(printf '%s' "$got" | tr '\t' '/')"
case "$got" in
  "$TYPED	"*) : ;;
  *) fail "after the fill the field's text is '$got', want '$TYPED'" ;;
esac
"$SMIX" tap --device "$SERIAL" --port "$PORT" id:fixture_submit >"$WORK/submit.log" 2>&1 \
  || fail "could not press Submit: $(tail -3 "$WORK/submit.log" | tr '\n' ' ')"
sleep 1
result="$("$SMIX" tree --device "$SERIAL" --port "$PORT" --reader a11y --json 2>/dev/null \
  | python3 -c "
import json,sys
d=json.load(sys.stdin)
def walk(n):
    if (n.get('identifier') or '').split('/')[-1]=='fixture_result':
        print(n.get('text') or ''); sys.exit(0)
    for c in n.get('children',[]): walk(c)
walk(d['root'])")" || result="unreadable"
log "the app received: $result"
[ "$result" = "$TYPED" ] || fail "the app received '$result', want '$TYPED'"

# 5. Cleared again: the hint is back as a placeholder and text is gone.
"$SMIX" tap --device "$SERIAL" --port "$PORT" id:fixture_input >/dev/null 2>&1 || true
sleep 1
said="$(clear_text)"
log "filled /clear-text → $said"
case "$said" in
  *'"ok":true'*'"held":0'*|*'"held":0'*'"ok":true'*) : ;;
  *) fail "/clear-text on the filled field did not answer ok holding 0: $said" ;;
esac
got="$(field)"
log "cleared tree → text/placeholderValue = $(printf '%s' "$got" | tr '\t' '/')"
[ "$got" = "-	$HINT" ] || fail "the cleared field's tree node is '$got', want no text and placeholderValue '$HINT'"

[ "$FAILED" = 0 ] || exit 1
log "C9E-EMPTY-FIELD-E2E-PASS (an empty field holds nothing and carries its hint as placeholderValue; the app's own label confirms the fill)"
