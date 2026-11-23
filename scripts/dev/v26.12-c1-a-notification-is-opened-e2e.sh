#!/usr/bin/env bash
# v26.12-C1: a whole push payload reaches an app, and its notification is
# opened from wherever it is.
#
#   1. iOS: the push is refused by name while the app is missing a
#      permission, is delivered once it is allowed without a prompt, and a
#      payload that is not APNs is refused
#   2. iOS, the app not running: the older of two folded together is the one
#      opened, and the other stays
#   3. iOS, the app not running and the screen locked: the one left opens
#   4. Android: an FCM message reaches the receiver only Play services may
#      reach, with its nested data as it was sent
#   5. Android, the app not running: the oldest of three folded together is
#      the one opened, which the app says by its words
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

log()  { printf '[v26.12-c1] %s\n' "$*" >&2; }
fail() { printf '[v26.12-c1] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v26.12-c1] cannot judge: %s\n' "$*" >&2; exit 2; }

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
printf '{"aps":{"alert":{"title":"%s","body":"B"}},"body":{"a":{"b":1}}}' One >"$WORK/one.json"
printf '{"aps":{"alert":{"title":"%s","body":"B"}},"body":{"a":{"b":1}}}' Two >"$WORK/two.json"
code() { local rc=0; "$@" >"$WORK/out.log" 2>&1 || rc=$?; echo "$rc"; }
ios_push() { code "$SMIX" sim push "$UDID" --bundle "$IOS_APP" --payload "$1"; }
ios_titles() { "$SMIX" sim notification list "$UDID" --bundle "$IOS_APP" --json | python3 -c 'import json,sys; print(" ".join(n["title"] for n in json.load(sys.stdin)))'; }

log "1. iOS: permission and payload"
"$SMIX" sim permission "$UDID" "$IOS_APP" notifications deny >"$WORK/deny.log" 2>&1 || fail "1: deny: $(tail -3 "$WORK/deny.log")"
[ "$(ios_push "$WORK/one.json")" = 5 ] || fail "1: pushed to an app that may not be: $(cat "$WORK/out.log")"
"$SMIX" sim permission "$UDID" "$IOS_APP" notifications allow >"$WORK/allow.log" 2>&1 || fail "1: allow: $(tail -3 "$WORK/allow.log")"
printf '{"body":{}}' >"$WORK/bad.json"
[ "$(ios_push "$WORK/bad.json")" = 6 ] || fail "1: a payload without aps: $(cat "$WORK/out.log")"
[ "$(code "$SMIX" sim push "$UDID" --bundle com.example.none --payload "$WORK/one.json")" = 4 ] || fail "1: no such app: $(cat "$WORK/out.log")"

"$SMIX" runner up "$UDID" --bundle "$IOS_APP" --runner-port "$IOS_PORT" >"$WORK/up.log" 2>&1 \
  || fail "iOS runner up: $(tail -5 "$WORK/up.log")"
IOS_UP=1


log "2. iOS: the older of two folded together"
"$SMIX" press-key home --device "$UDID" --port "$IOS_PORT" >/dev/null 2>&1 || fail "2: home"
"$SMIX" sim terminate "$UDID" "$IOS_APP" >/dev/null 2>&1 || true
[ "$(ios_push "$WORK/one.json")" = 0 ] || fail "2: push: $(cat "$WORK/out.log")"
[ "$(ios_push "$WORK/two.json")" = 0 ] || fail "2: push: $(cat "$WORK/out.log")"
sleep 8 # the banners leave the screen
[ "$(ios_titles)" = "Two One" ] || fail "2: delivered: $(ios_titles)"
[ "$(code "$SMIX" sim notification tap "$UDID" --bundle "$IOS_APP" --title One --port "$IOS_PORT")" = 0 ] || fail "2: $(cat "$WORK/out.log")"
[ "$(ios_titles)" = "Two" ] || fail "2: left: $(ios_titles)"
"$SMIX" tree --device "$UDID" --port "$IOS_PORT" 2>/dev/null | grep -q "smix fixture" || fail "2: the app is not in front"

log "3. iOS: on the lock screen"
"$SMIX" sim terminate "$UDID" "$IOS_APP" >/dev/null 2>&1 || true
"$SMIX" sim lock "$UDID" >/dev/null || fail "3: lock"
[ "$(code "$SMIX" sim notification tap "$UDID" --bundle "$IOS_APP" --index 0 --port "$IOS_PORT")" = 0 ] || fail "3: $(cat "$WORK/out.log")"
[ -z "$(ios_titles)" ] || fail "3: left: $(ios_titles)"
"$SMIX" tree --device "$UDID" --port "$IOS_PORT" 2>/dev/null | grep -q "smix fixture" || fail "3: the app is not in front"

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
adb -s "$SERIAL" shell su 0 id -u 2>/dev/null | grep -q '^0' || cannot_judge "$SERIAL gives no root shell"
and_push() { printf '{"message":{"notification":{"title":"%s","body":"it'"'"'s $x"},"data":{"body":"{\\"type\\":\\"invoice\\"}"}}}' "$1" | "$SMIX" sim push "$SERIAL" --bundle "$AND_APP" --payload - >"$WORK/out.log" 2>&1; }
and_titles() { "$SMIX" sim notification list "$SERIAL" --bundle "$AND_APP" --json | python3 -c 'import json,sys; print(" ".join(n["title"] for n in json.load(sys.stdin)))'; }

log "4. Android: an FCM message"
"$SMIX" press-key home --device "$SERIAL" --port "$AND_PORT" >/dev/null 2>&1 || fail "4: home"
for t in One Two Three; do and_push "$t" || fail "4: $(cat "$WORK/out.log")"; done
[ "$(and_titles)" = "Three Two One" ] || fail "4: posted: $(and_titles)"
[ "$(code "$SMIX" sim push "$SERIAL" --bundle "$AND_APP" --payload "$WORK/one.json")" = 6 ] || fail "4: took an APNs payload"

log "5. Android: the oldest of three folded together"
adb -s "$SERIAL" shell am kill "$AND_APP"
sleep 8 # the heads-ups leave the screen
[ "$(code "$SMIX" sim notification tap "$SERIAL" --bundle "$AND_APP" --title One --port "$AND_PORT")" = 0 ] || fail "5: $(cat "$WORK/out.log")"
[ "$(and_titles)" = "Three Two" ] || fail "5: left: $(and_titles)"
"$SMIX" tree --device "$SERIAL" --port "$AND_PORT" >"$WORK/tree.log" 2>&1 || fail "5: tree"
grep push_opened "$WORK/tree.log" | grep -q 'gcm.notification.title.*One' || fail "5: the app did not open with it: $(grep push_opened "$WORK/tree.log")"
grep push_opened "$WORK/tree.log" | grep -q 'invoice' || fail "5: its data did not arrive: $(grep push_opened "$WORK/tree.log")"

log "V26.12-C1-A-NOTIFICATION-IS-OPENED-PASS"
