#!/usr/bin/env bash
# v16.0-C9: words that are not ASCII reach the Android runner as they were sent.
#
# A consumer's Wi-Fi list (2026-10-18) had a row named `カフェ☕ Wi-Fi`.
# `smix find` found it and `smix tap` refused it for 5 s: "had left the
# point", while the runner read the same name under the point. The runner
# decoded every request body as US-ASCII — NanoHTTPD does, when the
# Content-Type names no charset — so the name it was told to look for had
# become replacement characters. The same decoding broke `inputText`, and
# behind it `input text` cannot type such characters at all (it throws).
#
# The fixture's text screen has that row beside an ASCII one, each counting
# the taps it receives, and a field whose text is echoed by the app.
# Judged by the app's counters and echo, not by smix's report:
#   - a tap by text and by label on the non-ASCII row reaches it
#   - a fill of non-ASCII text leaves the field holding exactly that text
#   - `runner down --device <serial>` without `--platform` stops this
#     emulator's runner, and leaves an iOS runner on 22087 alone
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
# shellcheck source=../lib/e2e-devices.sh
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
ALIAS="${SMIX_C9_ANDROID:-$E2E_ANDROID}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
CAFE="カフェ☕ Wi-Fi"
TYPED="東京☕カフェ"

log()  { printf '[v16.0-c9] %s\n' "$*" >&2; }
fail() { printf '[v16.0-c9] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v16.0-c9] cannot judge: %s\n' "$*" >&2; exit 2; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0 WORK=""
cleanup() {
  [ -n "$WORK" ] && rm -rf "$WORK"
  if [ "$WE_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
      >/dev/null || printf '[v16.0-c9] warning: the runner was not stopped\n' >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || fail "no adb on PATH — this judges an Android runner and cannot"
[ -f "$APK" ] || fail "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"

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
WORK="$(mktemp -d)"

adb -s "$SERIAL" shell am start -W -S -n "$APPID/.TextActivity" >/dev/null 2>&1 \
  || fail "could not start the text screen"

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
taps_of() { reading "$1 taps "; }
seen=""
for _ in $(seq 1 40); do seen="$(taps_of "$CAFE")"; [ "$seen" != "-" ] && break; sleep 0.25; done
[ "$seen" = 0 ] || fail "the text screen is not showing ('$CAFE taps' reads '$seen')"

# ---- 1. taps: the ASCII row first, so a broken tap is told from a broken name ----
"$SMIX" tap --device "$SERIAL" --port "$PORT" "text:Lobby Guest" >"$WORK/lobby.log" 2>&1 \
  || cannot_judge "a tap on the ASCII row failed, so a failure on the other row would say nothing about its name: $(tail -3 "$WORK/lobby.log")"
[ "$(taps_of "Lobby Guest")" = 1 ] || cannot_judge "the ASCII row did not count its tap"

for form in text label; do
  before="$(taps_of "$CAFE")"
  rc=0
  "$SMIX" tap --device "$SERIAL" --port "$PORT" "$form:$CAFE" >"$WORK/cafe-$form.log" 2>&1 || rc=$?
  after="$(taps_of "$CAFE")"
  log "tap $form:\"$CAFE\": exit $rc, the row counted $before → $after"
  [ "$rc" = 0 ] || { tail -5 "$WORK/cafe-$form.log" >&2; fail "the tap by $form on the non-ASCII row failed"; }
  [ "$after" = $((before + 1)) ] || fail "the tap by $form was reported and the non-ASCII row did not count it"
done

# ---- 2. fill: what the app's field received, read from the app's echo ----
rc=0
"$SMIX" fill --device "$SERIAL" --port "$PORT" "id:fixture_text_input" --text "$TYPED" \
  >"$WORK/fill.log" 2>&1 || rc=$?
held="$(reading "field holds ")"
log "fill \"$TYPED\": exit $rc, the app's field holds \"$held\""
[ "$rc" = 0 ] || { tail -5 "$WORK/fill.log" >&2; fail "the fill of non-ASCII text failed"; }
[ "$held" = "$TYPED" ] || fail "the field holds \"$held\", not \"$TYPED\""

# ---- 3. runner down names the device, not a platform ----
# Only a runner this script brought up is stopped; 22087 is whoever's
# iOS runner it is, and is only looked at.
if [ "$WE_UPPED" = 1 ]; then
  ios_before="$(curl -s -m 5 -o /dev/null -w '%{http_code}' http://localhost:22087/health || true)"
  "$SMIX" runner down --device "$SERIAL" --runner-port "$PORT" >"$WORK/down.log" 2>&1 \
    || { cat "$WORK/down.log" >&2; fail "runner down --device $SERIAL without --platform failed"; }
  WE_UPPED=0
  ios_after="$(curl -s -m 5 -o /dev/null -w '%{http_code}' http://localhost:22087/health || true)"
  log "runner down --device $SERIAL: $(tr '\n' ' ' <"$WORK/down.log"); 22087 answered $ios_before before, $ios_after after"
  ! curl -s -m 5 "http://localhost:$PORT/health" >/dev/null 2>&1 \
    || fail "the runner on $PORT still answers after runner down --device $SERIAL"
  [ "$ios_before" = "$ios_after" ] || fail "runner down --device $SERIAL changed what answers on 22087"
else
  log "runner down --device: not judged — the runner on $PORT was not brought up by this script"
fi

log "V16.0-C9-E2E-PASS"
