#!/usr/bin/env bash
# simx-c6-resolver-tap-smoke.sh — end-to-end smoke for v0.3 C6:
# SimctlDriver.tap with 3 selector forms × resolveSelector → host-hid coord
# tap → real Settings root UI navigation.
#
#   1. Boot C2 runner (SimxRunnerUITests/test_runForever) → auto-launches
#      Settings. Wait for /health = 200.
#   2. Discover the "General" cell's identifier (if any) via GET /tree.
#   3. Phase B (id / label selector path B):
#        driver.tap({id:<discovered>}) (fallback {label:'General'})
#        → host-hid digitizer tap → General sub-page push.
#      Double-sided probe:
#        POST /tap "Display & Brightness" → 404 (gone from root)
#        POST /tap "About"                → 200 (on General sub-page)
#   4. Reset to root via simctl terminate Settings + sleep (xcuitest reattaches).
#   5. Phase C (role+name selector path B):
#        driver.tap({role:'cell', name:'General'}) → probes.
#   6. Reset to root.
#   7. Phase D (text+modifier selector path B):
#        driver.tap({text:'General', below:{text:'Apple Account'}}) → probes.
#   8. Emit single-line summary JSON, exit 0 iff all 3 selector phases
#      reach the General sub-page.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="$ROOT/swift-bridge/SimxRunner.xcodeproj"
SCHEME="SimxRunner"
PORT="${SIMX_RUNNER_PORT:-22087}"

MISS_TEXT="${SIMX_PROBE_MISS_TEXT:-Display & Brightness}"
HIT_TEXT="${SIMX_PROBE_HIT_TEXT:-About}"

# Phase 0: dev sim UDID + state check.
if [[ -z "${SIMX_UDID:-}" ]]; then
  if [[ -f "$ROOT/.simx/dev-sim.txt" ]]; then
    SIMX_UDID="$(tr -d '[:space:]' < "$ROOT/.simx/dev-sim.txt")"
  else
    SIMX_UDID="$(xcrun simctl list devices -j \
      | jq -r '[.devices | to_entries[] | .value[] | select(.isAvailable==true and .state=="Booted")][0].udid // empty')"
  fi
fi
if [[ -z "${SIMX_UDID:-}" ]]; then
  echo "no booted iOS simulator (set SIMX_UDID or boot a sim)" >&2
  exit 2
fi

DEV_STATE="$(xcrun simctl list devices -j \
  | jq -r --arg u "$SIMX_UDID" \
        '.devices | to_entries[] | .value[] | select(.udid==$u) | .state // empty')"
if [[ "$DEV_STATE" != "Booted" ]]; then
  echo "dev sim $SIMX_UDID is not Booted (state=$DEV_STATE)" >&2
  exit 2
fi

HOST_HID_BIN="$ROOT/swift-bridge/.build/debug/simx-host-hid"
if [[ ! -x "$HOST_HID_BIN" ]]; then
  echo "host-hid binary missing at $HOST_HID_BIN; run: swift build -c debug --product simx-host-hid --package-path swift-bridge" >&2
  exit 2
fi

if lsof -iTCP:"$PORT" -sTCP:LISTEN > /dev/null 2>&1; then
  echo "port $PORT already in use; clean it before retry" >&2
  exit 2
fi

xcrun simctl terminate "$SIMX_UDID" com.apple.Preferences > /dev/null 2>&1 || true

LOGDIR="$ROOT/.simx/runner"
mkdir -p "$LOGDIR"
LOGFILE="$LOGDIR/xcodebuild-c6-resolver-$$.log"
PIDFILE="$LOGDIR/runner-c6-resolver-$$.pid"
TMPDIR_C6="$(mktemp -d /tmp/simx-c6-resolver-$$-XXXXXX)"

( xcodebuild -project "$PROJECT" \
             -scheme "$SCHEME" \
             -destination "platform=iOS Simulator,id=$SIMX_UDID" \
             -only-testing:SimxRunnerUITests/SimxRunnerUITests/test_runForever \
             test > "$LOGFILE" 2>&1 ) &
echo $! > "$PIDFILE"

cleanup() {
  local pid; pid="$(cat "$PIDFILE" 2>/dev/null || true)"
  if [[ -n "$pid" ]]; then
    pkill -P "$pid" 2>/dev/null || true
    kill -TERM "$pid" 2>/dev/null || true
  fi
  xcrun simctl terminate "$SIMX_UDID" com.apple.Preferences > /dev/null 2>&1 || true
  rm -rf "$TMPDIR_C6" 2>/dev/null || true
}
trap cleanup EXIT

# wait_health: poll /health up to N seconds.
wait_health() {
  local max="${1:-180}"
  local status=""
  for _ in $(seq 1 "$max"); do
    status="$(curl -sS -o /dev/null -w '%{http_code}' \
                   --max-time 2 "http://127.0.0.1:$PORT/health" 2>/dev/null || echo "")"
    if [[ "$status" = "200" ]]; then
      echo "$status"
      return 0
    fi
    sleep 1
  done
  echo "$status"
  return 1
}

HEALTH_STATUS="$(wait_health 180)" || {
  echo "health did not reach 200 within 180s" >&2
  tail -40 "$LOGFILE" >&2 || true
  exit 1
}

# --- helpers --------------------------------------------------------------

probe_count=0
mk_tmp() {
  probe_count=$((probe_count + 1))
  printf '%s/p%03d-%s' "$TMPDIR_C6" "$probe_count" "$1"
}

# probe_miss <text>: 0 if /tap returns 404 + {ok:false,error:not_found}.
probe_miss() {
  local text="$1"
  local body status
  body="$(mk_tmp miss.json)"
  status="$(curl -sS -o "$body" -w '%{http_code}' \
                 --max-time 15 \
                 -X POST -H 'Content-Type: application/json' \
                 -d "$(jq -Rn --arg t "$text" '{selector:{text:$t}}')" \
                 "http://127.0.0.1:$PORT/tap" 2>/dev/null || echo "000")"
  if [[ "$status" != "404" ]]; then
    echo "probe_miss($text) expected 404 got $status; body=$(cat "$body" 2>/dev/null || echo none)" >&2
    return 1
  fi
  if ! jq -e '.ok == false and .error == "not_found"' "$body" > /dev/null 2>&1; then
    echo "probe_miss($text) bad body schema: $(cat "$body")" >&2
    return 1
  fi
  return 0
}

# probe_hit <text>: 0 if /tree contains a Cell-shaped element with the
# label. Unlike POST /tap, this doesn't navigate INTO the matched element,
# so we don't push Settings deeper (which complicates return_to_root).
probe_hit() {
  local text="$1"
  local body
  body="$(mk_tmp hit.json)"
  if ! curl -sS -o "$body" --max-time 15 "http://127.0.0.1:$PORT/tree" > /dev/null 2>&1; then
    echo "probe_hit($text) /tree fetch failed" >&2
    return 1
  fi
  if ! jq -e --arg t "$text" '
        [.. | objects | select(.label? == $t)] | length > 0
      ' "$body" > /dev/null 2>&1; then
    echo "probe_hit($text): label not in tree" >&2
    return 1
  fi
  return 0
}

# return_to_root: tap the iOS back-button area via host-hid at fixed coord
# (top-left, ~0.05x 0.06y). Loops until /tree shows "Display & Brightness"
# (root readiness signal) or 15s elapsed. Resilient to iOS version differences
# in back-button label text (XCUITest /tap-by-text can mis-target).
return_to_root() {
  for _ in $(seq 1 10); do
    local body
    body="$(mk_tmp rtr-tree.json)"
    if curl -sS -o "$body" --max-time 5 "http://127.0.0.1:$PORT/tree" > /dev/null 2>&1; then
      if jq -e '[.. | objects | select(.label? == "Display & Brightness")] | length > 0' "$body" > /dev/null 2>&1; then
        # Wait one more cycle so any UIKit transition completes before the
        # next tap_via_driver pulls a tree.
        sleep 2
        return 0
      fi
    fi
    # Not on root — tap the back-button area at top-left of nav bar.
    # iPhone 16 Pro iOS 26.4 nav-bar back-button @ ~(30px, 110px) on 402x874 screen ≈ (0.075, 0.126).
    "$HOST_HID_BIN" tap --udid "$SIMX_UDID" --x 0.08 --y 0.13 > /dev/null 2>&1 || true
    sleep 1
  done
  echo "return_to_root: Display & Brightness never reappeared on root after 8 back-taps" >&2
  return 1
}

# fetch_tree: GET /tree → file. echo file path.
fetch_tree() {
  local body
  body="$(mk_tmp tree.json)"
  if ! curl -sS -o "$body" --max-time 15 "http://127.0.0.1:$PORT/tree" > /dev/null 2>&1; then
    echo "fetch_tree: curl failed" >&2
    return 1
  fi
  echo "$body"
}

# warmup_tree: GET /tree repeatedly until a tree with >=10 labeled nodes
# appears, or 8s elapsed. XCUIApplication.snapshot() after a back-button
# transition sometimes returns a structural skeleton (no labels) for the
# first 1-2 calls — burn through that before letting tap_via_driver pull a
# tree it'll feed into resolveSelector.
warmup_tree() {
  local i=0
  for _ in $(seq 1 25); do
    i=$((i + 1))
    local body
    body="$(mk_tmp warmup-tree.json)"
    if curl -sS -o "$body" --max-time 5 "http://127.0.0.1:$PORT/tree" > /dev/null 2>&1; then
      # Require both "General" and "Display & Brightness" labels to confirm
      # we're on Settings root with a fully-populated snapshot (not a
      # structural skeleton).
      if jq -e '
            [.. | objects | select(.label? == "General")] | length > 0
          and
            [.. | objects | select(.label? == "Display & Brightness")] | length > 0
          ' "$body" > /dev/null 2>&1; then
        return 0
      fi
    fi
    # Every 5 attempts, send a tiny no-op tap on the status bar area to nudge
    # Settings into rebuilding its snapshot. Status bar (y < 0.04) absorbs the
    # touch without navigating.
    if (( i % 5 == 0 )); then
      "$HOST_HID_BIN" tap --udid "$SIMX_UDID" --x 0.5 --y 0.02 > /dev/null 2>&1 || true
    fi
    sleep 1
  done
  return 1
}

# tap_via_driver: run bun -e, construct SimctlDriver(cell) and invoke
# driver.tap(eval(sel_json)). sel_json must be a JS expression evaluated
# as a Selector literal.
tap_via_driver() {
  local sel_expr="$1"
  local label="$2"
  local out
  out="$(mk_tmp ${label}.out)"
  # Persistent stderr log for tap_via_driver debugging.
  local persisterr="$ROOT/.simx/runner/c6-resolver-tap-${label}-$$.err"
  if ! (cd "$ROOT" && \
        SIMX_UDID="$SIMX_UDID" SIMX_PORT="$PORT" \
        SIMX_HOSTHID="$HOST_HID_BIN" SIMX_TRACE="$ROOT/.simx/trace" \
        SIMX_SEL_JSON="$sel_expr" \
        bun -e "
Promise.all([
  import('./src/sim/simctl.ts'),
  import('./src/sim/session.ts'),
  import('./src/driver/simctl-driver.ts'),
]).then(async ([sm, ss, sd]) => {
  const udid = process.env.SIMX_UDID
  const port = Number(process.env.SIMX_PORT)
  const client = new sm.SimctlClient()
  const devices = await client.listDevices()
  const device = devices.find(d => d.udid === udid)
  if (!device) { console.error('device not found:' + udid); process.exit(2) }
  const session = new ss.SimSession(client, device)
  const cell = {
    id: 'cell-0001',
    udid,
    runnerPort: port,
    traceDir: process.env.SIMX_TRACE,
    session,
  }
  const driver = new sd.SimctlDriver(cell, { hostHidBin: process.env.SIMX_HOSTHID })
  const sel = (0, eval)('(' + process.env.SIMX_SEL_JSON + ')')
  try {
    await driver.tap(sel)
    console.log('ok')
  } catch (e) {
    console.error('tap failed:', (e && e.message) || e)
    if (e && e.selector) console.error('selector:', JSON.stringify(e.selector))
    if (e && e.visibleElements) console.error('visibleElements:', JSON.stringify(e.visibleElements).slice(0,500))
    process.exit(1)
  }
}).catch((e) => { console.error('bootstrap failed:', String(e)); process.exit(2) })
") > "$out" 2> "$persisterr"; then
    echo "tap_via_driver[$label] failed (stderr saved to $persisterr):" >&2
    echo "--- stdout ---" >&2; cat "$out" >&2 || true
    echo "--- stderr ---" >&2; cat "$persisterr" >&2 || true
    echo "--- xcodebuild tail ---" >&2; tail -40 "$LOGFILE" >&2 || true
    return 1
  fi
  return 0
}

# --- Diagnostic: dump tree + node count -----------------------------------
TREE_FILE="$(fetch_tree)"
NODE_COUNT="$(jq '[.. | objects | select(.rawType?)] | length' "$TREE_FILE" 2>/dev/null || echo 0)"

# Find the General cell's identifier and rawType (any depth).
GENERAL_ID="$(jq -r '
  [.. | objects | select(.rawType? and (.label? == "General" or .label? == "General "))][0]
  | (.identifier // "") ' "$TREE_FILE" 2>/dev/null || echo "")"
GENERAL_ID="$(echo "$GENERAL_ID" | tr -d '\n')"

GENERAL_RAWTYPE="$(jq -r '
  [.. | objects | select(.rawType? and (.label? == "General" or .label? == "General "))][0]
  | (.rawType // "") ' "$TREE_FILE" 2>/dev/null || echo "")"
GENERAL_RAWTYPE="$(echo "$GENERAL_RAWTYPE" | tr -d '\n')"

# --- Phase B: id (or label-fallback) selector ----------------------------
ID_TAP_RESULT="fail"
ID_SEL_USED=""
if [[ -n "$GENERAL_ID" ]] && tap_via_driver "{id:'$GENERAL_ID'}" "phaseB-id"; then
  ID_TAP_RESULT="ok"
  ID_SEL_USED="id"
elif tap_via_driver "{label:'General'}" "phaseB-label"; then
  ID_TAP_RESULT="ok-via-label-fallback"
  ID_SEL_USED="label"
fi
if [[ "$ID_TAP_RESULT" = "fail" ]]; then
  echo "id_selector_tap failed (id='$GENERAL_ID' and label fallback both)" >&2
  tail -40 "$LOGFILE" >&2 || true
  exit 1
fi

sleep 2

ID_PROBE_MISS="$(probe_miss "$MISS_TEXT" && echo 404 || echo fail)"
ID_PROBE_HIT="$(probe_hit "$HIT_TEXT" && echo ok || echo fail)"

return_to_root || true
warmup_tree || true

# --- Phase C: role+name selector ----------------------------------------
# Pick role based on the General cell's rawType. iOS 26 Settings cells are
# sometimes `cell` and sometimes other (button/staticText wrapper). Fall
# back to whatever the tree shows; we want a real role+name selector test.
ROLE_TAP_RESULT="fail"
ROLE_SEL_USED="cell"
if [[ -n "$GENERAL_RAWTYPE" && "$GENERAL_RAWTYPE" != "cell" ]]; then
  ROLE_SEL_USED="$GENERAL_RAWTYPE"
fi
if tap_via_driver "{role:'${ROLE_SEL_USED}', name:'General'}" "phaseC-role"; then
  ROLE_TAP_RESULT="ok"
fi
sleep 2

ROLE_PROBE_MISS="$(probe_miss "$MISS_TEXT" && echo 404 || echo fail)"
ROLE_PROBE_HIT="$(probe_hit "$HIT_TEXT" && echo ok || echo fail)"

return_to_root || true
warmup_tree || true
# Extra settling for Phase D — Settings sometimes returns a transient
# stripped tree right after a quick back-tap stack.
sleep 2

# --- Phase D: text + modifier selector ----------------------------------
TEXT_TAP_RESULT="fail"
if tap_via_driver "{text:'General', below:{text:'Settings'}}" "phaseD-text"; then
  TEXT_TAP_RESULT="ok"
elif sleep 3 && tap_via_driver "{text:'General', below:{text:'Settings'}}" "phaseD-text-retry"; then
  # One retry after a longer settle; some iOS 26.4 Settings tree transitions
  # need ~3-4s to return cell contents in /tree.
  TEXT_TAP_RESULT="ok-retry"
fi
sleep 2

TEXT_PROBE_MISS="$(probe_miss "$MISS_TEXT" && echo 404 || echo fail)"
TEXT_PROBE_HIT="$(probe_hit "$HIT_TEXT" && echo ok || echo fail)"

# --- Phase E: single-line JSON summary on stdout -------------------------
printf '{"health":%s,"node_count":%s,"general_identifier":"%s","general_rawtype":"%s","id_selector_used":"%s","id_selector_tap":"%s","id_selector_probe_miss":"%s","id_selector_probe_hit":"%s","role_selector_used":"%s","role_selector_tap":"%s","role_selector_probe_miss":"%s","role_selector_probe_hit":"%s","text_selector_tap":"%s","text_selector_probe_miss":"%s","text_selector_probe_hit":"%s"}\n' \
       "$HEALTH_STATUS" "$NODE_COUNT" "${GENERAL_ID:-}" "${GENERAL_RAWTYPE:-}" "$ID_SEL_USED" \
       "$ID_TAP_RESULT" "$ID_PROBE_MISS" "$ID_PROBE_HIT" \
       "$ROLE_SEL_USED" "$ROLE_TAP_RESULT" "$ROLE_PROBE_MISS" "$ROLE_PROBE_HIT" \
       "$TEXT_TAP_RESULT" "$TEXT_PROBE_MISS" "$TEXT_PROBE_HIT"

# Exit 0 only if every selector form successfully navigated to General sub-page.
# Gate: probe_miss returns 404 (root-only element D&B gone) AND probe_hit returns
# ok (General sub-page element "About" visible in /tree).
if [[ "$ID_PROBE_MISS" = "404" && "$ID_PROBE_HIT" = "ok" \
   && "$ROLE_PROBE_MISS" = "404" && "$ROLE_PROBE_HIT" = "ok" \
   && "$TEXT_PROBE_MISS" = "404" && "$TEXT_PROBE_HIT" = "ok" ]]; then
  exit 0
fi
exit 1
