#!/usr/bin/env bash
# `runner up` puts the screen right before it answers, and only ever for
# an app somebody named.
#
# A consumer's round died at its first step after every `adb install` and
# after every round that left the notification shade down. They wrote the
# recovery into their own runner: take the runner down, collapse the
# shade, foreground the app, wait eight seconds, ask again. The shade half
# was also misdiagnosed here: with it down, `runner up` said the runner's
# accessibility connection had fallen behind and recommended cycling a
# runner that was fine.
#
# Each shape is built, then checked to be there, then judged — by what
# the device says (`dumpsys activity activities`, the runner's own
# `/windows`), never by what smix prints about itself:
#   A. runner up, shade pulled down           → it puts the shade away
#   B. the app killed (what an install does)  → `--bundle` brings it back
#   C. the home screen in front, no `--bundle` → it stays in front
#   D. `--bundle` of a package not installed  → refused, naming it
#   E. fresh bring-up, shade down, `--bundle` → up, app in front
#   iOS: Settings in front, `runner up --bundle` → the fixture's own
#        counter moves when tapped (the same sentence on both platforms)
#
# Exit 0 judged and passed, 1 judged and failed, 2 could not judge.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
gate_free_port IOS_PORT

ALIAS="${SMIX_C5_ANDROID:-sim-smix-android-01}"
IOS_ALIAS="${SMIX_C5_IOS:-sim-smix-02}"
APPID="dev.smix.fixture"
IOS_APPID="jp.golia.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"

log()  { printf '[c5-back] %s\n' "$*" >&2; }
step() { log "--- $*"; }
fail() { printf '[c5-back] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c5-back] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

SERIAL="" UDID="" WE_BOOTED=0 IOS_WE_BOOTED=0 UPPED=0 IOS_UPPED=0 WORK="$(mktemp -d)"
cleanup() {
  if [ "$UPPED" = 1 ]; then
    with_deadline 60 "$SMIX" runner down --platform android --device "$SERIAL" \
      --runner-port "$PORT" >/dev/null 2>&1 || log "warning: the Android runner was not stopped"
  fi
  if [ -n "$SERIAL" ]; then
    with_deadline 10 adb -s "$SERIAL" shell cmd statusbar collapse >/dev/null 2>&1 || true
  fi
  if [ "$WE_BOOTED" = 1 ]; then with_deadline 60 "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  if [ "$IOS_UPPED" = 1 ]; then
    with_deadline 60 "$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" >/dev/null 2>&1 \
      || log "warning: the iOS runner was not stopped"
  fi
  if [ "$IOS_WE_BOOTED" = 1 ]; then with_deadline 60 "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || cannot_judge "no adb on PATH"
[ -f "$APK" ] || cannot_judge "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"

# The device by alias first, so the ledger records that smix booted it and
# which AVD it is; the serial only after — it is the port it took today.
if ! SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | grep -v '^kevy:' | tail -1)" || [ -z "$SERIAL" ]; then
  log "booting $ALIAS"
  with_deadline 300 "$SMIX" sim boot "$ALIAS" >"$WORK/boot.log" 2>&1 \
    || cannot_judge "could not boot $ALIAS: $(tail -3 "$WORK/boot.log" | tr '\n' ' ')"
  WE_BOOTED=1
  SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | grep -v '^kevy:' | tail -1)"
fi
case "$SERIAL" in emulator-*) ;; *) cannot_judge "$ALIAS resolved to '$SERIAL', which is not an emulator" ;; esac
avd="$(with_deadline 10 adb -s "$SERIAL" emu avd name 2>/dev/null | head -1 | tr -d '\r')"
[ "$avd" = "$ALIAS" ] || cannot_judge "$SERIAL is '$avd', not $ALIAS — refusing to drive somebody else's emulator"
for i in $(seq 1 60); do
  [ "$(with_deadline 5 adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && break
  sleep 2
done
with_deadline 120 "$SMIX" sim install "$SERIAL" "$APK" >"$WORK/install.log" 2>&1 \
  || cannot_judge "fixture install: $(tail -3 "$WORK/install.log" | tr '\n' ' ')"

# --- what the device says ------------------------------------------------
resumed() {
  with_deadline 10 adb -s "$SERIAL" shell dumpsys activity activities 2>/dev/null \
    | tr -d '\r' | grep -m1 'ResumedActivity:' | sed -E 's#.* ([^ /]+)/[^ ]*}.*#\1#'
}
app_window_readable() { # the runner's own window list has a readable app window
  local body
  body="$(curl -s -m 5 "http://localhost:$PORT/windows")" || return 1
  printf '%s' "$body" | python3 -c 'import json,sys
d=json.load(sys.stdin)
sys.exit(0 if any(w.get("type")==1 and w.get("rootReadable") for w in d.get("windows",[])) else 1)'
}
resumed_is() { # resumed_is <package> <seconds>
  local i
  for i in $(seq 1 $(( $2 * 2 ))); do [ "$(resumed)" = "$1" ] && return 0; sleep 0.5; done
  return 1
}
shade_down() {
  local i
  with_deadline 10 adb -s "$SERIAL" shell cmd statusbar expand-notifications >/dev/null 2>&1
  for i in $(seq 1 10); do app_window_readable || return 0; sleep 0.5; done
  cannot_judge "pulled the shade down and the runner still reads an app window — the subject was not built"
}
up() { # up <args...> — runner up on this device, output to $WORK/up.out, exit code to $UP_RC
  UP_RC=0
  with_deadline 300 "$SMIX" runner up "$SERIAL" --platform android --runner-port "$PORT" "$@" \
    >"$WORK/up.out" 2>&1 || UP_RC=$?
  grep -v '^kevy:' "$WORK/up.out" >&2 || true
  [ "$UP_RC" = "$DEADLINE_STATUS" ] && cannot_judge "runner up did not return in 300 s"
  return 0
}
said() { grep -q -- "$1" "$WORK/up.out"; }

if curl -s -m 2 "http://localhost:$PORT/health" >/dev/null 2>&1; then
  cannot_judge "port $PORT already answers — another runner is there"
fi

step "bring the runner up on $SERIAL ($ALIAS)"
up
[ "$UP_RC" = 0 ] || cannot_judge "the first runner up failed — nothing below could be judged"
UPPED=1

step "A. runner up while the shade covers the screen"
with_deadline 10 adb -s "$SERIAL" shell am start -n "$APPID/.MainActivity" >/dev/null 2>&1
resumed_is "$APPID" 10 || cannot_judge "A: $APPID did not come to the front — the subject was not built"
shade_down
up
[ "$UP_RC" = 0 ] || fail "A: runner up refused a screen covered by the shade (exit $UP_RC) — see above"
said "notification shade was over the screen" || fail "A: it answered without saying it put the shade away"
app_window_readable || fail "A: runner up returned and the runner still reads no app window"
log "A=collapsed (runner reads an app window again)"

step "B. the app killed, then runner up --bundle $APPID"
with_deadline 10 adb -s "$SERIAL" shell am force-stop "$APPID" >/dev/null 2>&1
[ "$(resumed)" != "$APPID" ] || cannot_judge "B: force-stop left $APPID resumed — the subject was not built"
before="$(resumed)"
up --bundle "$APPID"
[ "$UP_RC" = 0 ] || fail "B: runner up --bundle $APPID failed (exit $UP_RC)"
now="$(resumed)"
[ "$now" = "$APPID" ] || fail "B: after runner up --bundle the device resumes '$now', not $APPID"
said "brought $APPID forward" || fail "B: it brought the app back without saying so"
# The shade was never down here. A screen between two apps once read as
# a covered one, and the bring-up announced putting away a shade that
# did not exist.
! said "notification shade" || fail "B: it says it put a shade away, and no shade was down"
log "B=brought-back ($before → $now)"

step "C. the home screen in front, runner up without --bundle"
# Not a system app driven by its ids — which ones exist depends on the
# emulator image. The home screen is an app like any other to this
# question, and which package it is comes from the device.
HOME_PKG="$(with_deadline 10 adb -s "$SERIAL" shell cmd package resolve-activity --brief \
  -a android.intent.action.MAIN -c android.intent.category.HOME 2>/dev/null | tr -d '\r' | tail -1 | cut -d/ -f1)"
[ -n "$HOME_PKG" ] || cannot_judge "C: the device names no home activity"
with_deadline 10 adb -s "$SERIAL" shell input keyevent KEYCODE_HOME >/dev/null 2>&1
resumed_is "$HOME_PKG" 10 || cannot_judge "C: $HOME_PKG did not come to the front — the subject was not built"
up
[ "$UP_RC" = 0 ] || fail "C: runner up failed with $HOME_PKG in front (exit $UP_RC)"
now="$(resumed)"
[ "$now" = "$HOME_PKG" ] || fail "C: runner up without --bundle moved the front from $HOME_PKG to '$now' — an app nobody named"
log "C=left-alone ($now)"

step "D. runner up --bundle of a package that is not installed"
up --bundle com.example.nosuchapp
[ "$UP_RC" != 0 ] || fail "D: runner up --bundle com.example.nosuchapp exited 0"
said "com.example.nosuchapp" || fail "D: the refusal does not name the package"
said "not installed" || fail "D: the refusal does not say what is wrong with it"
log "D=refused (exit $UP_RC)"

step "E. a fresh bring-up with the shade down and --bundle"
with_deadline 60 "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
  >/dev/null 2>&1 || cannot_judge "E: could not take the runner down to bring it up fresh"
UPPED=0
with_deadline 10 adb -s "$SERIAL" shell cmd statusbar expand-notifications >/dev/null 2>&1
up --bundle "$APPID"
UPPED=1
[ "$UP_RC" = 0 ] || fail "E: a fresh runner up with the shade down failed (exit $UP_RC)"
now="$(resumed)"
[ "$now" = "$APPID" ] || fail "E: after a fresh runner up --bundle the device resumes '$now'"
app_window_readable || fail "E: the fresh runner reads no app window"
log "E=up-and-in-front ($now)"

# --- iOS: the same sentence ----------------------------------------------
step "iOS: Settings in front, runner up --bundle $IOS_APPID"
if ! UDID="$("$SMIX" sim resolve "$IOS_ALIAS" 2>/dev/null | grep -v '^kevy:' | tail -1)" || [ -z "$UDID" ]; then
  cannot_judge "no iOS simulator registered as $IOS_ALIAS"
fi
ios_state() {
  xcrun simctl list devices -j | python3 -c 'import json,sys
u=sys.argv[1]
for ds in json.load(sys.stdin)["devices"].values():
    for d in ds:
        if d["udid"] == u: print(d["state"])' "$UDID"
}
if [ "$(ios_state)" != "Booted" ]; then
  with_deadline 300 "$SMIX" sim boot "$UDID" >/dev/null 2>&1 || cannot_judge "could not boot $IOS_ALIAS"
  IOS_WE_BOOTED=1
fi
with_deadline 30 xcrun simctl launch "$UDID" com.apple.Preferences >/dev/null 2>&1 \
  || cannot_judge "could not put Settings in front on $UDID"
with_deadline 600 "$SMIX" runner up "$UDID" --bundle "$IOS_APPID" --runner-port "$IOS_PORT" \
  >"$WORK/ios-up.out" 2>&1 || fail "iOS: runner up --bundle failed: $(tail -3 "$WORK/ios-up.out" | tr '\n' ' ')"
IOS_UPPED=1
count() {
  local tree
  tree="$("$SMIX" tree --device "$UDID" --port "$IOS_PORT" --reader a11y --json 2>/dev/null | grep -v '^kevy:')" || return 1
  TREE_JSON="$tree" python3 - <<'PY'
import json, os, re, sys
def walk(n):
    if (n.get("identifier") or "").split("/")[-1] == "fixture-icon-count":
        m = re.search(r"(\d+)", " ".join(str(n.get(k) or "") for k in ("text", "label", "value")))
        if m:
            print(m.group(1)); sys.exit(0)
    for c in n.get("children", []):
        walk(c)
# `smix tree --json` answers {"source", "root"}: the reader travels with
# the tree. Walking from the envelope finds nothing and reads as "not on
# the screen".
d = json.loads(os.environ["TREE_JSON"])
walk(d.get("root", d))
sys.exit(1)
PY
}
b="$(count)" || fail "iOS: after runner up --bundle the fixture's counter is not on the screen"
printf 'appId: %s\n---\n- tapOn: { label: Pause }\n' "$IOS_APPID" >"$WORK/ios-tap.yaml"
SMIX_RUNNER_PORT="$IOS_PORT" with_deadline 120 "$SMIX" run --device "$UDID" "$WORK/ios-tap.yaml" \
  >"$WORK/ios-tap.log" 2>&1 || fail "iOS: the tap on the fixture did not run: $(tail -3 "$WORK/ios-tap.log" | tr '\n' ' ')"
sleep 1
a="$(count)" || fail "iOS: the fixture's counter went away after the tap"
[ "$(( a - b ))" = 1 ] || fail "iOS: the fixture's own counter moved by $(( a - b )), not 1 — it was not the app in front"
log "iOS=fixture-in-front (its counter $b → $a)"

log "C5-APP-BACK-E2E-PASS on $SERIAL and $UDID"
