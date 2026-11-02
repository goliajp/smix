#!/usr/bin/env bash
# v22.0-C2: `expect: { signal }` reads the app's own log on its device, live,
# and more than one source at a time.
#
# A consumer's native app writes to the iOS unified log and Android's
# logcat, and smix read only a Metro log or one file — so a flow waited a
# fixed time for an app to handle a link it showed nothing for. With
# `--app-log` the app's log is a signal source, read as it is written:
#   1. iOS: the fixture logs "live menu 1" under its bundle id when its
#      menu is tapped; the flow's signal matches it. A second source, a
#      file written by this script, is matched in the same run
#   2. Android: the fixture logs "card applied 1" to logcat; matched
#   3. without --app-log the same iOS flow fails: the line is only in the
#      app's log
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port IOS_PORT
gate_free_port AND_PORT
IOS_APP="jp.golia.smix.fixture"
AND_APP="dev.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"

log()  { printf '[v22.0-c2-c] %s\n' "$*" >&2; }
fail() { printf '[v22.0-c2-c] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v22.0-c2-c] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" SERIAL="" IOS_UP=0 AND_UP=0 WE_BOOTED_IOS=0 WE_BOOTED_AND=0 WRITER=""
cleanup() {
  local said
  [ -n "$WRITER" ] && kill "$WRITER" 2>/dev/null || true
  if [ "$IOS_UP" = 1 ] && ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" 2>&1)"; then
    log "warning: the iOS runner on $IOS_PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  if [ "$AND_UP" = 1 ] \
    && ! said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$AND_PORT" 2>&1)"; then
    log "warning: the Android runner on $AND_PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  if [ "$WE_BOOTED_IOS" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  if [ "$WE_BOOTED_AND" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

# --- iOS
UDID="$("$SMIX" sim resolve "$E2E_IOS" 2>/dev/null | tail -1 | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$E2E_IOS'"
case "$(simulator_name "$UDID")" in
  sim-smix-*) ;;
  *) cannot_judge "'$E2E_IOS' resolves to $UDID, which is not one of smix's own simulators" ;;
esac
if [ "$(simulator_state "$UDID")" != Booted ]; then
  with_deadline 300 "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED_IOS=1
fi
"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
"$SMIX" runner up "$UDID" --bundle "$IOS_APP" --runner-port "$IOS_PORT" >"$WORK/up.log" 2>&1 \
  || fail "iOS runner up: $(tail -5 "$WORK/up.log")"
IOS_UP=1

cat >"$WORK/ios.yaml" <<YAML
appId: $IOS_APP
---
- launchApp:
    stopApp: true
    arguments: ["-live"]
- extendedWaitUntil: { visible: { id: live-menu }, timeout: 15000 }
- tapOn: { id: live-menu }
- expect: { signal: { regex: "live menu 1", timeoutMs: 8000 } }
- expect: { signal: { regex: "server says ready", timeoutMs: 8000 } }
YAML

log "1. iOS: the app's own log and a file, in one run"
: >"$WORK/server.log"
( sleep 6; echo "server says ready" >>"$WORK/server.log" ) &
WRITER=$!
"$SMIX_RUN" --device "$UDID" --runner-port "$IOS_PORT" --app-log --log-file "$WORK/server.log" "$WORK/ios.yaml" \
  >"$WORK/ios.log" 2>&1 || fail "1: $(grep -m1 FAIL "$WORK/ios.log")"

log "3. without --app-log the app's line is not seen"
head -8 "$WORK/ios.yaml" >"$WORK/ios-only.yaml"
# raw run: this run's failure is what is judged
if "$SMIX" run --device "$UDID" --runner-port "$IOS_PORT" "$WORK/ios-only.yaml" >"$WORK/ios-only.log" 2>&1; then
  fail "3: the app's line was matched without --app-log"
fi
grep -q "expectSignal → FAILED" "$WORK/ios-only.log" || fail "3: failed elsewhere: $(grep -m1 FAIL "$WORK/ios-only.log")"

# --- Android
log "2. Android: the app's logcat"
command -v adb >/dev/null 2>&1 || cannot_judge "no adb"
if ! SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tail -1)" || [ -z "$SERIAL" ]; then
  with_deadline 300 "$SMIX" sim boot "$E2E_ANDROID" >"$WORK/aboot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/aboot.log")"
  SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tail -1)" || fail "$E2E_ANDROID does not resolve"
  WE_BOOTED_AND=1
fi
case "$SERIAL" in emulator-*) : ;; *) fail "$E2E_ANDROID resolves to '$SERIAL' — refusing" ;; esac
for _ in $(seq 1 60); do
  adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done
adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"
"$SMIX" runner up "$SERIAL" --platform android --runner-port "$AND_PORT" >"$WORK/aup.log" 2>&1 \
  || fail "Android runner up: $(tail -5 "$WORK/aup.log")"
AND_UP=1
adb -s "$SERIAL" shell am start -W -S -n "$AND_APP/.CardActivity" >/dev/null 2>&1 || fail "could not start the card screen"
printf 'appId: %s\n---\n- tapOn: { id: card_open }\n- tapOn: { id: card_apply }\n- expect: { signal: { regex: "card applied 1", timeoutMs: 8000 } }\n' "$AND_APP" >"$WORK/and.yaml"
"$SMIX_RUN" --device "$SERIAL" --runner-port "$AND_PORT" --app-log "$WORK/and.yaml" >"$WORK/and.log" 2>&1 \
  || fail "2: $(grep -m1 FAIL "$WORK/and.log")"

log "V22.0-C2-C-THE-APPS-OWN-LOG-PASS"
