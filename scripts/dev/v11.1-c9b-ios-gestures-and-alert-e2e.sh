#!/usr/bin/env bash
# v11.1-C9b, iOS: a double tap and a long press arrive, and a UIKit
# alert's `Delete` is pressed the way a consumer's flow presses it.
#
# T1. iOS double tap and long press used to go to `/double-tap` and
# `/long-press`: XCUI element actions resolved inside the runner, which
# answered `ok` and nothing about where the touch went. They now go
# where a tap goes (host-resolved, `/tap-at-norm-coord`, judged by what
# was under the point). The verdict here is the app's own counts —
# `double taps N` and `held N` — never smix's word.
#
# T2. A consumer's `clear-cache.yaml` confirms React Native's
# `Alert.alert` (a `UIAlertController`) with, in this order and with
# nothing between: wait for the message, an optional XCUI tap on an id
# the alert does not carry, then `tapOn: { text: 'Delete', optional: true }`.
# They reported the tap as succeeding without pressing. C1 measured a
# SwiftUI `.alert` and could not reproduce it. The fixture now raises a
# UIKit alert with their message, their button styles and their order,
# and the flow below is theirs step for step. Twenty runs; each one's
# count and smix's own line for the Delete step are written out raw
# whatever the verdict, so a reproduction is evidence and a
# non-reproduction is a record rather than a shrug.
#
# Exit: 0 judged and passed, 1 judged and failed, 2 could not judge.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/gate-port.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/e2e-devices.sh"
PORT="$SMIX_RUNNER_PORT"
TARGET="${1:-${SMIX_C9B_IOS:-$E2E_IOS}}"
RUNS="${SMIX_C9B_ALERT_RUNS:-20}"
APPID="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
WORK="$(mktemp -d)"
RAW="${SMIX_C9B_RAW:-$WORK/t2-raw.tsv}"
MESSAGE="You are about to remove all credentials from this device. This action cannot be undone."

log()  { printf '[c9b-ios] %s\n' "$*" >&2; }
fail() { printf '[c9b-ios] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c9b-ios] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

UDID="" WE_BOOTED=0 WE_UPPED=0
cleanup() {
  local said
  if [ "$WE_UPPED" = 1 ]; then
    said="$("$SMIX" runner down --device "$UDID" --runner-port "$PORT" 2>&1)" \
      || printf '[c9b-ios] warning: the runner on %s:%s was not stopped:\n%s\n' \
        "$UDID" "$PORT" "$(printf '%s' "$said" | tail -3)" >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -d "$FIXTURE" ] || cannot_judge "no fixture app — run: bash scripts/dev/build-fixture-app.sh"
UDID="$("$SMIX" sim resolve "$TARGET" 2>/dev/null | tail -1)" || true
[ -n "$UDID" ] || cannot_judge "no simulator registered as $TARGET"
if ! xcrun simctl list devices booted 2>/dev/null | grep -q "$UDID"; then
  log "booting $TARGET"
  with_deadline 300 "$SMIX" sim boot "$UDID" >/dev/null 2>&1 || cannot_judge "could not boot $TARGET"
  WE_BOOTED=1
fi
with_deadline 180 "$SMIX" sim install "$UDID" "$FIXTURE" >/dev/null 2>&1 || fail "could not install the fixture"
with_deadline 600 "$SMIX" runner up "$UDID" --bundle "$APPID" --runner-port "$PORT" --force >/dev/null 2>&1 \
  || cannot_judge "the runner would not start on $UDID:$PORT"
WE_UPPED=1

number_of() { # number_of <identifier> — the first number in its label
  local tree
  tree="$(with_deadline 60 "$SMIX" tree --device "$UDID" --port "$PORT" --json 2>/dev/null)" || return 1
  TREE_JSON="$tree" python3 - "$1" <<'PY'
import json, os, re, sys
d = json.loads(os.environ["TREE_JSON"])
def walk(n):
    if n.get("identifier") == sys.argv[1]:
        m = re.search(r"(\d+)", n.get("label") or n.get("text") or n.get("value") or "")
        if m:
            print(m.group(1)); sys.exit(0)
    for c in n.get("children", []):
        walk(c)
walk(d["root"])
sys.exit(1)
PY
}

run_flow() { # run_flow <file> — sets OUT and RC
  RC=0
  OUT="$(SMIX_RUNNER_PORT="$PORT" with_deadline 180 "$SMIX_RUN" --device "$UDID" "$1" 2>&1)" || RC=$?
}

fresh() {
  # Not running is fine: this only makes the launch below a fresh one.
  with_deadline 30 xcrun simctl terminate "$UDID" "$APPID" >/dev/null 2>&1 || true
  with_deadline 60 "$SMIX" sim launch "$UDID" "$APPID" >/dev/null 2>&1 \
    || fail "could not launch the fixture"
  local ok=0
  for _ in $(seq 1 30); do
    number_of fixture-doubletap >/dev/null 2>&1 && { ok=1; break; }
    sleep 1
  done
  [ "$ok" = 1 ] || fail "the fixture's counters never became readable on $UDID"
}

# ---- T1: double tap ---------------------------------------------------
fresh
before="$(number_of fixture-doubletap)" || fail "double tap: no count before"
cat >"$WORK/double.yaml" <<FLOW
appId: $APPID
---
- doubleTapOn:
    id: fixture-doubletap
FLOW
run_flow "$WORK/double.yaml"
sleep 1
after="$(number_of fixture-doubletap)" || fail "double tap: no count after"
log "T1 double tap: app count $before → $after, smix exit $RC"
[ "$RC" = 0 ] || fail "double tap: smix failed (exit $RC): $(printf '%s' "$OUT" | tail -4)"
[ "$after" = $((before + 1)) ] || fail "double tap: the app counted $before then $after"

# ---- T1: long press ---------------------------------------------------
before="$(number_of fixture-longpress-count)" || fail "long press: no count before"
cat >"$WORK/long.yaml" <<FLOW
appId: $APPID
---
- longPressOn:
    id: fixture-longpress
    duration: 1000
FLOW
run_flow "$WORK/long.yaml"
sleep 1
after="$(number_of fixture-longpress-count)" || fail "long press: no count after"
log "T1 long press: app count $before → $after, smix exit $RC"
[ "$RC" = 0 ] || fail "long press: smix failed (exit $RC): $(printf '%s' "$OUT" | tail -4)"
[ "$after" = $((before + 1)) ] || fail "long press: the app counted $before then $after"

# ---- T2: the consumer's confirm, their way, $RUNS times --------------
cat >"$WORK/confirm.yaml" <<FLOW
appId: $APPID
---
- tapOn:
    id: fixture-open-uikit-alert
- extendedWaitUntil:
    visible:
      {
        text: '$MESSAGE',
      }
    timeout: 20000
- tapOn: { dispatch: 'xcui', id: 'btn-clear-cache-confirm', optional: true }
- tapOn: { text: 'Delete', optional: true }
FLOW
printf 'run\tbefore\tafter\tpressed\tsmix_exit\tsmix_steps_3_4\n' >"$RAW"
fresh
missed=0
for i in $(seq 1 "$RUNS"); do
  before="$(number_of fixture-uikit-alert-count)" || fail "T2 run $i: no count before"
  run_flow "$WORK/confirm.yaml"
  sleep 1.5
  after="$(number_of fixture-uikit-alert-count)" || fail "T2 run $i: no count after (smix: $(printf '%s' "$OUT" | tail -3))"
  # How steps 3 (the XCUI tap on an id the alert does not carry) and 4
  # (Delete) ended, in smix's own words. A step that succeeded prints
  # only its start line; a skip or a failure prints `→ …` after it.
  line="$(printf '%s\n' "$OUT" | python3 -c '
import re, sys
out = {}
for l in sys.stdin:
    m = re.match(r"STEP ([34]): .*?(?: → (.*))?$", l.rstrip())
    if m:
        out[m.group(1)] = m.group(2) or out.get(m.group(1)) or "ok"
print(" | ".join("step %s: %s" % (k, out.get(k, "<not run>")) for k in ("3", "4")))
')"
  pressed=no
  [ "$after" = $((before + 1)) ] && pressed=yes
  [ "$pressed" = yes ] || missed=$((missed + 1))
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$i" "$before" "$after" "$pressed" "$RC" "$line" >>"$RAW"
  # An alert left up would sit over the next run's open-alert button.
  if [ "$pressed" = no ]; then
    printf 'appId: %s\n---\n- tapOn: { text: Cancel, optional: true }\n' "$APPID" >"$WORK/cancel.yaml"
    run_flow "$WORK/cancel.yaml"
  fi
done
log "T2 raw results ($RUNS runs):"
column -t -s "$(printf '\t')" "$RAW" >&2
[ "$missed" = 0 ] || fail "T2: $missed of $RUNS runs did not press Delete — reproduced; raw above"

log "C9B-IOS-E2E-PASS on $UDID (double tap and long press counted by the app; the consumer's confirm pressed $RUNS of $RUNS)"
