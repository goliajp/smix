#!/usr/bin/env bash
# After Enter in one field and a tap on the next, `inputText` types into
# the field that has focus — and says so — every time.
#
# The sequence is a consumer's sign-in card (their report #11): type into field
# A, press Enter, tap field B (masked), type into B. On the Android
# fixture's Compose screen the runner refused the second `inputText` two
# runs out of three with "no editable field had focus" while the tree
# reported B focused (measured 2026-09-25): `findFocus(FOCUS_INPUT)` kept
# returning A after the tap, a node that itself answers `isFocused=false`,
# and it was taken as the focused field.
#
# Judged by reading the device, not smix's answer: B holds twelve bullets
# (twelve characters, masked) and A holds what was typed into it. Five
# rounds, because the refusal was intermittent.
#
# Exit: 0 judged and passed, 1 judged and failed, 2 could not judge.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
source "$ROOT/scripts/lib/e2e-devices.sh"
ALIAS="${SMIX_C9H_ANDROID:-$E2E_ANDROID}"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"
ROUNDS="${SMIX_C9H_ROUNDS:-5}"

log()  { printf '[c9h-focus] %s\n' "$*" >&2; }
cannot_judge() { printf '[c9h-focus] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }
FAILED=0
fail() { printf '[c9h-focus] FAIL: %s\n' "$*" >&2; FAILED=1; }

SERIAL="" UPPED=0 WE_BOOTED=0
cleanup() {
  if [ "$UPPED" = 1 ]; then
    said="$("$SMIX" runner down --device "$SERIAL" --platform android --runner-port "$PORT" 2>&1)" \
      || printf '[c9h-focus] warning: the runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$ALIAS" >/dev/null 2>&1 || true; fi
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

cat >"$WORK/flow.yaml" <<'EOF'
appId: dev.smix.fixture
---
- launchApp:
    clearState: true
- tapOn: { label: "open-compose" }
- extendedWaitUntil:
    visible: { id: "compose_input" }
    timeout: 10000
- tapOn: { id: "compose_input" }
- inputText: 'harbor-logistics'
- pressKey: enter
- tapOn: { id: "compose_password" }
- inputText: 'Secret!23456'
EOF

# What the two fields hold, read from the runner's tree: `<id>\t<len>\t<text>`.
fields() {
  curl -s "http://127.0.0.1:$PORT/tree" | python3 -c '
import json, sys
d = json.load(sys.stdin)
def walk(n):
    ident = n.get("identifier")
    if ident in ("compose_input", "compose_password"):
        v = n.get("value") or n.get("text") or ""
        print(f"{ident}\t{len(v)}\t{v!r}")
    for c in n.get("children", []):
        walk(c)
walk(d.get("root", d))'
}

for i in $(seq 1 "$ROUNDS"); do
  rc=0
  # raw run: judged by reading both fields from the device, not by smix's answer
  "$SMIX" run "$WORK/flow.yaml" --device "$SERIAL" --platform android --runner-port "$PORT" \
    >"$WORK/run-$i.log" 2>&1 || rc=$?
  got="$(fields)" || cannot_judge "round $i: could not read the tree"
  if [ -z "$got" ] || [ "$rc" != 0 ]; then
    # What the flow said, in full: two reds under load were left with one
    # line each and no way to tell a tap that missed from a screen that
    # never came (2026-09-26).
    printf '[c9h-focus] round %s: smix run said:\n' "$i" >&2
    grep -E '^STEP|error|hint|visible elements|on screen' "$WORK/run-$i.log" | head -40 >&2
  fi
  [ -n "$got" ] || cannot_judge "round $i: the tree named neither field — the reading is broken, not the flow"
  pwd_len="$(printf '%s\n' "$got" | awk -F'\t' '$1=="compose_password"{print $2}')"
  a_text="$(printf '%s\n' "$got" | awk -F'\t' '$1=="compose_input"{print $3}')"
  code="$(sed -nE 's/.*FAIL \[([A-Z_]+)\].*/\1/p' "$WORK/run-$i.log" | tail -1)"
  if [ "$rc" != 0 ] && ! python3 "$ROOT/scripts/lib/failure-codes.py" verdicts | grep -qxF "$code"; then
    # smix could not look (the runner, the transport, the app gone), so
    # this round says nothing about focus after enter. Twice under load 37
    # and 45 a round ended this way or with the tree empty, and neither
    # reproduced at load 11 in ten rounds; the cause is not known.
    cannot_judge "round $i: smix run ended with ${code:-no failure code} (exit $rc), which is not a verdict on the screen: $(grep -m1 'FAIL' "$WORK/run-$i.log" | cut -c1-240)"
  elif [ "$rc" != 0 ]; then
    fail "round $i: the flow failed (exit $rc): $(grep -m1 'FAIL' "$WORK/run-$i.log" | cut -c1-240)"
  elif [ "$pwd_len" != 12 ]; then
    fail "round $i: the masked field holds $pwd_len characters, not 12"
  elif [ "$a_text" != "'harbor-logistics\\n'" ]; then
    fail "round $i: the first field holds $a_text"
  else
    log "round $i: B holds 12, A holds $a_text"
  fi
done

[ "$FAILED" = 0 ] || exit 1
log "PASS ($ROUNDS rounds)"
