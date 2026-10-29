#!/usr/bin/env bash
# v18.0-C3: text a keyboard would compose goes in without it, and a fill
# the keyboard rewrote says so.
#
# A consumer's Samsung S22 (2026-10-29), its keyboard in Chinese mode,
# turned `probe@example.test` into `probe@买票了.`: `input text` presses
# keys, and the keys went through the keyboard. The read-back failed the
# step, and its message said only that there were characters nothing had
# typed. The emulator's keyboard has no such mode a gate can switch on, so
# the fixture's text screen has a field that does the same to typing —
# Latin letters become full-width ones — and leaves text set on it alone.
#
# Judged by the app's own echo of what the field holds:
#   0. a plain field typed into holds exactly the text (the path works)
#   1. a typed fill into the composing field fails, the field really
#      holds composed text, and the message says the typed keys were
#      composed and points at `method: setText`
#   2. `smix fill --set-text` into it leaves exactly the text
#   3. a flow's `inputText: { id, text, method: setText }` does the same
#   4. iOS refuses `--set-text` by name, before any runner is asked
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
# shellcheck source=../lib/e2e-devices.sh
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
ALIAS="${SMIX_C3_ANDROID:-$E2E_ANDROID}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
TEXT="probe@example.test"
# what the composing field makes of TEXT when it is typed
COMPOSED="ｐｒｏｂｅ@ｅｘａｍｐｌｅ.ｔｅｓｔ"

log()  { printf '[v18.0-c3] %s\n' "$*" >&2; }
fail() { printf '[v18.0-c3] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v18.0-c3] cannot judge: %s\n' "$*" >&2; exit 2; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0 WORK=""
cleanup() {
  [ -n "$WORK" ] && rm -rf "$WORK"
  if [ "$WE_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
      >/dev/null || printf '[v18.0-c3] warning: the runner was not stopped\n' >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || fail "no adb on PATH — this judges an Android runner and cannot"
[ -f "$APK" ] || fail "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"

WORK="$(mktemp -d)"

# ---- 4 first: it needs no device ----
rc=0
"$SMIX" fill --platform ios --port 1 "id:anything" --text "$TEXT" --set-text >"$WORK/ios.log" 2>&1 || rc=$?
log "ios fill --set-text: exit $rc: $(tail -1 "$WORK/ios.log")"
[ "$rc" != 0 ] || fail "iOS accepted --set-text"
grep -q "Android-only" "$WORK/ios.log" || fail "iOS refused --set-text without saying it is Android-only"

if ! SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" \
   || [ -z "$SERIAL" ] || ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  log "booting $ALIAS"
  with_deadline 300 "$SMIX" sim boot "$ALIAS" >/dev/null 2>&1 || fail "could not boot $ALIAS"
  WE_BOOTED=1
  SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" || fail "$ALIAS was booted and does not resolve"
fi
case "$SERIAL" in
  emulator-*) : ;;
  *) fail "$ALIAS resolves to '$SERIAL', which is not an emulator — refusing" ;;
esac
for _ in $(seq 1 60); do
  adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done

adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"
if ! curl -s -m 5 "http://localhost:$PORT/health" >/dev/null 2>&1; then
  "$SMIX" runner up "$ALIAS" --platform android --runner-port "$PORT" >/dev/null 2>&1 \
    || fail "the runner would not start on $SERIAL:$PORT"
  WE_UPPED=1
fi

open_screen() {
  adb -s "$SERIAL" shell am start -W -S -n "$APPID/.TextActivity" >/dev/null 2>&1 \
    || fail "could not start the text screen"
  local seen=""
  for _ in $(seq 1 40); do seen="$(reading "composing field holds ")"; [ "$seen" = nothing ] && return 0; sleep 0.25; done
  fail "the text screen is not showing ('composing field holds' reads '$seen')"
}

reading() { # $1 the line's prefix; prints the rest of the app's own line, "-" when it is not on screen
  "$SMIX" tree --json --port "$PORT" --device "$SERIAL" --platform android 2>/dev/null | python3 -c '
import json, sys
prefix = sys.argv[1]
def walk(n):
    t = n.get("text") or ""
    if t.startswith(prefix):
        print(t[len(prefix):]); raise SystemExit
    for c in n.get("children") or []:
        walk(c)
walk(json.load(sys.stdin)["root"])
print("-")' "$1"
}

open_screen

# ---- 0. the plain field, typed: the path a failure below is told from ----
"$SMIX" fill --device "$SERIAL" --port "$PORT" "id:fixture_text_input" --text "$TEXT" \
  >"$WORK/plain.log" 2>&1 || cannot_judge "typing into the plain field failed: $(tail -3 "$WORK/plain.log")"
[ "$(reading "field holds ")" = "$TEXT" ] \
  || cannot_judge "the plain field holds '$(reading "field holds ")' after typing '$TEXT'"

# ---- 1. typed into the composing field: fails, and says why ----
rc=0
"$SMIX" fill --device "$SERIAL" --port "$PORT" "id:fixture_text_composing" --text "$TEXT" \
  >"$WORK/typed.log" 2>&1 || rc=$?
held="$(reading "composing field holds ")"
log "typed fill: exit $rc, the app's field holds \"$held\""
[ "$held" = "$COMPOSED" ] || cannot_judge "the composing field holds \"$held\", not the composed \"$COMPOSED\": the fixture did not compose"
[ "$rc" != 0 ] || fail "a fill that left \"$held\" in the field for \"$TEXT\" reported success"
grep -q "composed the typed keys" "$WORK/typed.log" \
  || { tail -4 "$WORK/typed.log" >&2; fail "the failure does not say the typed keys were composed"; }
grep -q "method: setText" "$WORK/typed.log" || fail "the failure does not point at method: setText"
grep -q "keyboard on screen ([a-z][a-z0-9_.]*)" "$WORK/typed.log" \
  || fail "the failure does not name the keyboard that was up"
log "  said: $(grep -o 'The field gained[^`]*' "$WORK/typed.log" | head -1)"

# ---- 2. smix fill --set-text into it ----
rc=0
"$SMIX" fill --device "$SERIAL" --port "$PORT" "id:fixture_text_composing" --text "$TEXT" --set-text \
  >"$WORK/set.log" 2>&1 || rc=$?
held="$(reading "composing field holds ")"
log "fill --set-text: exit $rc, the app's field holds \"$held\"; said: $(tail -1 "$WORK/set.log")"
[ "$rc" = 0 ] || { tail -5 "$WORK/set.log" >&2; fail "fill --set-text failed"; }
[ "$held" = "$TEXT" ] || fail "after fill --set-text the field holds \"$held\", not \"$TEXT\""
grep -q "set without the keyboard" "$WORK/set.log" || fail "fill --set-text did not say the text was set"

# ---- 3. a flow's method: setText ----
open_screen
cat >"$WORK/flow.yaml" <<FLOW
appId: $APPID
---
- inputText:
    id: fixture_text_composing
    text: "$TEXT"
    method: setText
FLOW
rc=0
"$SMIX_RUN" --device "$SERIAL" --runner-port "$PORT" "$WORK/flow.yaml" >"$WORK/flow.log" 2>&1 || rc=$?
held="$(reading "composing field holds ")"
log "flow method: setText: exit $rc, the app's field holds \"$held\""
[ "$rc" = 0 ] || { tail -8 "$WORK/flow.log" >&2; fail "the flow with method: setText failed"; }
[ "$held" = "$TEXT" ] || fail "after the flow the field holds \"$held\", not \"$TEXT\""

log "V18.0-C3-E2E-PASS"
