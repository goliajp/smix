#!/usr/bin/env bash
# v6.2-C3: the same capability, through every entrance.
#
# `fill` and `find text:` from the CLI were 501 on Android while the same
# device's flow `inputText` worked — a capability that existed or not
# depending on which door you came through (the consumer's ③). The fix
# is one rule at both entrances: the act verbs dial the driver from the
# device's platform, the way `smix run` already does (C1). This pins it
# on a real device.
#
# The judge is field content, not a return code. By empty-predicate
# (.claude/rule/empty-predicate.md) the parity claim needs both sides
# reachable: presence first (a tree without `fixture_input` is reading
# air, and red), then two entrances writing two DIFFERENT words into the
# same field so which one landed is decidable, then find proved on a
# present AND an absent needle. "Both entrances 501" cannot pass this —
# it asks for content in the field, not for two matching status codes.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
ALIAS="${SMIX_C3_ANDROID:-smix-android}"
PORT="${SMIX_C3_PORT:-22088}"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"

log()  { printf '[c3] %s\n' "$*" >&2; }
step() { printf '[c3] --- %s\n' "$*" >&2; }
fail() { printf '[c3] FAIL: %s\n' "$*" >&2; exit 1; }
skip() { printf '[c3] SKIP: %s\n' "$*" >&2; exit 0; }

WE_BOOTED_IT=no
cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
command -v adb >/dev/null 2>&1 || skip "no adb — this needs the Android SDK"

SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | grep -v '^kevy:' | tr -d '[:space:]')" || true
[ -n "$SERIAL" ] || skip "no emulator registered as '$ALIAS' — register one first"
adb devices 2>/dev/null | grep -q "^$SERIAL[[:space:]]*device" || skip "device $SERIAL not attached"
[ -f "$APK" ] || skip "no Android fixture apk (scripts/dev/build-android-fixture.sh)"
curl -s "http://127.0.0.1:$PORT/health" 2>/dev/null | grep -q smix-android-runner \
  || skip "no Android runner on $PORT — bring it up: smix runner up $SERIAL --platform android --runner-port $PORT"
log "device $SERIAL, runner $PORT"

adb -s "$SERIAL" install -r "$APK" >"$WORK/install.log" 2>&1 || fail "fixture install failed: $(tail -2 "$WORK/install.log")"

# Read one node's text field out of the tree, by id. Prints the text or
# nothing. A missing node prints nothing — the presence check below turns
# that into a red rather than a false pass.
field_text() {
  SMIX_RUNNER_PORT="$PORT" "$SMIX" tree --json --device "$SERIAL" 2>/dev/null \
    | grep -v '^kevy:' \
    | python3 -c "
import sys,json
want=sys.argv[1]; found=[]
def walk(n):
    if n.get('identifier')==want: found.append(n.get('text'))
    for c in n.get('children',[]) or []: walk(c)
try: walk(json.load(sys.stdin))
except Exception: pass
print(found[0] if found and found[0] is not None else '')
" "$1"
}

launch_fresh() {
  adb -s "$SERIAL" shell am force-stop "$APPID" >/dev/null 2>&1 || true
  printf 'appId: %s\n---\n- launchApp\n' "$APPID" >"$WORK/launch.yaml"
  SMIX_RUNNER_PORT="$PORT" "$SMIX" run --device "$SERIAL" "$WORK/launch.yaml" >/dev/null 2>&1 \
    || fail "could not launch $APPID"
}

# ---- presence (empty-predicate: no fixture_input = reading air) -------
step "presence: fixture_input must be in the tree"
launch_fresh
PRESENT="$(SMIX_RUNNER_PORT="$PORT" "$SMIX" tree --json --device "$SERIAL" 2>/dev/null | grep -c '"fixture_input"' || true)"
[ "$PRESENT" -ge 1 ] || fail "fixture_input is not in the tree — the gate would be reading air, not testing parity"

# ---- flow entrance ---------------------------------------------------
WORD_FLOW="flowWORD1"
step "flow entrance: inputText '$WORD_FLOW' → field must hold it"
launch_fresh
cat >"$WORK/flow.yaml" <<FLOW
appId: $APPID
---
- launchApp
- tapOn:
    id: fixture_input
- inputText: "$WORD_FLOW"
FLOW
FLOW_RC=0
if ! SMIX_RUNNER_PORT="$PORT" "$SMIX" run --device "$SERIAL" "$WORK/flow.yaml" >/dev/null 2>&1; then FLOW_RC=$?; fi
[ "$FLOW_RC" -eq 0 ] || fail "flow entrance run exited $FLOW_RC"
GOT_FLOW="$(field_text fixture_input)"
[ "$GOT_FLOW" = "$WORD_FLOW" ] || fail "flow entrance: field holds '$GOT_FLOW', expected '$WORD_FLOW'"
log "flow entrance OK: field == '$GOT_FLOW'"

# ---- CLI entrance (a DIFFERENT word, so which door wrote it is decidable)
WORD_CLI="cliWORD2"
step "CLI entrance: smix fill '$WORD_CLI' → field must hold it"
launch_fresh
FILL_RC=0
if ! SMIX_RUNNER_PORT="$PORT" "$SMIX" fill 'id:fixture_input' --text "$WORD_CLI" --device "$SERIAL" >/dev/null 2>&1; then FILL_RC=$?; fi
[ "$FILL_RC" -eq 0 ] || fail "CLI fill exited $FILL_RC (this is the 501 the fix removes)"
GOT_CLI="$(field_text fixture_input)"
[ "$GOT_CLI" = "$WORD_CLI" ] || fail "CLI entrance: field holds '$GOT_CLI', expected '$WORD_CLI' — content did not land"
[ "$WORD_CLI" != "$WORD_FLOW" ] || fail "the two words must differ or the entrances are indistinguishable"
log "CLI entrance OK: field == '$GOT_CLI'"

# ---- find, proved on a present AND an absent needle -------------------
step "CLI find: present → exists=true, absent → exists=false"
FIND_P_RC=0
if ! SMIX_RUNNER_PORT="$PORT" "$SMIX" find 'text:SUBMIT' --device "$SERIAL" >"$WORK/find_present.out" 2>&1; then FIND_P_RC=$?; fi
[ "$FIND_P_RC" -eq 0 ] || fail "find (present) exited $FIND_P_RC (the 501 the fix removes): $(grep -v '^kevy:' "$WORK/find_present.out" | tail -2)"
grep -q '^exists=true' <(grep -v '^kevy:' "$WORK/find_present.out") || fail "find 'text:SUBMIT' did not report exists=true"
log "find present: exists=true"

SMIX_RUNNER_PORT="$PORT" "$SMIX" find 'text:NoSuchElementZZZ' --device "$SERIAL" >"$WORK/find_absent.out" 2>&1 || true
grep -q '^exists=false' <(grep -v '^kevy:' "$WORK/find_absent.out") || fail "find of an absent needle did not report exists=false — the find judge is not two-sided"
log "find absent: exists=false"

log "v6.2-C3 PASS: fill and find reach the same field from CLI and flow; find proved both sides"
