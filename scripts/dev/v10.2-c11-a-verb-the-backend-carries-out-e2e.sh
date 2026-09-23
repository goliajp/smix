#!/usr/bin/env bash
# `smix sim` carries out on an Android device what the Android backend
# implements, and says what an install did to the app it replaced.
#
# The defect this is the instrument for (a consumer's Android round,
# 2026-09-22): on a registered handset, `smix sim launch` after an
# install answered "this command runs through simctl … Android lifecycle
# goes through adb — `smix runner up …`". The verb was refused although
# `AndroidDeviceControl::launch` had run `am start` the whole time, and
# the refusal carried advice about a different verb, so it read as a
# report that the install had taken the runner down. Measured here the
# same day: the install does not touch the runner (/health stays 200); it
# ends the app under test, and nothing said so.
#
# The judgements are the verbs' own exit codes and the device's own
# answers (`smix sim frontmost` reads `dumpsys activity`), never this
# script's opinion of them.
#
# There is no SKIP path. A missing emulator, a missing apk, a busy port
# or a runner that will not start are all reasons this cannot judge
# anything, and a gate that cannot judge must be red rather than quiet.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
ALIAS="${SMIX_C11_ANDROID:-sim-smix-android-01}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"

log()  { printf '[c11-verb] %s\n' "$*" >&2; }
step() { printf '[c11-verb] --- %s\n' "$*" >&2; }
fail() { printf '[c11-verb] FAIL: %s\n' "$*" >&2; exit 1; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0
cleanup() {
  if [ "$WE_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
      >/dev/null 2>&1 || printf '[c11-verb] warning: the runner was not stopped\n' >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || fail "no adb on PATH — this judges an Android device and cannot"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
[ -f "$APK" ] || fail "no fixture apk at $APK (assembleDebug in test-fixtures/android-app)"

SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | grep -v '^kevy:' | tail -1)"
if [ -z "$SERIAL" ]; then
  log "nothing is registered as $ALIAS"
  # 2, not 0: nothing was judged. A run that could not look and a run
  # that looked and liked what it saw must not end the same way.
  exit 2
fi

# Never a phone. This installs over whatever is running and force-stops
# an app; neither belongs on somebody's own handset.
case "$SERIAL" in
  emulator-[0-9]*) ;;
  *)
    log "$ALIAS resolves to $SERIAL, which is not an emulator — refusing."
    exit 2
    ;;
esac

if ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  log "booting $ALIAS"
  "$SMIX" sim boot "$ALIAS" >/dev/null 2>&1 || fail "could not boot $ALIAS"
  WE_BOOTED=1
  adb -s "$SERIAL" wait-for-device
fi
step "device: $ALIAS ($SERIAL)"

# Run a `smix sim` verb, keeping its exit code and its words apart.
#
# Not `cmd | grep`: the pipeline's status is the last stage's, so a verb
# that failed reads as success and the judgement below is made on the
# device's state alone. A sibling gate shipped with exactly that.
say=""
smix_sim() {
  local status=0
  say="$("$SMIX" sim "$@" 2>&1)" || status=$?
  say="$(printf '%s\n' "$say" | grep -v '^kevy:' || true)"
  return $status
}

frontmost_package() {
  smix_sim frontmost "$ALIAS" || fail "frontmost: $say"
  printf '%s' "$say" | sed -n 's/^frontmost: \([^ ]*\).*/\1/p' | tail -1
}

step "a runner is up and driving the app"
smix_sim install "$ALIAS" "$APK" || fail "install: $say"
if ! "$SMIX" runner up "$ALIAS" --platform android --runner-port "$PORT" >"$WORK/up.log" 2>&1; then
  fail "the runner would not start on $SERIAL:$PORT — $(tail -2 "$WORK/up.log")"
fi
WE_UPPED=1
cat > "$WORK/drive.yaml" <<YAML
appId: $APPID
---
- launchApp
- assertVisible: { id: fixture_input }
YAML
SMIX_RUNNER_PORT="$PORT" "$SMIX" run --device "$SERIAL" "$WORK/drive.yaml" >"$WORK/run.log" 2>&1 \
  || fail "the flow that proves the app is being driven failed: $(tail -3 "$WORK/run.log")"
log "  drive=ok"

step "an install over the running app says what it stopped"
smix_sim install "$ALIAS" "$APK" || fail "install over a running app: $say"
printf '%s\n' "$say" | grep -q "$APPID was running and this reinstall stopped it" \
  || fail "the install did not say it stopped $APPID: $say"
log "  install-names-what-it-stopped=yes"
# And the runner is untouched by it — the half the consumer had read the
# other way round.
health="$(curl -s -m 5 -o /dev/null -w '%{http_code}' "http://localhost:$PORT/health" || true)"
[ "$health" = "200" ] || fail "the runner stopped answering after an install (/health=$health)"
log "  runner-survives-install=yes (/health=200)"

step "the verbs the backend carries out are carried out"
smix_sim launch "$ALIAS" "$APPID" || fail "launch: $say"
sleep 2
front="$(frontmost_package)"
[ "$front" = "$APPID" ] || fail "launch returned 0 and $APPID is not in front (front=$front)"
log "  launch=ok front=$front"

smix_sim terminate "$ALIAS" "$APPID" || fail "terminate: $say"
sleep 1
front="$(frontmost_package)"
[ "$front" != "$APPID" ] || fail "terminate returned 0 and $APPID is still in front"
log "  terminate=ok front=$front"

smix_sim openurl "$ALIAS" "https://example.com" || fail "openurl: $say"
log "  openurl=ok"

step "a launch environment Android has no concept of is refused by name"
if smix_sim launch "$ALIAS" "$APPID" --child-env SMIX_E2E=1; then
  fail "--child-env was accepted on Android, where an activity cannot be given one: $say"
fi
printf '%s\n' "$say" | grep -q -- "--child-env" \
  || fail "the refusal does not name the flag it refused: $say"
printf '%s\n' "$say" | grep -qi "launch argument" \
  || fail "the refusal leaves the caller nowhere to go: $say"
log "  child-env-refused-by-name=yes"

printf '[c11-verb] C11-VERB-E2E-PASS on %s (launch, terminate and openurl are carried out; an install names the app it stopped; the runner survives it)\n' "$SERIAL"
