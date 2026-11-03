#!/usr/bin/env bash
# v23.0-C2: `sendPush` delivers a notification, the banner is tapped by its
# title, and the app opens what the notification carried.
#
# The fixtures read it from the app's side: a tapped notification shows
# "opened <title> → <route>" on their push screen.
#   1. iOS, the app in the background: sendPush, tapOn the banner, and the
#      app comes forward showing the route
#   2. iOS, the app in front: the banner over it is tapped the same way
#   3. Android, the app in the background, three times: the app posts the
#      heads-up from its receiver, and a tap into it lands. A heads-up still
#      sliding in drops a touch, and how long it slides grows with the
#      machine's load; on an idle machine the tap's own settling already
#      comes late enough, so this pass does not go red when the 0.7 s wait
#      for SystemUI is taken out (measured: 8 of 8 either way). That wait is
#      held by its unit test and by the timing it was measured from
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
OPENED="opened Order shipped → detail"
PUSH='- sendPush: { title: Order shipped, body: Your parcel is on its way, data: { route: detail } }'

log()  { printf '[v23.0-c2-b] %s\n' "$*" >&2; }
fail() { printf '[v23.0-c2-b] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v23.0-c2-b] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" SERIAL="" IOS_UP=0 AND_UP=0 WE_BOOTED_IOS=0 WE_BOOTED_AND=0
cleanup() {
  local said
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

# a simulator does not let smix grant notifications: the app asks, and the
# prompt is answered — once; afterwards the screen says it is allowed
cat >"$WORK/ios-allow.yaml" <<YAML
appId: $IOS_APP
---
- launchApp: { arguments: ["-push"] }
- runFlow:
    when: { visible: { id: push-permission, text: "notifications not allowed" } }
    commands:
      - tapOn: { id: push-ask }
      - tapOn: "Allow"
- extendedWaitUntil: { visible: { id: push-permission, text: "notifications allowed" }, timeout: 8000 }
YAML
"$SMIX_RUN" --device "$UDID" --runner-port "$IOS_PORT" "$WORK/ios-allow.yaml" >"$WORK/ios-allow.log" 2>&1 \
  || fail "notification permission: $(grep -m1 FAIL "$WORK/ios-allow.log")"

printf 'appId: %s\n---\n- launchApp: { arguments: ["-push"] }\n- pressKey: home\n%s\n- tapOn: "Order shipped"\n- assertVisible: { id: push-opened, text: "%s" }\n' \
  "$IOS_APP" "$PUSH" "$OPENED" >"$WORK/ios-bg.yaml"
printf 'appId: %s\n---\n- launchApp: { arguments: ["-push"] }\n- assertVisible: { id: push-opened, text: "nothing opened" }\n%s\n- tapOn: "Order shipped"\n- assertVisible: { id: push-opened, text: "%s" }\n' \
  "$IOS_APP" "$PUSH" "$OPENED" >"$WORK/ios-fg.yaml"

log "1. iOS: the banner over the home screen opens the app"
"$SMIX_RUN" --device "$UDID" --runner-port "$IOS_PORT" "$WORK/ios-bg.yaml" >"$WORK/ios-bg.log" 2>&1 \
  || fail "1: $(grep -m1 FAIL "$WORK/ios-bg.log")"
grep -q 'popup "Order shipped"' "$WORK/ios-bg.log" || fail "1: the run does not say the banner was pressed"

log "2. iOS: the banner over the app itself"
sleep 8 # the first banner leaves the screen
"$SMIX_RUN" --device "$UDID" --runner-port "$IOS_PORT" "$WORK/ios-fg.yaml" >"$WORK/ios-fg.log" 2>&1 \
  || fail "2: $(grep -m1 FAIL "$WORK/ios-fg.log")"

# --- Android
command -v adb >/dev/null 2>&1 || cannot_judge "no adb"
if ! SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tail -1)" || [ -z "$SERIAL" ]; then
  with_deadline 300 "$SMIX" sim boot "$E2E_ANDROID" --no-window >"$WORK/aboot.log" 2>&1 \
    || fail "boot: $(tail -3 "$WORK/aboot.log")"
  SERIAL="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tail -1)" || fail "$E2E_ANDROID does not resolve"
  WE_BOOTED_AND=1
fi
case "$SERIAL" in emulator-*) : ;; *) fail "$E2E_ANDROID resolves to '$SERIAL' — refusing" ;; esac
for _ in $(seq 1 60); do
  adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done
adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"
"$SMIX" runner up "$SERIAL" --platform android --bundle "$AND_APP" --runner-port "$AND_PORT" >"$WORK/aup.log" 2>&1 \
  || fail "Android runner up: $(tail -5 "$WORK/aup.log")"
AND_UP=1
# Android 16 shows no heads-up for two minutes after BOOT_COMPLETED (it
# throttles them as a burst). Uptime counts from the kernel, earlier than
# that broadcast, so the wait is on uptime with margin
up="$(adb -s "$SERIAL" shell cat /proc/uptime | cut -d. -f1)"
if [ "$up" -lt 180 ]; then
  log "   the emulator has been up ${up}s; waiting out the heads-up throttle"
  sleep $((180 - up))
fi

printf 'appId: %s\n---\n- launchApp\n- pressKey: home\n%s\n- tapOn: "Order shipped"\n- assertVisible: { id: push_opened, text: "%s" }\n' \
  "$AND_APP" "$PUSH" "$OPENED" >"$WORK/and.yaml"

log "3. Android: the heads-up opens the app, three times"
for i in 1 2 3; do
  "$SMIX_RUN" --device "$SERIAL" --runner-port "$AND_PORT" "$WORK/and.yaml" >"$WORK/and-$i.log" 2>&1 \
    || fail "3 (pass $i): $(grep -m1 FAIL "$WORK/and-$i.log")"
  sleep 7 # the heads-up leaves the screen
done

log "V23.0-C2-B-A-PUSH-AND-ITS-BANNER-PASS"
