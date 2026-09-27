#!/usr/bin/env bash
# Typing into a field that submits itself leaves nothing in any other field.
#
# A consumer's reset-code field submits once full and takes itself off the
# screen. The runner read the gone node back as empty, took the text as a
# dropped tail, and typed it again three times — into whichever field held
# focus next. Measured on the fixture, 2026-09-27: the app got `4321`, the
# step failed `text_did_not_land`, and the address field above held
# `432143214321`, three runs out of three.
#
# The second half types with no field named, after a tap on a number-pad
# field. The runner types into the focused field; nothing is tapped on the
# way, so no key of the pad can be pressed for it.
#
# Witnesses beside smix's own answer: the app's result label (what the app
# received) and the address field's contents (what nobody asked for).
#
# Exit: 0 judged and passed, 1 judged and failed, 2 could not judge.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
# shellcheck source=../lib/e2e-devices.sh
source "$ROOT/scripts/lib/e2e-devices.sh"
ALIAS="${SMIX_C9M_ANDROID:-$E2E_ANDROID}"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"

log()  { printf '[c9m-submits-itself] %s\n' "$*" >&2; }
cannot_judge() { printf '[c9m-submits-itself] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }
fail() { printf '[c9m-submits-itself] FAIL: %s\n' "$*" >&2; exit 1; }

SERIAL="" UPPED=0 WE_BOOTED=0
cleanup() {
  if [ "$UPPED" = 1 ]; then
    said="$("$SMIX" runner down --device "$SERIAL" --platform android --runner-port "$PORT" 2>&1)" \
      || printf '[c9m-submits-itself] warning: the runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then
    said="$("$SMIX" sim shutdown "$ALIAS" 2>&1)" \
      || printf '[c9m-submits-itself] warning: %s was not shut down:\n%s\n' "$ALIAS" "$(printf '%s' "$said" | tail -3)" >&2
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -f "$APK" ] || cannot_judge "no Android fixture — run: bash scripts/dev/build-android-fixture.sh"
SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1 | tr -d '[:space:]')" || true
if [ -z "$SERIAL" ] || ! adb devices 2>/dev/null | grep -q "^$SERIAL[[:space:]]*device"; then
  "$SMIX" sim boot "$ALIAS" >"$WORK/boot.log" 2>&1 || cannot_judge "could not boot $ALIAS"
  WE_BOOTED=1
  SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1 | tr -d '[:space:]')"
fi
case "$SERIAL" in emulator-*) ;; *) cannot_judge "'$ALIAS' resolved to '$SERIAL', not an emulator" ;; esac
"$SMIX" sim install "$SERIAL" "$APK" >"$WORK/install.log" 2>&1 \
  || cannot_judge "could not install the fixture: $(tail -3 "$WORK/install.log" | tr '\n' ' ')"
"$SMIX" runner up "$SERIAL" --platform android --runner-port "$PORT" >"$WORK/up.log" 2>&1 \
  || cannot_judge "the runner would not start: $(tail -5 "$WORK/up.log" | tr '\n' ' ')"
UPPED=1

# What a node on the code screen holds, read from the tree: its text, or
# nothing when it has none. The node must be there: a missing node is not
# an empty one.
holds() {
  "$SMIX" tree --device "$SERIAL" --port "$PORT" --json 2>/dev/null | python3 -c '
import json, sys
want = sys.argv[1]
def walk(n):
    if n.get("identifier") == want:
        return n
    for c in n.get("children", []):
        found = walk(c)
        if found is not None:
            return found
    return None
node = walk(json.load(sys.stdin)["root"])
if node is None:
    sys.exit(3)
print(node.get("text") or "")' "$1"
}

run_flow() { # <name> <steps…> — the code screen, fresh, then the steps
  local name="$1"
  shift
  {
    printf 'appId: dev.smix.fixture\n---\n- launchApp:\n    clearState: true\n'
    printf -- '- tapOn: { label: "open-code" }\n'
    printf '%s\n' "$@"
  } >"$WORK/$name.yaml"
  # raw run: judged by its exit together with what the app received and what the other field holds
  "$SMIX" run "$WORK/$name.yaml" --device "$SERIAL" --platform android --runner-port "$PORT" \
    >"$WORK/$name.log" 2>&1
}

# 1. The field that leaves once it is full.
rc=0
run_flow leaving '- inputText: { id: "fixture_code_leaving", text: "4321" }' || rc=$?
result="$(holds fixture_code_result)" || cannot_judge "the code screen has no result label — is this the fixture that has it?"
[ "$result" = "leaving 4321" ] \
  || cannot_judge "the app did not receive the code (result label: '$result'), so there is nothing here about the readback"
address="$(holds fixture_code_email)" || cannot_judge "the code screen has no address field"
[ -z "$address" ] \
  || fail "a field nobody named received typing: the address field holds '$address' after the code field left"
[ "$rc" = 0 ] \
  || fail "the app took the code and the step still failed (exit $rc): $(grep -m1 'FAIL' "$WORK/leaving.log" | cut -c1-300)"
log "a field that left once full: the step passed, the app got 4321, the address field holds nothing"

# 2. No field named, after a tap on the number-pad field.
rc=0
run_flow untargeted \
  '- tapOn: { id: "fixture_code" }' \
  '- extendedWaitUntil: { visible: { role: keyboard }, timeout: 8000 }' \
  '- inputText: "123456"' || rc=$?
[ "$rc" = 0 ] \
  || fail "typing where focus is failed (exit $rc): $(grep -m1 'FAIL' "$WORK/untargeted.log" | cut -c1-300)"
result="$(holds fixture_code_result)" || cannot_judge "the code screen has no result label"
[ "$result" = "code 123456" ] \
  || fail "the app received something other than what was typed: result label '$result'"
log "typing with no field named: the app got exactly 123456"
log "PASS"
