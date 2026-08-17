#!/usr/bin/env bash
# v6.2-C4: --env / ${…} interpolation, with a gate that reddens on BOTH
# sides.
#
# The consumer's ④ was `--env PW=…` reaching a `${PW}` in the flow as the
# literal `${PW}` — a silent failure with a cost (the account had a
# finite retry budget). On develop HEAD that does not reproduce: the
# single-run path already injects env into the flow context, and an
# unresolved `${…}` errors out rather than typing the literal. What was
# missing is the gate — the cold plan's own risk note asked for it
# ("④ 修完要有门"). So this checkpoint adds it, and proves its teeth by
# reverting the wiring to the broken form and requiring the red.
#
# By empty-predicate (.claude/rule/empty-predicate.md) the gate is
# two-sided: supplied → the real value lands (judged by field content,
# never a log line — the progress log counts the raw template on purpose,
# and printing the expanded length would leak a secret's length); missing
# → non-zero exit naming `undefined variable`, and the field untouched
# (not the literal, not the value).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
ALIAS="${SMIX_C4_ANDROID:-smix-android}"
PORT="${SMIX_C4_PORT:-22088}"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"

# A distinctive value, so a stale field cannot pass SIDE A by accident.
WORD_A="envParityW7q"
# A variable name held out of --env AND the process env, so "missing"
# really means missing (env_store falls back to std::env::vars()).
MISSING="SMIX_C4_MISSING"

log()  { printf '[c4] %s\n' "$*" >&2; }
step() { printf '[c4] --- %s\n' "$*" >&2; }
fail() { printf '[c4] FAIL: %s\n' "$*" >&2; exit 1; }
skip() { printf '[c4] SKIP: %s\n' "$*" >&2; exit 0; }

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

fill_flow() { # $1 = template to put in inputText
  cat >"$WORK/flow.yaml" <<FLOW
appId: $APPID
---
- launchApp
- tapOn:
    id: fixture_input
- inputText: "$1"
FLOW
}

# ---- presence + baseline ---------------------------------------------
step "presence: fixture_input must be in the tree; record its baseline"
launch_fresh
PRESENT="$(SMIX_RUNNER_PORT="$PORT" "$SMIX" tree --json --device "$SERIAL" 2>/dev/null | grep -c '"fixture_input"' || true)"
[ "$PRESENT" -ge 1 ] || fail "fixture_input is not in the tree — the gate would be reading air"
BASELINE="$(field_text fixture_input)"
log "baseline field == '$BASELINE'"

# ---- SIDE A: supplied → the real value lands -------------------------
step "SIDE A: --env supplied → field must hold '$WORD_A'"
launch_fresh
fill_flow "\${SMIX_C4_VAL}"
A_RC=0
env -u SMIX_C4_VAL SMIX_RUNNER_PORT="$PORT" "$SMIX" run --device "$SERIAL" "$WORK/flow.yaml" --env "SMIX_C4_VAL=$WORD_A" >"$WORK/a.log" 2>&1 || A_RC=$?
[ "$A_RC" -eq 0 ] || fail "SIDE A run exited $A_RC (supplied --env should resolve): $(grep -v '^kevy:' "$WORK/a.log" | tail -2)"
GOT_A="$(field_text fixture_input)"
[ "$GOT_A" = "$WORD_A" ] || fail "SIDE A: field holds '$GOT_A', expected '$WORD_A' — --env did not reach the flow (this is the ④ regression)"
log "SIDE A OK: field == '$GOT_A'"

# ---- SIDE B: missing → error, field untouched, no literal ------------
step "SIDE B: no --env, $MISSING not in env → must error, not type the literal"
launch_fresh
BASE_B="$(field_text fixture_input)"
fill_flow "\${$MISSING}"
B_RC=0
env -u "$MISSING" SMIX_RUNNER_PORT="$PORT" "$SMIX" run --device "$SERIAL" "$WORK/flow.yaml" >"$WORK/b.log" 2>&1 || B_RC=$?
[ "$B_RC" -ne 0 ] || fail "SIDE B exited 0 — log: $(grep -v '^kevy:' "$WORK/b.log" | tail -5)"
grep -qi 'undefined variable' <(grep -v '^kevy:' "$WORK/b.log") \
  || fail "SIDE B did not name 'undefined variable' — the failure must say why, not just be non-zero: $(grep -v '^kevy:' "$WORK/b.log" | tail -2)"
GOT_B="$(field_text fixture_input)"
[ "$GOT_B" != "\${$MISSING}" ] || fail "SIDE B typed the literal \${$MISSING} into the field — this is exactly ④"
[ "$GOT_B" = "$BASE_B" ] || fail "SIDE B touched the field ('$GOT_B' != baseline '$BASE_B') — a failed interpolation must type nothing"
log "SIDE B OK: exit $B_RC, named undefined variable, field untouched ('$GOT_B')"

log "v6.2-C4 PASS: --env interpolation gated both sides — supplied lands, missing errors and types nothing"
