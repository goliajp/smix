#!/usr/bin/env bash
# v14.1-C2: the tree a tap is judged from is a whole one, on a screen that
# never stops laying out.
#
# A consumer's walkthrough (2026-10-11): `tapOn id: walkthrough-advance`
# advanced the walkthrough and was reported TAP_MISSED, 2 runs in 16. The
# tree read just before the touch, to judge where it would land, held the
# screen's outer containers and the video player under them and none of the
# content the button is in; the trees read 0.1 s before and 0.4 s after were
# whole. The probe walked Compose's tree from the binder thread the question
# arrived on, and met layout passes half done — on the fixture's tour
# screen, which rebuilds forty rows every frame, about half of such reads,
# and once it ended the app.
#
# Two measurements, each able to fail on its own: reads of the tree straight
# from the probe, every one of which must hold the overlay that is on screen
# the whole time, and three taps on the tour's Next with no allowance for a
# mismatch, counted by the app.
#
# There is no SKIP path: a gate that cannot judge must be red.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
ALIAS="${SMIX_V141_C2_ANDROID:-$E2E_ANDROID}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"

log()  { printf '[v14.1-c2] %s\n' "$*" >&2; }
fail() { printf '[v14.1-c2] FAIL: %s\n' "$*" >&2; exit 1; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0 WORK=""
cleanup() {
  [ -n "$WORK" ] && rm -rf "$WORK"
  if [ "$WE_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
      >/dev/null || printf '[v14.1-c2] warning: the runner was not stopped\n' >&2
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

adb -s "$SERIAL" logcat -b crash -c >/dev/null 2>&1 || fail "could not clear the crash log"
adb -s "$SERIAL" shell am start -W -S -n "$APPID/.TourActivity" >/dev/null 2>&1 \
  || fail "could not start the tour screen"

log "--- the probe's tree, read back to back while the screen lays out"
READS=80
counted="$(adb -s "$SERIAL" shell "sleep 2; n=0; whole=0; while [ \$n -lt $READS ]; do o=\$(content call --uri content://$APPID.smixprobe --method tree 2>&1); n=\$((n+1)); case \"\$o\" in *tour-scrim*tour-advance*) whole=\$((whole+1));; esac; done; echo \$n \$whole")"
read -r n whole <<<"$counted"
log "  $whole of $n reads held the tour that was on screen throughout"
[ "$n" = "$READS" ] || fail "asked for $READS reads and the device counted '$n'"
[ "$whole" = "$READS" ] || fail "$((READS - whole)) of $READS reads of the tree were missing the tour on screen"

log "--- three taps on the tour's Next, judged with no allowance"
cat >"$WORK/tour.yaml" <<EOF
appId: $APPID
---
- tapOn:
    id: "tour-advance"
- assertVisible: "step 2 of 3"
- tapOn:
    id: "tour-advance"
- assertVisible: "step 3 of 3"
- tapOn:
    id: "tour-advance"
- assertVisible: "tour-step: 3"
- assertNotVisible:
    id: "tour-scrim"
EOF
run_flow tour
log "  the app counted three steps, and no tap was called a miss"

crashes="$(adb -s "$SERIAL" logcat -b crash -d 2>/dev/null | grep -c 'FATAL EXCEPTION' || true)"
[ "$crashes" = 0 ] || fail "the app crashed $crashes time(s) while it was read"

log "V14.1-C2-E2E-PASS on $SERIAL"
