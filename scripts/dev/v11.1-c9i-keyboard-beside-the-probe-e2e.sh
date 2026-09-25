#!/usr/bin/env bash
# On an app that carries the probe, a flow sees the keyboard come and go.
#
# The probe reads the app from inside its process and sees nothing else, and
# the tree used to be the probe's alone — so `role:keyboard` timed out on any
# app carrying the probe while the keyboard was plainly up (10.1.0 in flows;
# from v10.2 in every CLI verb too). Found 2026-09-25 by the release's
# `the-three-that-went-red` #3, three runs out of three.
#
# Two witnesses beside smix's own answer:
#   - the tree on that screen says `semantics` — otherwise this is not the
#     probe's path at all and a green would be about the other reader;
#   - the device's input-method service says the keyboard is shown once the
#     flow has waited for it, and is not shown after `hideKeyboard`.
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
ALIAS="${SMIX_C9I_ANDROID:-$E2E_ANDROID}"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"

log()  { printf '[c9i-keyboard] %s\n' "$*" >&2; }
cannot_judge() { printf '[c9i-keyboard] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }
fail() { printf '[c9i-keyboard] FAIL: %s\n' "$*" >&2; exit 1; }

SERIAL="" UPPED=0 WE_BOOTED=0
cleanup() {
  if [ "$UPPED" = 1 ]; then
    said="$("$SMIX" runner down --device "$SERIAL" --platform android --runner-port "$PORT" 2>&1)" \
      || printf '[c9i-keyboard] warning: the runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then
    said="$("$SMIX" sim shutdown "$ALIAS" 2>&1)" \
      || printf '[c9i-keyboard] warning: %s was not shut down:\n%s\n' "$ALIAS" "$(printf '%s' "$said" | tail -3)" >&2
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

# Whether the device's input-method service has the keyboard shown.
ime_shown() {
  adb -s "$SERIAL" shell dumpsys input_method | grep -o 'mInputShown=[a-z]*' | head -1 | cut -d= -f2
}

cat >"$WORK/open.yaml" <<'EOF'
appId: dev.smix.fixture
---
- launchApp:
    clearState: true
- tapOn: { label: "open-compose" }
- extendedWaitUntil:
    visible: { id: "compose_input" }
    timeout: 10000
EOF
# raw run: its only job is to reach the screen; the judgement is below
"$SMIX" run "$WORK/open.yaml" --device "$SERIAL" --platform android --runner-port "$PORT" \
  >"$WORK/open.log" 2>&1 || cannot_judge "could not reach the Compose screen: $(grep -m1 FAIL "$WORK/open.log" | cut -c1-200)"

source_now="$("$SMIX" tree --device "$SERIAL" --port "$PORT" --json 2>/dev/null \
  | python3 -c 'import json,sys; print(json.load(sys.stdin).get("source",""))')" || true
[ "$source_now" = "semantics" ] \
  || cannot_judge "the tree on the Compose screen came from '$source_now', not the probe — this would not be the probe's path"
log "the Compose screen is read through the probe"

cat >"$WORK/up.yaml" <<'EOF'
appId: dev.smix.fixture
---
- tapOn: { id: "compose_input" }
- extendedWaitUntil:
    visible: { role: keyboard }
    timeout: 8000
EOF
cat >"$WORK/down.yaml" <<'EOF'
appId: dev.smix.fixture
---
- hideKeyboard
- assertNotVisible: { role: keyboard }
EOF

rc=0
# raw run: judged by its exit and by the device's own input-method state
"$SMIX" run "$WORK/up.yaml" --device "$SERIAL" --platform android --runner-port "$PORT" \
  >"$WORK/up-run.log" 2>&1 || rc=$?
shown="$(ime_shown)"
[ -n "$shown" ] || cannot_judge "could not read the device's input-method state"
[ "$shown" = "true" ] \
  || cannot_judge "the device shows no keyboard after the tap (mInputShown=$shown) — there is nothing to see"
[ "$rc" = 0 ] \
  || fail "the device shows the keyboard and the flow did not see it (exit $rc): $(grep -m1 'FAIL' "$WORK/up-run.log" | cut -c1-240)"
log "keyboard up: the device says so and the flow saw it"

rc=0
# raw run: judged by its exit and by the device's own input-method state
"$SMIX" run "$WORK/down.yaml" --device "$SERIAL" --platform android --runner-port "$PORT" \
  >"$WORK/down-run.log" 2>&1 || rc=$?
after="$(ime_shown)"
[ "$rc" = 0 ] \
  || fail "hideKeyboard then assertNotVisible failed (exit $rc): $(grep -m1 'FAIL' "$WORK/down-run.log" | cut -c1-240)"
[ "$after" = "false" ] || fail "the flow passed and the device still shows the keyboard (mInputShown=$after)"
log "keyboard down: the flow says so and the device agrees"
log "PASS"
