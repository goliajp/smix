#!/usr/bin/env bash
# What is above the app, and who takes the picture — on a real emulator.
#
# Two defects reported from a consumer's Android round, 2026-09-22:
#
#   * `smix tap --then-screenshot` answered `501 not_implemented`: the
#     Android runner served no `/screenshot`, while the host asked it
#     anyway. The same capture was already being taken for OCR one
#     function away.
#   * A keyboard opened its own promotion dialog over the field being
#     filled. `inputText` failed after six seconds with a sentence about
#     focus, `/system-popups` answered `[]`, and nothing anywhere said a
#     window belonging to somebody else was on top.
#
# The subject for the second one is the runtime permission dialog: it
# belongs to `com.android.permissioncontroller`, takes the focus,
# carries three named buttons, and the app's own window leaves the
# stack entirely — the same shape as the consumer's keyboard dialog,
# and it comes up every time the permission is not held. (`am crash`
# was tried first and is not deterministic: after repeated crashes the
# system stops showing the dialog and drops to the launcher, so the
# gate would have been red about its own subject.)
#
# There is no SKIP path. A missing emulator, a missing apk, a busy port
# or a runner that will not start are all reasons this cannot judge
# anything, and a gate that cannot judge must be red rather than quiet.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
ALIAS="${SMIX_C10_ANDROID:-sim-smix-android-01}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"

log()  { printf '[c10-above] %s\n' "$*" >&2; }
step() { printf '[c10-above] --- %s\n' "$*" >&2; }
fail() { printf '[c10-above] FAIL: %s\n' "$*" >&2; exit 1; }
# Standing aside is not a failure and not a pass: the port is held
# by something this must not disturb, so there is nothing to judge.
cannot_judge() { printf '[c10-above] cannot judge: %s\n' "$*" >&2; exit 2; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0
cleanup() {
  if [ -n "$SERIAL" ]; then
    # Whatever happened above, do not leave the crash dialog sitting on
    # somebody's screen.
    adb -s "$SERIAL" shell am force-stop "$APPID" >/dev/null 2>&1 || true
  fi
  if [ "$WE_UPPED" = 1 ]; then
    if ! said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" 2>&1)"; then
      printf '[c10-above] warning: the runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
    fi
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

# Every request carries the bundle the smix client carries, because
# "is this window somebody else's" cannot be answered without it.
get()  { curl -s -m 30 -H "App-Bundle-Id: $APPID" "http://localhost:$PORT$1"; }
post() { curl -s -m 30 -H "App-Bundle-Id: $APPID" -X POST "http://localhost:$PORT$1" -d "${2:-{\}}"; }

command -v adb >/dev/null 2>&1 || fail "no adb on PATH — this judges an Android runner and cannot"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
[ -f "$APK" ] || fail "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"

SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | grep -v '^kevy:' | tr -d '[:space:]')" || true
[ -n "$SERIAL" ] || fail "no emulator registered as '$ALIAS' — register one, or set SMIX_C10_ANDROID"
case "$SERIAL" in
  emulator-*) ;;
  *) fail "'$ALIAS' resolves to $SERIAL, which is not an emulator. This drives \
a device with adb and will not do that to a phone." ;;
esac

if curl -s -m 2 "http://localhost:$PORT/health" >/dev/null 2>&1; then
  cannot_judge "port $PORT already answers — another runner is there"
fi

step "device: $ALIAS ($SERIAL)"
if ! adb devices | grep -q "^${SERIAL}[[:space:]]*device"; then
  "$SMIX" sim boot "$SERIAL" >"$WORK/boot.log" 2>&1 || fail "emulator boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi
"$SMIX" sim install "$SERIAL" "$APK" >"$WORK/install.log" 2>&1 \
  || fail "install: $(tail -3 "$WORK/install.log")"
ANDROID_SERIAL="$SERIAL" "$SMIX" runner up "$SERIAL" --platform android --runner-port "$PORT" \
  >"$WORK/up.log" 2>&1 || fail "runner up: $(tail -5 "$WORK/up.log")"
WE_UPPED=1

launch() {
  cat >"$WORK/open.yaml" <<FLOW
appId: $APPID
---
- launchApp
FLOW
  SMIX_RUNNER_PORT="$PORT" "$SMIX" run --device "$SERIAL" "$WORK/open.yaml" >"$WORK/open.log" 2>&1 \
    || fail "could not launch the fixture: $(tail -5 "$WORK/open.log")"
}

# A screen with nothing left on it from last time. The permission
# dialog outlives the app that raised it, and a leftover one makes the
# first judgement here read a screen it was not about.
adb -s "$SERIAL" shell am force-stop com.android.permissioncontroller >/dev/null 2>&1 || true
adb -s "$SERIAL" shell pm grant "$APPID" android.permission.CAMERA >/dev/null 2>&1 || true
adb -s "$SERIAL" shell am force-stop "$APPID" >/dev/null 2>&1 || true

# ---- the hand that tapped can take a picture --------------------------
step "a frame from the runner"
launch
code="$(curl -s -o "$WORK/shot.png" -w '%{http_code}' -m 30 "http://localhost:$PORT/screenshot")"
size=$(wc -c <"$WORK/shot.png" | tr -d ' ')
magic="$(head -c 8 "$WORK/shot.png" | od -An -tx1 | tr -d ' \n')"
log "  screenshot-route=$code $size bytes magic=$magic"
[ "$code" = "200" ] || fail "screenshot-route: the runner answered $code — this is the 501 the consumer met"
[ "$magic" = "89504e470d0a1a0a" ] || fail "screenshot-route: those bytes are not a PNG (magic=$magic)"
[ "$size" -gt 10000 ] || fail "screenshot-route: $size bytes is not a screen"

step "tap and frame in one command"
rm -f "$WORK/tapped.png"
SMIX_RUNNER_PORT="$PORT" "$SMIX" tap id:fixture_submit --device "$SERIAL" --port "$PORT" \
  --then-screenshot "$WORK/tapped.png" >"$WORK/tap.log" 2>&1 \
  || fail "then-screenshot: $(tail -3 "$WORK/tap.log")"
tsize=$(wc -c <"$WORK/tapped.png" | tr -d ' ')
tmagic="$(head -c 8 "$WORK/tapped.png" | od -An -tx1 | tr -d ' \n')"
log "  then-screenshot=ok $tsize bytes via $(grep -o 'frame via [a-z-]*' "$WORK/tap.log" | head -1)"
[ "$tmagic" = "89504e470d0a1a0a" ] || fail "then-screenshot: the file is not a PNG (magic=$tmagic)"
[ "$tsize" -gt 10000 ] || fail "then-screenshot: $tsize bytes on disk is not a picture of anything"

# ---- the keyboard is a foreign window and is not a popup --------------
step "the keyboard is not a popup"
post /tap-by-id '{"id":"fixture_input"}' >"$WORK/tap-input.json"
sleep 2
wins="$(get /windows)"
pops="$(get /system-popups)"
ime_seen=$(printf '%s' "$wins" | python3 -c 'import json,sys
w=json.load(sys.stdin)["windows"]
print(sum(1 for r in w if r.get("type")==2))')
pop_count=$(printf '%s' "$pops" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["popups"]))')
log "  keyboard-is-not-a-popup: input-method windows=$ime_seen popups=$pop_count"
[ "$ime_seen" -ge 1 ] || fail "the keyboard never came up, so this proves nothing about it"
[ "$pop_count" = "0" ] || fail "the keyboard was offered as a popup, and it has no button to dismiss it with"

# ---- a dialog owned by somebody else ----------------------------------
step "a dialog from another package, over the app"
# Not held, so asking raises the dialog. Revoking is also the only way
# to know the dialog is this run's and not one left on screen.
adb -s "$SERIAL" shell pm revoke "$APPID" android.permission.CAMERA >/dev/null 2>&1 || true
launch
post /tap-by-id '{"id":"fixture_ask_camera"}' >"$WORK/ask.json"
asked="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("ok"))' "$WORK/ask.json")"
[ "$asked" = "True" ] || fail "the fixture's ask-for-the-camera button was not pressed ($asked), \
so nothing raised a dialog and the rest of this would be judging an empty screen"
sleep 3
pops="$(get /system-popups)"
read -r pop_id pop_src pop_title pop_buttons <<EOF
$(printf '%s' "$pops" | python3 -c 'import json,sys
p=json.load(sys.stdin)["popups"]
if not p:
    print("- - - 0"); raise SystemExit
q=p[0]
print(q.get("id","-"), q.get("source","-") or "-", (q.get("title","-") or "-").replace(" ","_"), len(q.get("buttons",[])))')
EOF
log "  foreign-dialog-listed: id=$pop_id source=$pop_src buttons=$pop_buttons"
log "    title=$pop_title"
case "$pop_src" in
  *permissioncontroller*) ;;
  *) fail "foreign-dialog-listed: expected the permission controller's dialog, got '$pop_src' — \
this is the window that used to be reported as nothing at all" ;;
esac
[ "$pop_buttons" -ge 2 ] || fail "foreign-dialog-listed: $pop_buttons buttons — a popup nobody can press is not actionable"

step "the failure it causes names it"
said="$(post /input-text '{"text":"hello"}')"
msg="$(printf '%s' "$said" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("message",""))')"
kind="$(printf '%s' "$said" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("error",""))')"
stack="$(printf '%s' "$said" | python3 -c 'import json,sys; print(len(json.load(sys.stdin).get("windows",[])))')"
log "  focus-failure-names-it: error=$kind windows=$stack"
log "    $msg"
[ "$kind" = "no_focused_field" ] || fail "expected no_focused_field while the dialog is up, got '$kind'"
case "$msg" in
  *permissioncontroller*) ;;
  *) fail "focus-failure-names-it: the message never says who is on top — this is the consumer's \
six seconds and a screenshot, verbatim" ;;
esac
[ "$stack" -ge 1 ] || fail "focus-failure-names-it: the structured window list is empty"

step "listed so it can be pressed"
btn="$(printf '%s' "$pops" | python3 -c 'import json,sys
b=json.load(sys.stdin)["popups"][0]["buttons"]
hit=[x for x in b if "deny" in (x.get("label","")+x.get("id","")).lower()]
print((hit or b)[0]["id"])')"
post /system-popup-action "{\"popupId\":\"$pop_id\",\"buttonId\":\"$btn\"}" >"$WORK/dismiss.json"
sleep 2
after=$(get /system-popups | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["popups"]))')
log "  dismissed: pressed '$btn', popups now=$after"
[ "$after" = "0" ] || fail "the dialog is still listed after pressing '$btn' — listing it is only \
worth something if the button works"

log "C10-ABOVE-THE-APP-E2E-PASS on $SERIAL (a frame from the runner, and a window that is not the \
app's is reported both in the list and in the failure it causes)"
