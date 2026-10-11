#!/usr/bin/env bash
# v14.1-C6: a tap on a control under a layer that does nothing with a touch
# is judged to land on the control.
#
# A consumer's camera page (2026-10-12): `tapOn` on the "GO LIVE" pill went
# back to live and was reported TAP_MISSED, the point "inside id=box-detection".
# The detection boxes are a full-size Compose layer composed after the pill,
# and the judgement took the later of two siblings as the one a touch goes
# to. Compose gives it to the topmost node that acts on it, and the boxes
# act on nothing.
#
# The fixture's live screen moves a box every frame in such a layer over
# the pill, which appears two seconds in. Five fresh starts, a tap on the
# pill each time with no allowance for a mismatch, and the app's own count
# of presses.
#
# There is no SKIP path: a gate that cannot judge must be red.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
ALIAS="${SMIX_V141_C6_ANDROID:-$E2E_ANDROID}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"

log()  { printf '[v14.1-c6] %s\n' "$*" >&2; }
fail() { printf '[v14.1-c6] FAIL: %s\n' "$*" >&2; exit 1; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0 WORK=""
cleanup() {
  [ -n "$WORK" ] && rm -rf "$WORK"
  if [ "$WE_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
      >/dev/null || printf '[v14.1-c6] warning: the runner was not stopped\n' >&2
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
run_flow() {
  local name="$1" rc=0
  env -u SMIX_TAP_HIT_MISMATCH SMIX_RUNNER_PORT="$PORT" \
    "$SMIX_RUN" --device "$SERIAL" --platform android --no-launch "$WORK/$name.yaml" >"$WORK/$name.log" 2>&1 || rc=$?
  if [ "$rc" -ne 0 ]; then
    grep -m4 -E '^(STEP|error)|FAIL' "$WORK/$name.log" >&2 || tail -8 "$WORK/$name.log" >&2
    fail "the $name flow did not pass (exit $rc)"
  fi
}

WORK="$(mktemp -d)"
cat >"$WORK/live.yaml" <<EOF
appId: $APPID
---
- extendedWaitUntil:
    visible:
      id: "live-go"
    timeout: 20000
- tapOn:
    id: "live-go"
- assertVisible: "live-presses: 1"
EOF

adb -s "$SERIAL" logcat -b crash -c >/dev/null 2>&1 || fail "could not clear the crash log"
ROUNDS=5
for round in $(seq 1 "$ROUNDS"); do
  adb -s "$SERIAL" shell am start -W -S -n "$APPID/.LiveActivity" >/dev/null 2>&1 \
    || fail "could not start the live screen"
  rc=0
  env -u SMIX_TAP_HIT_MISMATCH SMIX_RUNNER_PORT="$PORT" \
    "$SMIX_RUN" --device "$SERIAL" --platform android --no-launch "$WORK/live.yaml" >"$WORK/live-$round.log" 2>&1 || rc=$?
  if [ "$rc" -ne 0 ]; then
    grep -m4 -E '^(STEP|error)|FAIL' "$WORK/live-$round.log" >&2 || tail -8 "$WORK/live-$round.log" >&2
    fail "round $round of $ROUNDS: the tap on GO LIVE did not pass (exit $rc)"
  fi
  log "  round $round: the pill was tapped, judged where it was, and the app counted it"
done

crashes="$(adb -s "$SERIAL" logcat -b crash -d 2>/dev/null | grep -c 'FATAL EXCEPTION' || true)"
[ "$crashes" = 0 ] || fail "the app crashed $crashes time(s)"

log "V14.1-C6-E2E-PASS on $SERIAL"
