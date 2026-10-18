#!/usr/bin/env bash
# v16.0-C2: putting the keyboard away presses nothing, and says so when it
# cannot.
#
# The defect this is the instrument for, from a consumer's iOS 27 round
# (2026-10-14): on a sign-in step whose field is a sign-in name, the
# keyboard carries a Passwords bar above its keys, which XCUITest's
# keyboard frame leaves out. `hideKeyboard` could not drag the keyboard
# down (the form is too short to scroll), touched 24pt above the keys —
# on Passwords — and saw the keyboard leave the tree for about a second.
# It answered "dismissed"; the keyboard came back, and the flow's next
# tap on Continue was swallowed. Every step reported success.
#
# The subject is the fixture's UIKit sign-in step in the consumer's
# shape: a field in a scroll view too short to scroll, Continue pinned to
# the keyboard, `uks_result` the app's own count of submissions.
#   -uikit-signin            nothing on the screen takes the keyboard down
#   -uikit-signin-taps-away  a touch on an empty spot ends editing
#
# The field's return key is Go, the field's submit, which hideKeyboard
# used to press before anything else (a sign-in that signs itself in).
#
# On the first, hideKeyboard refuses with keyboard_did_not_close, the app
# was sent nothing, and the keyboard is still up. On the second, it says
# a touch above the keyboard took it down, the app was sent nothing, the
# keyboard stays down, and Continue then submits once.
#
# Against the 16.0.0 runner the first answers ok:true and the keyboard is
# back two seconds later.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
IOS_ALIAS="${SMIX_C2_IOS:-$E2E_IOS}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port IOS_PORT
APPID="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
PROJECT="${SMIX_C2_RUNNER_PROJECT:-$ROOT/swift-bridge/SmixRunner.xcodeproj}"
WORK="$(mktemp -d)"

log()  { printf '[v16.0-c2] %s\n' "$*" >&2; }
fail() { printf '[v16.0-c2] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v16.0-c2] SKIP: %s\n' "$*" >&2; exit 2; }

UDID="" WE_BOOTED=0 WE_UPPED=0
cleanup() {
  if [ "$WE_UPPED" = 1 ]; then
    if ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" 2>&1)"; then
      printf '[v16.0-c2] warning: the iOS runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
    fi
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v xcrun >/dev/null 2>&1 || cannot_judge "no xcrun"
UDID="$("$SMIX" sim resolve "$IOS_ALIAS" 2>/dev/null | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$IOS_ALIAS'"
[ -d "$FIXTURE" ] || fail "no iOS fixture at $FIXTURE (bash scripts/dev/build-fixture-app.sh)"

if [ "$(simulator_state "$UDID")" != Booted ]; then
  "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi
"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" runner up "$UDID" --bundle "$APPID" --runner-port "$IOS_PORT" \
  --runner-project "$PROJECT" >"$WORK/up.log" 2>&1 || fail "runner up: $(tail -5 "$WORK/up.log")"
WE_UPPED=1

# the app's tree, read for the fixture whatever the runner was last bound to
tree() { curl -s -m 60 -H "App-Bundle-Id: $APPID" "http://127.0.0.1:$IOS_PORT/tree"; }
submitted() { tree | grep -oE '"label":"submitted [^"]*"' | sed 's/"label":"//; s/"$//'; }
keyboards() { tree | grep -c '"rawType":"keyboard"' || true; }

# A cold start of the step, with the address typed and the keyboard up.
open_step() { # $1 launch argument
  cat >"$WORK/open.yaml" <<FLOW
appId: $APPID
---
- launchApp:
    clearState: true
    arguments: ["$1"]
- tapOn: { id: uks_email }
- inputText: "a@b.co"
- extendedWaitUntil:
    visible: { role: keyboard }
    timeout: 8000
FLOW
  SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX_RUN" --device "$UDID" "$WORK/open.yaml" >"$WORK/open.log" 2>&1 \
    || { tail -10 "$WORK/open.log" >&2; fail "could not open the sign-in step ($1)"; }
  local t
  t="$(tree)"
  printf '%s' "$t" | grep -q '"label":"submitted 0"' || cannot_judge "$1: the step did not open fresh"
  printf '%s' "$t" | grep -q '"rawType":"keyboard"' || cannot_judge "$1: no keyboard came up"
  # the shape this was written for: a bar with Passwords above the keys
  printf '%s' "$t" | grep -q '"label":"Passwords"' \
    || cannot_judge "$1: the keyboard has no Passwords bar, so the touch this guards against has nowhere to land"
}

hide() { # → the runner's answer
  curl -s -m 60 -X POST "http://127.0.0.1:$IOS_PORT/hide-keyboard" \
    -H 'content-type: application/json' -H "App-Bundle-Id: $APPID" -d '{"budgetMs":20000}'
}

# Long enough for a keyboard that only left the tree to come back: measured
# on iOS 27 it was gone 0.96-1.0 s after a touch on Passwords.
settle() { sleep 2; }

log "--- a step nothing on which takes the keyboard down"
open_step -uikit-signin
answer="$(hide)"
log "hideKeyboard answered: $answer"
settle
got="$(submitted)"
[ "$got" = "submitted 0" ] || fail "hideKeyboard sent the app something: '$got'"
kb="$(keyboards)"
case "$answer" in
  *'"ok":true'*)
    fail "hideKeyboard answered ok on a step that cannot put the keyboard away; keyboard elements now: $kb" ;;
  *'"error":"keyboard_did_not_close"'*) : ;;
  *) fail "an answer that is neither: $answer" ;;
esac
[ "$kb" -gt 0 ] || fail "hideKeyboard said the keyboard stayed, and it is gone"
case "$answer" in
  *'focus: uks_email'*) : ;;
  *) fail "the refusal does not name the field still holding focus: $answer" ;;
esac
# the field's return key is Go: the one way it lets go, and not pressed
case "$answer" in
  *'return key: Go'*) : ;;
  *) fail "the refusal does not name the return key it did not press: $answer" ;;
esac
log "refused, named the field and its return key, sent the app nothing, and the keyboard is still up"

log "--- a step where a touch on an empty spot takes the keyboard down"
open_step -uikit-signin-taps-away
answer="$(hide)"
log "hideKeyboard answered: $answer"
settle
got="$(submitted)"
[ "$got" = "submitted 0" ] || fail "hideKeyboard sent the app something: '$got'"
case "$answer" in
  *'"ok":true'*'dismissed by tap-above-keyboard'*) : ;;
  *) fail "expected a touch above the keyboard to take it down: $answer" ;;
esac
kb="$(keyboards)"
[ "$kb" = 0 ] || fail "hideKeyboard said the keyboard went down, and two seconds later it is up ($kb)"
cat >"$WORK/continue.yaml" <<FLOW
appId: $APPID
---
- tapOn: { id: uks_continue }
FLOW
SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX_RUN" --device "$UDID" "$WORK/continue.yaml" >"$WORK/continue.log" 2>&1 \
  || { tail -10 "$WORK/continue.log" >&2; fail "tapOn Continue failed"; }
got="$(submitted)"
[ "$got" = "submitted 1: button" ] || fail "Continue after hideKeyboard: '$got'"
log "put away by a touch on an empty spot, sent the app nothing, stayed down, and Continue then submitted once"

log "V16.0-C2-E2E-PASS"
