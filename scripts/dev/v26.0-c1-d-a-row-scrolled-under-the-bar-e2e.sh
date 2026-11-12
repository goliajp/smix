#!/usr/bin/env bash
# v26.0-C1: a row scrolled out of its list is not tapped through what is over it.
#
# A consumer's iOS list scrolled a row up past the list's top edge, under a
# translucent navigation bar. XCUITest gave the row its whole frame, smix
# called it fully visible, and the tap at its centre went to the bar —
# reported as tapped. On the fixture's scroll screen (a list below a title
# and a label), with the list swiped up so its first rows are past its top:
#   1. the tree says such a row is not visible
#   2. `tapOn` that row fails, saying it is scrolled out of its list and to
#      scroll to it, and the app received no tap
#   3. `scrollUntilVisible` up to it, then `tapOn`, presses it
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
BUNDLE="jp.golia.smix.fixture"
APP="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
WORK="$(mktemp -d)"

log()  { printf '[v26.0-c1-d] %s\n' "$*" >&2; }
fail() { printf '[v26.0-c1-d] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v26.0-c1-d] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" WE_BOOTED=0 UP=0
cleanup() {
  local said
  if [ "$UP" = 1 ] && ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$PORT" 2>&1)"; then
    log "warning: the runner on $PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

UDID="$("$SMIX" sim resolve "$E2E_IOS" 2>/dev/null | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator registered as '$E2E_IOS'"
e2e_yield_if_held "$UDID"
bash "$ROOT/scripts/dev/build-fixture-app.sh" >"$WORK/fixture.log" 2>&1 \
  || fail "fixture build: $(tail -5 "$WORK/fixture.log")"
if [ "$(simulator_state "$UDID")" != Booted ]; then
  "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "sim boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi
xcrun simctl install "$UDID" "$APP" >"$WORK/install.log" 2>&1 || fail "install: $(cat "$WORK/install.log")"
"$SMIX" runner up "$UDID" --runner-port "$PORT" --bundle "$BUNDLE" >"$WORK/up.log" 2>&1 \
  || fail "runner up: $(tail -5 "$WORK/up.log")"
UP=1

flow() { # <name> <steps> — exit status of the run, output in $WORK/<name>.log
  printf 'appId: %s\n---\n%s\n' "$BUNDLE" "$2" >"$WORK/$1.yaml"
  # raw run: judged by its exit together with what the app shows
  SMIX_RUNNER_PORT="$PORT" "$SMIX" run --device "$UDID" "$WORK/$1.yaml" >"$WORK/$1.log" 2>&1
}
result() {
  "$SMIX" tree --device "$UDID" --port "$PORT" --json 2>/dev/null | python3 -c 'import json,sys
def w(n):
    if n.get("identifier") == "scroll_result": return n.get("label") or ""
    for c in n.get("children", []):
        r = w(c)
        if r is not None: return r
print(w(json.load(sys.stdin)["root"]))'
}

flow arrange '- launchApp:
    stopApp: true
- scrollUntilVisible:
    element: { id: "fixture-open-scroll" }
- tapOn: { id: "fixture-open-scroll" }
- assertVisible: { id: "scroll_row_0" }
- swipe: { start: "50%, 70%", end: "50%, 45%" }
- waitForAnimationToEnd' || fail "arranging the screen: $(tail -3 "$WORK/arrange.log")"

# The row to judge: the last one whose frame ends past the list's top but
# still on the screen — where a swipe stops varies with its momentum.
read -r top row row_end row_visible <<ROW
$("$SMIX" tree --device "$UDID" --port "$PORT" --json 2>/dev/null | python3 -c 'import json,sys
top, rows = None, {}
def w(n):
    global top
    if n.get("rawType") == "scrollView": top = n["bounds"]["y"]
    i = n.get("identifier") or ""
    if i.startswith("scroll_row_"): rows[int(i.rsplit("_", 1)[1])] = n
    for c in n.get("children", []): w(c)
w(json.load(sys.stdin)["root"])
end = lambda n: n["bounds"]["y"] + n["bounds"]["h"]
past = [i for i, n in rows.items() if top is not None and 0 < end(n) <= top]
if not past: print(round(top or -1), -1, -1, "-")
else:
    k = max(past); print(round(top), k, round(end(rows[k])), rows[k].get("visible"))')
ROW
log "list top y=$top; row $row ends at y=$row_end, visible=$row_visible"
[ "$row" != -1 ] || cannot_judge "no row ends between the screen top and the list top, so the screen was not arranged"
[ "$row_visible" = False ] || fail "row $row is past its list's top and the tree calls it visible"
log "ok    row $row, past its list's top, is not visible"

rc=0
flow tap "- tapOn: { id: \"scroll_row_$row\" }" || rc=$?
[ "$rc" != 0 ] || fail "tapOn row $row passed with the row past its list's top"
grep -q "scrolled out of their list" "$WORK/tap.log" \
  || fail "the miss does not say the row is scrolled out of its list: $(grep -m1 hint "$WORK/tap.log")"
[ "$(result)" = "nothing tapped" ] || fail "the app received a tap: $(result)"
log "ok    tapOn row $row is refused as scrolled out of its list, and the app got nothing"

flow back "- scrollUntilVisible:
    element: { id: \"scroll_row_$row\" }
    direction: UP
- tapOn: { id: \"scroll_row_$row\" }" || fail "scrolling back to row $row and tapping it: $(tail -3 "$WORK/back.log")"
[ "$(result)" = "tapped $row" ] || fail "after scrolling back the app shows '$(result)', not 'tapped $row'"
log "ok    scrolled back, row $row is pressed"
log "PASS"
