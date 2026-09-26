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
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/e2e-devices.sh
source "$ROOT/scripts/lib/e2e-devices.sh"
# Our own emulator, and a port of this script's own. It defaulted to the
# alias `smix-android` — on this machine a consumer's emulator — and
# waited for somebody to have a runner up on 22088, reporting a skip when
# nobody had (2026-09-25). A check that cannot set itself up is red.
ALIAS="${SMIX_C4_ANDROID:-$E2E_ANDROID}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
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

SERIAL="" WE_BOOTED=0 WE_UPPED=0
cleanup() {
  if [ "$WE_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
      >/dev/null 2>&1 || printf '[c4] warning: the runner was not stopped\n' >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
command -v adb >/dev/null 2>&1 || fail "no adb — this needs the Android SDK"

SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)"
[ -n "$SERIAL" ] || fail "no device registered as $ALIAS"
case "$SERIAL" in
  emulator-*) : ;;
  *) fail "$ALIAS resolves to $SERIAL, which is not an emulator — refusing" ;;
esac
[ -f "$APK" ] || fail "no Android fixture apk (scripts/dev/build-android-fixture.sh)"
# And the one THESE sources build: the path existing says a build
# happened, not which sources it happened over (open-items O1).
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"
if ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  log "booting $SERIAL"
  "$SMIX" sim boot "$ALIAS" >/dev/null 2>&1 || fail "could not boot $ALIAS"
  WE_BOOTED=1
  adb -s "$SERIAL" wait-for-device
fi
"$SMIX" runner up "$ALIAS" --platform android --runner-port "$PORT" >"$WORK/up.log" 2>&1 \
  || fail "the runner would not start on $SERIAL:$PORT: $(tail -3 "$WORK/up.log")"
WE_UPPED=1
log "device $SERIAL, runner $PORT"

adb -s "$SERIAL" install -r "$APK" >"$WORK/install.log" 2>&1 || fail "fixture install failed: $(tail -2 "$WORK/install.log")"

field_text() {
  SMIX_RUNNER_PORT="$PORT" "$SMIX" tree --json --device "$SERIAL" 2>/dev/null \
    | python3 -c "
import sys,json
want=sys.argv[1]; found=[]
def walk(n):
    if n.get('identifier')==want: found.append(n.get('text'))
    for c in n.get('children',[]) or []: walk(c)
# tree --json answers {source, root} since 10.0; the tree is under root.
try:
    d=json.load(sys.stdin); walk(d.get('root', d))
except Exception: pass
print(found[0] if found and found[0] is not None else '')
" "$1"
}

launch_fresh() {
  adb -s "$SERIAL" shell am force-stop "$APPID" >/dev/null 2>&1 || true
  printf 'appId: %s\n---\n- launchApp\n' "$APPID" >"$WORK/launch.yaml"
  SMIX_RUNNER_PORT="$PORT" "$SMIX_RUN" --device "$SERIAL" "$WORK/launch.yaml" >/dev/null 2>&1 \
    || fail "could not launch $APPID"
  # launchApp returns before the view hierarchy is laid out; read the
  # field only once it is on the screen, not from an early empty tree.
  for _ in $(seq 1 15); do
    SMIX_RUNNER_PORT="$PORT" "$SMIX" tree --json --device "$SERIAL" 2>/dev/null \
      | grep -q '"fixture_input"' && return 0
    sleep 1
  done
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
env -u SMIX_C4_VAL SMIX_RUNNER_PORT="$PORT" "$SMIX_RUN" --device "$SERIAL" "$WORK/flow.yaml" --env "SMIX_C4_VAL=$WORD_A" >"$WORK/a.log" 2>&1 || A_RC=$?
[ "$A_RC" -eq 0 ] || fail "SIDE A run exited $A_RC (supplied --env should resolve): $(tail -2 "$WORK/a.log")"
GOT_A="$(field_text fixture_input)"
[ "$GOT_A" = "$WORD_A" ] || fail "SIDE A: field holds '$GOT_A', expected '$WORD_A' — --env did not reach the flow (this is the ④ regression)"
log "SIDE A OK: field == '$GOT_A'"

# ---- SIDE B: missing → error, field untouched, no literal ------------
step "SIDE B: no --env, $MISSING not in env → must error, not type the literal"
launch_fresh
BASE_B="$(field_text fixture_input)"
fill_flow "\${$MISSING}"
B_RC=0
# raw run: an undefined variable is refused before any step; that refusal is what side B judges
env -u "$MISSING" SMIX_RUNNER_PORT="$PORT" "$SMIX" run --device "$SERIAL" "$WORK/flow.yaml" >"$WORK/b.log" 2>&1 || B_RC=$?
[ "$B_RC" -ne 0 ] || fail "SIDE B exited 0 — log: $(tail -5 "$WORK/b.log")"
grep -qi 'undefined variable' "$WORK/b.log" \
  || fail "SIDE B did not name 'undefined variable' — the failure must say why, not just be non-zero: $(tail -2 "$WORK/b.log")"
GOT_B="$(field_text fixture_input)"
[ "$GOT_B" != "\${$MISSING}" ] || fail "SIDE B typed the literal \${$MISSING} into the field — this is exactly ④"
[ "$GOT_B" = "$BASE_B" ] || fail "SIDE B touched the field ('$GOT_B' != baseline '$BASE_B') — a failed interpolation must type nothing"
log "SIDE B OK: exit $B_RC, named undefined variable, field untouched ('$GOT_B')"

log "v6.2-C4 PASS: --env interpolation gated both sides — supplied lands, missing errors and types nothing"
