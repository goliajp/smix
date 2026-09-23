#!/usr/bin/env bash
# v10.2-C6: `/back` answers whether anything went back, and the act
# routes' answers reach the host.
#
# The defect this is the instrument for, from a consumer's Android
# round (2026-09-22): `/back`
# returned `UiDevice.pressBack()`, whose bytecode is
# `sendKeyAndWaitForEvent(KEYCODE_BACK, 0, TYPE_WINDOW_CONTENT_CHANGED,
# 1000)`. That boolean is "somebody's window content changed within a
# second", which is neither the key going in nor the screen going back.
# The consumer saw it answer false while the screenshot showed the app
# had navigated; measured here it is wrong the other way too — against a
# binary built before the fix, the blocked screen below answers
# `{"ok":true}` while nothing has moved at all.
#
# Three things are checked, and the first two are the halves of one
# claim: a back that cannot happen must be refused, and a back that
# happens must not be.
#   - the blocked screen: back is swallowed by the app while a label
#     ticks every 200ms → ok:false, settledBy=gaveUp
#   - the Compose screen: back leaves it → ok:true, settledBy=screenChanged,
#     corroborated by the tree afterwards showing the main screen's own
#     button (evidence that does not come from the verdict)
#   - the routes that used to compute an answer and drop it: a tap, a
#     key, a fill, a clear and four rotations all carry `ok` now, and
#     the rotation one is read back rather than assumed — it is what
#     caught `portraitUpsideDown` never arriving.
#
# There is no SKIP path. A missing emulator, a missing apk, a busy port
# or a runner that will not start are all reasons this cannot judge
# anything, and a gate that cannot judge must be red rather than quiet
# (open-items I4).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
ALIAS="${SMIX_C6_ANDROID:-sim-smix-android-01}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
WORK="$(mktemp -d)"

log()  { printf '[c6-back] %s\n' "$*" >&2; }
step() { printf '[c6-back] --- %s\n' "$*" >&2; }
fail() { printf '[c6-back] FAIL: %s\n' "$*" >&2; exit 1; }
# Standing aside is not a failure and not a pass: the port is held
# by something this must not disturb, so there is nothing to judge.
cannot_judge() { printf '[c6-back] cannot judge: %s\n' "$*" >&2; exit 2; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0
cleanup() {
  if [ "$WE_UPPED" = 1 ]; then
    if ! said="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" 2>&1)"; then
      printf '[c6-back] warning: the runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
    fi
  fi
  # Leave the display the way it was found, whatever happened above.
  if [ -n "$SERIAL" ]; then
    adb -s "$SERIAL" shell "settings put system user_rotation 0" >/dev/null 2>&1 || true
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

post() { curl -s -m 30 -X POST "http://localhost:$PORT$1" -d "${2:-{\}}"; }

field() { # field <json> <key> — the value, or the empty string
  printf '%s' "$1" | python3 -c 'import json,sys
try:
    print(json.load(sys.stdin).get(sys.argv[1], ""))
except Exception:
    print("")' "$2"
}

command -v adb >/dev/null 2>&1 || fail "no adb on PATH — this judges an Android runner and cannot"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
[ -f "$APK" ] || fail "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"

SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | grep -v '^kevy:' | tr -d '[:space:]')" || true
[ -n "$SERIAL" ] || fail "no emulator registered as '$ALIAS' — register one, or set SMIX_C6_ANDROID"

if curl -s -m 2 "http://localhost:$PORT/health" >/dev/null 2>&1; then
  cannot_judge "port $PORT already answers — another runner is there; set SMIX_C6_PORT"
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

open_screen() { # open_screen <label>
  cat >"$WORK/open.yaml" <<FLOW
appId: $APPID
---
- launchApp
- tapOn:
    label: "$1"
FLOW
  SMIX_RUNNER_PORT="$PORT" "$SMIX" run --device "$SERIAL" "$WORK/open.yaml" >"$WORK/open.log" 2>&1 \
    || fail "could not open the '$1' screen: $(tail -5 "$WORK/open.log")"
}

# ---- a back that cannot happen ---------------------------------------
step "the screen back cannot leave"
open_screen "open-blocked"
blocked="$(post /back)"
blocked_ok="$(field "$blocked" ok)"
blocked_by="$(field "$blocked" settledBy)"
blocked_injected="$(field "$blocked" injected)"
log "  blocked-back=ok:$blocked_ok settledBy=$blocked_by injected=$blocked_injected"
log "  saw: $(field "$blocked" saw)"
[ "$blocked_ok" = "False" ] || fail "blocked-back: the app swallowed the key and this answered ok=$blocked_ok"
[ "$blocked_by" = "gaveUp" ] || fail "blocked-back: expected settledBy=gaveUp, got '$blocked_by'"
[ "$blocked_injected" = "True" ] || fail "blocked-back: the key did not go in, so this case proves nothing"

# ---- a back that does happen -----------------------------------------
step "the screen back does leave"
open_screen "open-compose"
navd="$(post /back)"
nav_ok="$(field "$navd" ok)"
nav_by="$(field "$navd" settledBy)"
log "  nav-back=ok:$nav_ok settledBy=$nav_by"
[ "$nav_ok" = "True" ] || fail "nav-back: back left the Compose screen and this answered ok=$nav_ok"
[ "$nav_by" = "screenChanged" ] || fail "nav-back: expected settledBy=screenChanged, got '$nav_by'"
# Corroboration that does not come from the verdict: the main screen's
# own button is on screen again.
if curl -s -m 30 "http://localhost:$PORT/tree" | grep -q "open-compose"; then
  log "  tree-shows-main=yes"
else
  fail "nav-back said it arrived and the main screen's button is not in the tree"
fi

# ---- the answers that used to be computed and dropped ----------------
step "an act route's answer reaches the host"
tap="$(post /tap-at-norm-coord '{"nx":0.5,"ny":0.5}')"
[ "$(field "$tap" ok)" = "True" ] || fail "tap: no ok on the wire — got $tap"
key="$(post /press-key '{"key":"tab"}')"
[ "$(field "$key" ok)" = "True" ] || fail "press-key: no ok on the wire — got $key"
fg="$(post /foreground "{\"bundleId\":\"$APPID\"}")"
[ "$(field "$fg" ok)" = "True" ] || fail "foreground: the app did not come forward — $fg"
log "  tap/press-key/foreground all carry ok"

step "a fill and a clear say what the field holds"
open_screen "open-compose"
cat >"$WORK/fill.yaml" <<FLOW
appId: $APPID
---
- tapOn:
    id: "compose_input"
- inputText: "hello c6"
- assertVisible:
    id: "compose_input"
FLOW
SMIX_RUNNER_PORT="$PORT" "$SMIX" run --device "$SERIAL" "$WORK/fill.yaml" >"$WORK/fill.log" 2>&1 \
  || fail "the fill flow failed: $(tail -5 "$WORK/fill.log")"
cleared="$(post /clear-text '{}')"
held="$(field "$cleared" held)"
log "  fill-then-clear=ok:$(field "$cleared" ok) held=$held method=$(field "$cleared" method)"
[ "$(field "$cleared" ok)" = "True" ] || fail "clear-text: the field still holds $held character(s)"
[ "$held" = "0" ] || fail "clear-text answered ok with held=$held"

# ---- the read-back that caught a verb never arriving -----------------
step "every orientation arrives where it says"
for o in portrait landscapeLeft landscapeRight portraitUpsideDown portrait; do
  r="$(post /set-orientation "{\"orientation\":\"$o\"}")"
  ok="$(field "$r" ok)"
  rot="$(field "$r" rotation)"
  log "  orientation $o → rotation $rot ok:$ok"
  [ "$ok" = "True" ] || fail "set-orientation $o: the display is at rotation $rot"
done

log "C6-BACK-E2E-PASS on $SERIAL (a back that cannot happen is refused, one that does is not, \
and every act route's answer is on the wire)"
