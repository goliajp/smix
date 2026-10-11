#!/usr/bin/env bash
# v14.2-C2: a control in a list row below the screen is off screen.
#
# The defect this is the instrument for, from a consumer's iOS 27 round
# (2026-10-12): UIKit lists every row of a table to accessibility, and
# iOS 27 gives the controls inside an off-screen row the table's origin
# for a frame while the row's cell keeps its own. smix took the frame:
# row 20's button was "visible" over row 0, `scrollUntilVisible` did not
# scroll, a swipe on it opened row 0's actions, and a tap at it opened
# row 0.
#
# The subject is the fixture's `-uikit-rows` screen: 30 UIKit rows of
# buttons with swipe actions, and a label (`uk_result`) that is the
# app's own reading of which row a touch reached. Every check reads that
# label or the tree, never smix's answer alone:
#   - every row's button lies inside its own cell, and some are off
#     screen (otherwise the screen is not the one this was written for)
#   - the runner's live find calls an off-screen row's button off screen
#     and an on-screen one on screen
#   - a tap at row 20 without scrolling fails, names the row as off
#     screen, and the app received nothing
#   - scrolling to row 20 and tapping it opens row 20
#   - scrolling to row 20, swiping it and pressing Edit edits row 20
#
# Against the 14.1.1 runner the first check fails (row 12 on is placed
# at the table's origin) and so does the tap: the app says "opened 0".
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

log()  { printf '[c2-rows] %s\n' "$*" >&2; }
fail() { printf '[c2-rows] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c2-rows] SKIP: %s\n' "$*" >&2; exit 2; }

UDID="" WE_BOOTED=0 WE_UPPED=0
cleanup() {
  if [ "$WE_UPPED" = 1 ]; then
    if ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" 2>&1)"; then
      printf '[c2-rows] warning: the iOS runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
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

open_rows() {
  cat >"$WORK/open.yaml" <<FLOW
appId: $APPID
---
- launchApp:
    arguments: ["-uikit-rows"]
- assertVisible: "nothing yet"
FLOW
  SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX_RUN" --device "$UDID" "$WORK/open.yaml" >"$WORK/open.log" 2>&1 \
    || { tail -10 "$WORK/open.log" >&2; fail "could not open the UIKit rows screen"; }
}

tree() { # $1 out.json
  "$SMIX" tree --json --port "$IOS_PORT" --device "$UDID" --platform ios 2>/dev/null >"$1" || true
  head -c 1 "$1" | grep -q '{' || fail "no tree"
}

result() { # what the app says it received
  tree "$WORK/result.json"
  python3 - "$WORK/result.json" <<'PY'
import json, sys
def walk(n):
    if n.get("identifier") == "uk_result":
        print(n.get("label", "")); raise SystemExit
    for c in n.get("children") or []: walk(c)
walk(json.load(open(sys.argv[1]))["root"])
print("<no uk_result>")
PY
}

run_flow() { # $1 name; flow text on stdin. Prints the exit code.
  cat >"$WORK/$1.yaml"
  local rc=0
  SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX_RUN" --device "$UDID" "$WORK/$1.yaml" >"$WORK/$1.log" 2>&1 || rc=$?
  echo "$rc"
}

# ---- 1. every button inside its own cell, some of them off screen ------
open_rows
tree "$WORK/rows.json"
placement="$(python3 - "$WORK/rows.json" <<'PY'
import json, sys
root = json.load(open(sys.argv[1]))["root"]
sh = root["bounds"]["h"]
rows, outside, off = 0, [], 0
def walk(n, cell):
    global rows, off
    if n.get("rawType") == "cell":
        cell = n["bounds"]
        if cell["y"] >= sh:
            off += 1
    ident = n.get("identifier") or ""
    if ident.startswith("uk_row_") and cell is not None:
        rows += 1
        b = n["bounds"]
        if not (b["y"] < cell["y"] + cell["h"] and cell["y"] < b["y"] + b["h"]):
            outside.append(f'{ident} y={b["y"]:.0f} cell y={cell["y"]:.0f}')
    for c in n.get("children") or []:
        walk(c, cell)
walk(root, None)
print(rows, off, "; ".join(outside[:4]) or "-")
PY
)"
read -r rows off outside <<<"$placement"
[ "$rows" = 30 ] || fail "found $rows of the 30 rows' buttons — the screen is not the one this was written for"
[ "$off" -ge 1 ] || fail "no row is below the screen, so nothing off screen was measured"
[ "$outside" = "-" ] || fail "buttons placed outside their own cells: $outside"
log "placement: 30 buttons each inside its own cell, $off rows below the screen"

# ---- 2. the live find --------------------------------------------------
find_on_screen() {
  curl -s -X POST "http://127.0.0.1:$IOS_PORT/find" -H 'content-type: application/json' \
    -H "App-Bundle-Id: $APPID" -d "{\"selector\":{\"text\":\"$1\"},\"requireOnScreen\":true}" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin).get("found"))'
}
[ "$(find_on_screen "Rule 3")" = True ] || fail "live find: row 3 is on screen and was not found there"
[ "$(find_on_screen "Rule 20")" = False ] || fail "live find: row 20 is below the screen and was called on screen"
log "live find: row 3 on screen, row 20 off"

# ---- 3. a tap at row 20 without scrolling ------------------------------
rc="$(run_flow tap-unscrolled <<FLOW
appId: $APPID
---
- tapOn: { id: uk_row_20 }
FLOW
)"
[ "$rc" != 0 ] || fail "a tap at row 20, below the screen, passed (the app says: $(result))"
grep -q "ELEMENT_NOT_FOUND" "$WORK/tap-unscrolled.log" || { tail -15 "$WORK/tap-unscrolled.log" >&2; fail "the unscrolled tap failed with something other than not found"; }
grep -q "off screen" "$WORK/tap-unscrolled.log" || { tail -15 "$WORK/tap-unscrolled.log" >&2; fail "the miss does not say row 20 is off screen"; }
said="$(result)"
[ "$said" = "nothing yet" ] || fail "the unscrolled tap reached the app: $said"
log "unscrolled tap: refused as off screen, the app received nothing"

# ---- 4. scroll, then tap -----------------------------------------------
open_rows
rc="$(run_flow scroll-tap <<FLOW
appId: $APPID
---
- scrollUntilVisible:
    element: { id: uk_row_20 }
    direction: DOWN
- tapOn: { id: uk_row_20 }
FLOW
)"
said="$(result)"
[ "$rc" = 0 ] || { tail -15 "$WORK/scroll-tap.log" >&2; fail "scroll then tap did not pass (the app says: $said)"; }
[ "$said" = "opened 20" ] || fail "scroll then tap reached the wrong row: $said"
log "scroll + tap: opened 20"

# ---- 5. scroll, swipe, Edit --------------------------------------------
open_rows
rc="$(run_flow swipe-edit <<FLOW
appId: $APPID
---
- scrollUntilVisible:
    element: { id: uk_row_20 }
    direction: DOWN
- swipe:
    over: { id: uk_row_20 }
    from: { x: 0.9, y: 0.5 }
    to: { x: 0.1, y: 0.5 }
- tapOn: "Edit"
FLOW
)"
said="$(result)"
[ "$rc" = 0 ] || { tail -15 "$WORK/swipe-edit.log" >&2; fail "scroll, swipe, Edit did not pass (the app says: $said)"; }
[ "$said" = "edited 20" ] || fail "the swipe opened the wrong row's actions: $said"
log "scroll + swipe + Edit: edited 20"

log "C2-ROWS-E2E-PASS"
