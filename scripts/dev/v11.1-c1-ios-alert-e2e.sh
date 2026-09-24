#!/usr/bin/env bash
# v11.1-C1, the iOS half: a tap on an alert's confirm presses it.
#
# A consumer wrote, beside a flow that confirms their app's own alert:
# "Matched by its words an ordinary tap finds the button and reports
# success without pressing it", and routes that tap through
# `dispatch: 'xcui'` instead. On Android the same shape was a real
# defect and it is fixed in this checkpoint (see
# v11.1-c1-a-press-that-lands-e2e.sh): the touch landed below the dialog.
#
# On iOS it DID NOT REPRODUCE. Measured 2026-09-24 on sim-smix-02
# (iOS 26.5 runtime, Xcode 27.0, smix at feature/v11.1): the fixture's
# SwiftUI `.alert` — a `UIAlertController` underneath, the same thing
# React Native's `Alert.alert` presents — confirmed by `text:Delete` and
# by `id:fixture-alert-confirm`, nine presses across two conditions (1.5 s
# after the alert opened, and immediately after), and the app counted
# nine. smix's chain put every touch on the confirm button.
#
# So this is a guard, not a fix: the verdict is not changed on the
# strength of a report nobody here could reproduce (the rule C11 set).
# What it does hold is the shape the consumer relies on — an alert's
# confirm, aimed by its words, is pressed, and smix says so — so that
# the day it stops being true on some runtime, something goes red
# instead of a consumer's flow reading a dismissal as a confirmation.
#
# Judged by the app's own count, never by the alert disappearing: a
# confirmed alert and a dismissed one are both gone.
#
# Three codes, per the contract: 0 judged and right, 1 judged and wrong
# or the setup this run owns did not come up, 2 could not judge.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
UDID="${1:-${SMIX_E2E_UDID:-}}"
APP="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"

log()  { printf '[c1-ios-alert] %s\n' "$*" >&2; }
fail() { printf '[c1-ios-alert] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c1-ios-alert] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

[ -n "$UDID" ] || cannot_judge "usage: $0 <simulator udid> (or SMIX_E2E_UDID)"
xcrun simctl list devices booted 2>/dev/null | grep -q "$UDID" \
  || cannot_judge "$UDID is not a booted simulator"
[ -d "$FIXTURE" ] || fail "no fixture app — run: bash scripts/dev/build-fixture-app.sh"

WE_UPPED=0
cleanup() {
  if [ "$WE_UPPED" = 1 ]; then
    local said
    if ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$PORT" 2>&1)"; then
      printf '[c1-ios-alert] warning: the runner on %s:%s was not stopped:\n%s\n' \
        "$UDID" "$PORT" "$(printf '%s' "$said" | tail -3)" >&2
    fi
  fi
}
trap cleanup EXIT

xcrun simctl install "$UDID" "$FIXTURE" || fail "could not install the fixture"
"$SMIX" runner up "$UDID" --bundle "$APP" --runner-port "$PORT" --force >/dev/null 2>&1 \
  || fail "the runner would not start on $UDID:$PORT"
WE_UPPED=1
"$SMIX" sim launch "$UDID" "$APP" >/dev/null 2>&1 || fail "could not launch the fixture"

count() {
  local tree
  tree="$("$SMIX" tree --device "$UDID" --port "$PORT" --json 2>/dev/null)" || return 1
  TREE_JSON="$tree" python3 - <<'PY'
import json, os, re, sys
d = json.loads(os.environ["TREE_JSON"])
def walk(n):
    if n.get("identifier") == "fixture-alert-count":
        m = re.search(r"(\d+)", n.get("label") or n.get("text") or "")
        if m:
            print(m.group(1)); sys.exit(0)
    for c in n.get("children", []):
        walk(c)
walk(d["root"])
sys.exit(1)
PY
}

ready=0
for _ in $(seq 1 30); do
  count >/dev/null 2>&1 && { ready=1; break; }
  sleep 1
done
[ "$ready" = 1 ] || fail "the fixture's alert count never became readable on $UDID"

press() { # $1 selector, $2 seconds to wait after the alert opens, $3 label
  local before after rc=0 said
  before="$(count)" || fail "$3: could not read the count before"
  "$SMIX" tap "id:fixture-open-alert" --device "$UDID" --port "$PORT" >/dev/null 2>&1 \
    || fail "$3: could not open the alert"
  [ "$2" = 0 ] || sleep "$2"
  said="$("$SMIX" tap "$1" --device "$UDID" --port "$PORT" 2>&1)" || rc=$?
  sleep 1.5
  after="$(count)" || fail "$3: could not read the count after"
  [ "$after" = $((before + 1)) ] \
    || fail "$3: the app counted $before then $after — the confirm was not pressed. smix said (exit $rc): $said"
  [ "$rc" = 0 ] || fail "$3: the confirm was pressed and smix reported a failure (exit $rc): $said"
  log "  $3: pressed (the app's count went $before → $after)"
}

press "text:Delete" 1.5 "by its words, after the alert settled"
press "text:Delete" 0 "by its words, as soon as it appeared"
press "id:fixture-alert-confirm" 0 "by its id, as soon as it appeared"

log "C1-IOS-ALERT-E2E-PASS on $UDID (an alert's confirm, aimed by its words or its id, is pressed — the consumer's report did not reproduce here)"
