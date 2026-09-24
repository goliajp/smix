#!/usr/bin/env bash
# A failure says whose screen it happened on (Android).
#
# The defect: when a step failed, smix printed "visible elements (top 10)",
# and on Android those ten were always the status bar — it is a window of
# its own and the runner lists it first. A consumer read that list as the
# screen and built a detector on it that called every Android failure
# blind; the same moment read another way held the form, its name field
# and its back button. One commit to write, one to take back.
#
# Two legs, because there are two trees a failure can be read from:
#   probe — the fixture carries the semantics probe, so its tree is the
#           app's own roots. The failure must still name the app and say
#           how many elements the list was cut from.
#   a11y  — Settings carries no probe, so this is the accessibility tree
#           with the status bar's window first: the consumer's shape. The
#           first ten listed must not be system UI.
# The verdicts come from v11.1-c2-judge.py, shared with the iOS leg so both
# platforms answer the same sentence.
#
# Exit 0 judged and passed, 1 judged and failed, 2 could not judge.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/e2e-devices.sh"
ALIAS="${SMIX_C2_ANDROID:-$E2E_ANDROID}"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
JUDGE="$ROOT/scripts/dev/v11.1-c2-judge.py"
WORK="$(mktemp -d)"

log()  { printf '[c2-whose-screen] %s\n' "$*" >&2; }
fail() { printf '[c2-whose-screen] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c2-whose-screen] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

SERIAL="" WE_UPPED=0 WE_BOOTED=0
cleanup() {
  if [ -n "$SERIAL" ]; then
    adb -s "$SERIAL" shell am force-stop com.android.settings >/dev/null 2>&1 || true
  fi
  if [ "$WE_UPPED" = 1 ]; then
    local said
    if ! said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" 2>&1)"; then
      printf '[c2-whose-screen] warning: the runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
    fi
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || cannot_judge "no adb on PATH"
# An alias names an AVD, and an AVD that is not running has no serial to
# resolve to (an emulator serial is a port, not an identity). So boot by
# the alias first when it is not up, and ask for the serial afterwards.
resolve() { "$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1; }
SERIAL="$(resolve)" || true
if [ -z "$SERIAL" ]; then
  log "booting $ALIAS"
  "$SMIX" sim boot "$ALIAS" >/dev/null 2>&1 || fail "could not boot $ALIAS"
  WE_BOOTED=1
  SERIAL="$(resolve)" || true
fi
[ -n "$SERIAL" ] || cannot_judge "$ALIAS did not resolve to a running emulator, even after booting it"
case "$SERIAL" in
  emulator-*) : ;;
  *) cannot_judge "$ALIAS resolves to $SERIAL, which is not an emulator — refusing" ;;
esac
adb -s "$SERIAL" wait-for-device
for _ in $(seq 1 120); do
  adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 1
done
adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 \
  || cannot_judge "$SERIAL did not finish booting in 120 s"

[ -f "$APK" ] || fail "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"
adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"

if ! curl -s -m 5 "http://localhost:$PORT/health" >/dev/null 2>&1; then
  "$SMIX" runner up "$SERIAL" --platform android --runner-port "$PORT" >/dev/null 2>&1 \
    || fail "the runner would not start on $SERIAL:$PORT"
  WE_UPPED=1
fi

# A step that cannot pass: nothing on either screen carries this id.
failing_flow() { # $1 app id → path
  local f="$WORK/$1.yaml"
  printf 'appId: %s\n---\n- launchApp\n- assertVisible: { id: "c2_nothing_carries_this_id" }\n' "$1" > "$f"
  echo "$f"
}

run_leg() { # $1 leg, $2 app id, $3.. judge flags
  local leg="$1" app="$2"; shift 2
  local flow out rc=0
  flow="$(failing_flow "$app")"
  log "--- $leg ($app)"
  out="$(SMIX_RUNNER_PORT="$PORT" "$SMIX_RUN" --device "$SERIAL" "$flow" 2>&1)" || rc=$?
  [ "$rc" != 0 ] || fail "$leg: a step that cannot pass passed"
  printf '%s\n' "$out" > "$WORK/$leg.out"
  # The screen as the runner serves it, read right after the failure: the
  # judge takes whose each element is from here, by window, not from how
  # its id is spelled.
  curl -s -m 20 "http://localhost:$PORT/tree" > "$WORK/$leg.tree.json" \
    || cannot_judge "$leg: could not read the runner's tree after the failure"
  python3 "$JUDGE" "$leg" "$app" "$@" < "$WORK/$leg.out" >&2 \
    || FAILED="$FAILED $leg"
}

# Both legs are judged before the verdict: the first one failing must not
# hide the second one's evidence — the a11y leg is where the consumer's
# symptom lives.
FAILED=""
run_leg probe "$APPID"
run_leg a11y com.android.settings --no-system-first "$WORK/a11y.tree.json"
[ -z "$FAILED" ] || fail "the failure does not say whose screen it happened on in:$FAILED"

log "C2-WHOSE-SCREEN-E2E-PASS on $SERIAL (a failure names the app, counts what it cut, and lists the app before the status bar)"
