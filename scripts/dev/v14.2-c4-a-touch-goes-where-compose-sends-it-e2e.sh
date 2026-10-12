#!/usr/bin/env bash
# v14.2-C4: a tap is judged to land where the toolkit sends the touch.
#
# A consumer's Home suite on 14.2.0 (2026-10-12): five taps the app answered
# were reported TAP_MISSED. Read from their probe trees the moment before,
# the chain had gone wrong four ways — it stopped at the edge of a sheet's
# short container while the panel sat below it; it let a vertex handle that
# drags with `pointerInput` (no semantics action) be passed through; it took
# every View hosted in Compose to keep a touch, so a full-size View that is
# not clickable stood over a player's "GO LIVE"; and it did not list a label
# the touch went through to the card it opens.
#
# The fixture's touch screen has each shape and counts what it receives, so
# where a tap went is read from the app: a tap on each, with no allowance for
# a mismatch, and the counts. Then the other side, which has to stay red: a
# tap at a control under a clickable scrim is a miss, and the app says the
# scrim took it.
#
# Against the 14.2.0 runner the handle, GO LIVE, label and panel taps are
# each reported TAP_MISSED.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
ALIAS="${SMIX_V142_C4_ANDROID:-$E2E_ANDROID}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"

log()  { printf '[v14.2-c4] %s\n' "$*" >&2; }
fail() { printf '[v14.2-c4] FAIL: %s\n' "$*" >&2; exit 1; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0 WORK=""
cleanup() {
  [ -n "$WORK" ] && rm -rf "$WORK"
  if [ "$WE_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
      >/dev/null || printf '[v14.2-c4] warning: the runner was not stopped\n' >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || fail "no adb on PATH — this judges an Android runner and cannot"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
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
  # This installs a fixture app; a registered phone belongs to somebody.
  *) fail "$ALIAS resolves to '$SERIAL', which is not an emulator — refusing" ;;
esac
with_deadline 120 adb -s "$SERIAL" wait-for-device || fail "$SERIAL did not come up"
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

fresh() {
  adb -s "$SERIAL" shell am start -W -S -n "$APPID/.TouchActivity" >/dev/null 2>&1 \
    || fail "could not start the touch screen"
}

flow() { # $1 name; flow body on stdin. Prints the exit code.
  { printf 'appId: %s\n---\n' "$APPID"; cat; } >"$WORK/$1.yaml"
  local rc=0
  env -u SMIX_TAP_HIT_MISMATCH SMIX_RUNNER_PORT="$PORT" \
    "$SMIX_RUN" --device "$SERIAL" --platform android --no-launch "$WORK/$1.yaml" >"$WORK/$1.log" 2>&1 || rc=$?
  echo "$rc"
}

must_pass() { # $1 name, $2 what
  local rc
  rc="$(flow "$1")"
  if [ "$rc" -ne 0 ]; then
    grep -m4 -E '^(STEP|error)|FAIL' "$WORK/$1.log" >&2 || tail -8 "$WORK/$1.log" >&2
    fail "$2 (exit $rc)"
  fi
}

adb -s "$SERIAL" logcat -b crash -c >/dev/null 2>&1 || fail "could not clear the crash log"

fresh
must_pass handle "a tap on the handle that only drags was not judged to land on it" <<'FLOW'
- tapOn: { id: touch-handle }
- assertVisible: "handle=1 zone=0 pill=0 card=0 sheet=0 panel=0"
FLOW
log "handle: the handle counted it, the zone under it did not"

fresh
must_pass pill "a tap on GO LIVE under a View that is not clickable was not judged to land on it" <<'FLOW'
- tapOn: "GO LIVE"
- assertVisible: "handle=0 zone=0 pill=1 card=0 sheet=0 panel=0"
FLOW
log "GO LIVE: the hosted control counted it through the View over it"

fresh
must_pass label "a tap on the label was not judged to land, though the card it is in opened" <<'FLOW'
- tapOn: { id: touch-card-label }
- assertVisible: "handle=0 zone=0 pill=0 card=1 sheet=0 panel=0"
FLOW
log "label: the touch went through it, and the card counted it"

fresh
must_pass panel "a tap on the panel below its container was not judged to land on it" <<'FLOW'
- tapOn: { id: touch-open-panel }
- tapOn: { id: touch-panel-30 }
- assertVisible: "handle=0 zone=0 pill=0 card=0 sheet=0 panel=1"
FLOW
log "panel: the panel below its container counted it"

fresh
must_pass sheet "a tap on the Material sheet's row was not judged to land on it" <<'FLOW'
- tapOn: { id: touch-open-sheet }
- extendedWaitUntil:
    visible: { id: touch-sheet-30 }
    timeout: 10000
- tapOn: { id: touch-sheet-30 }
- extendedWaitUntil:
    visible: "handle=0 zone=0 pill=0 card=0 sheet=1 panel=0"
    timeout: 10000
FLOW
log "sheet: the Material sheet's row counted it"

# The other side: under a clickable scrim, the scrim takes it.
fresh
must_pass open "the panel would not open" <<'FLOW'
- tapOn: { id: touch-open-panel }
- assertVisible: { id: touch-panel-30 }
FLOW
rc="$(flow under <<'FLOW'
- tapOn: { id: touch-open-sheet }
FLOW
)"
[ "$rc" -ne 0 ] || fail "a tap at a control under the clickable scrim passed"
grep -q "TAP_MISSED" "$WORK/under.log" || { tail -10 "$WORK/under.log" >&2; fail "the tap under the scrim failed with something other than TAP_MISSED"; }
grep -q 'Close panel' "$WORK/under.log" || { tail -10 "$WORK/under.log" >&2; fail "the miss does not name the scrim that took the touch"; }
must_pass after "after the tap under the scrim the app is not where a scrim tap leaves it" <<'FLOW'
- assertNotVisible: { id: touch-panel-30 }
- assertVisible: "handle=0 zone=0 pill=0 card=0 sheet=0 panel=0"
FLOW
log "under the scrim: reported TAP_MISSED naming the scrim, and the scrim is what the app says took it"

crashes="$(adb -s "$SERIAL" logcat -b crash -d 2>/dev/null | grep -c 'FATAL EXCEPTION' || true)"
[ "$crashes" = 0 ] || fail "the app crashed $crashes time(s)"

log "V14.2-C4-E2E-PASS on $SERIAL"
