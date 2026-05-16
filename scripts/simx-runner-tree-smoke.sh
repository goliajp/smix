#!/usr/bin/env bash
# simx-runner-tree-smoke.sh — boot SimxRunner, hit GET /tree on Settings,
# verify the frontmost a11y tree comes back with the Settings "General" cell.
# WHY: v0.3 C1 verification harness; standalone (does NOT modify the v0.2
# acceptance script — v0.3 acceptance will integrate in v0.3 C7).
# Target app (com.apple.Preferences) is hard-coded inside SimxRunnerUITests.swift.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROJECT="$ROOT/swift-bridge/SimxRunner.xcodeproj"
SCHEME="SimxRunner"
PORT="${SIMX_RUNNER_PORT:-22087}"
EXPECTED_BUNDLE="${SIMX_TREE_BUNDLE_ID:-com.apple.Preferences}"
EXPECTED_CELL="${SIMX_TREE_EXPECT_CELL:-General}"

if [[ -z "${SIMX_UDID:-}" ]]; then
  if [[ -f "$ROOT/.simx/dev-sim.txt" ]]; then
    SIMX_UDID="$(cat "$ROOT/.simx/dev-sim.txt" | tr -d '[:space:]')"
  else
    SIMX_UDID="$(xcrun simctl list devices -j \
      | jq -r '[.devices | to_entries[] | .value[] | select(.isAvailable==true and .state=="Booted")][0].udid // empty')"
  fi
fi
if [[ -z "$SIMX_UDID" ]]; then
  echo "no booted iOS simulator (set SIMX_UDID or boot a sim)" >&2
  exit 2
fi

if lsof -iTCP:"$PORT" -sTCP:LISTEN > /dev/null 2>&1; then
  echo "port $PORT already in use; clean it before retry" >&2
  exit 2
fi

LOGDIR="$ROOT/.simx/runner"
mkdir -p "$LOGDIR"
LOGFILE="$LOGDIR/xcodebuild-tree-$$.log"
PIDFILE="$LOGDIR/runner-tree-$$.pid"

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
}
trap cleanup EXIT

# Phase A: poll /health up to 150s (cold build + Settings launch).
HEALTH_STATUS=""
for i in $(seq 1 150); do
  HEALTH_STATUS="$(curl -sS -o /dev/null -w '%{http_code}' \
                       --max-time 2 "http://127.0.0.1:$PORT/health" 2>/dev/null || echo "")"
  if [[ "$HEALTH_STATUS" = "200" ]]; then
    break
  fi
  sleep 1
done
if [[ "$HEALTH_STATUS" != "200" ]]; then
  echo "health did not reach 200 within 150s" >&2
  tail -40 "$LOGFILE" >&2 || true
  exit 1
fi

# Phase B: GET /tree — expect 200 + JSON body shaped like A11yNode.
TREE_BODY="/tmp/simx-tree-$$.json"
TREE_STATUS="$(curl -sS -o "$TREE_BODY" -w '%{http_code}' \
                    --max-time 20 \
                    "http://127.0.0.1:$PORT/tree")"
if [[ "$TREE_STATUS" != "200" ]]; then
  echo "tree expected 200, got $TREE_STATUS" >&2
  cat "$TREE_BODY" >&2 || true
  exit 1
fi

# Phase C: jq gates — fail fast on any unmet expectation.
APP_RAWTYPE="$(jq -r '.rawType // empty' "$TREE_BODY")"
if [[ "$APP_RAWTYPE" != "application" ]]; then
  echo "tree root rawType expected 'application', got '$APP_RAWTYPE'" >&2
  head -c 2000 "$TREE_BODY" >&2 || true
  exit 1
fi

APP_IDENTIFIER="$(jq -r '.identifier // empty' "$TREE_BODY")"
if [[ "$APP_IDENTIFIER" != "$EXPECTED_BUNDLE" ]]; then
  echo "tree root identifier expected '$EXPECTED_BUNDLE', got '$APP_IDENTIFIER'" >&2
  exit 1
fi

if ! jq -e '(.bounds.w > 0) and (.bounds.h > 0)' "$TREE_BODY" > /dev/null; then
  echo "tree root bounds non-positive" >&2
  jq '.bounds' "$TREE_BODY" >&2 || true
  exit 1
fi

# iOS 26.4 renders Settings list items as `cell` containing a `button` whose
# label is the item name (e.g. "General"). Older iOS surfaces the label
# directly on the cell. Accept either layout — the gate proves "a Settings
# list item named EXPECTED_CELL exists", which is the semantic intent.
GENERAL_COUNT="$(jq --arg lbl "$EXPECTED_CELL" \
                    '[.. | objects
                      | select(.rawType? == "cell")
                      | select([.. | objects | .label? // empty] | index($lbl))]
                     | length' \
                    "$TREE_BODY")"
if [[ "$GENERAL_COUNT" -lt 1 ]]; then
  echo "expected cell containing label '$EXPECTED_CELL' not found in tree" >&2
  head -c 4000 "$TREE_BODY" >&2 || true
  exit 1
fi

NODE_COUNT="$(jq '[.. | objects | select(.rawType? != null)] | length' "$TREE_BODY")"
if [[ "$NODE_COUNT" -lt 10 ]]; then
  echo "tree node_count $NODE_COUNT below threshold 10" >&2
  exit 1
fi

GENERAL_FOUND_JSON=$([[ "$GENERAL_COUNT" -ge 1 ]] && echo "true" || echo "false")

# Phase C.5: role-fill gate — Swift wire deliberately omits `role` (v0.3 C1
# decision); the TS-side RunnerClient.getTree() post-processor (v0.3 C3) is
# what writes `role` derived from `rawType`. So we run a 1-shot Bun script
# that imports RunnerClient and prints the parsed tree, then jq-gate the
# result for role_filled_count + role_filled_button_or_cell.
TS_TREE="$(cd "$ROOT" && bun -e "
  import('./src/sim/runner-client.ts').then(async (m) => {
    const c = new m.RunnerClient({ port: ${PORT} })
    const t = await c.getTree()
    console.log(JSON.stringify(t))
  }).catch((e) => { console.error(String(e)); process.exit(1) })
" 2>/dev/null || echo '{}')"

ROLE_FILLED_COUNT="$(echo "$TS_TREE" \
    | jq '[.. | objects | select(.role? != null)] | length' 2>/dev/null || echo 0)"
ROLE_BC_FOUND="$(echo "$TS_TREE" \
    | jq -r 'first(.. | objects | select(.role? == "button" or .role? == "cell")) | if . != null then "true" else "false" end' 2>/dev/null || echo "false")"

if [[ "${ROLE_FILLED_COUNT:-0}" -lt 1 ]]; then
  echo "expected role_filled_count >= 1, got '$ROLE_FILLED_COUNT'" >&2
  echo "TS_TREE preview:" >&2
  echo "$TS_TREE" | head -c 1000 >&2
  exit 1
fi
if [[ "$ROLE_BC_FOUND" != "true" ]]; then
  echo "expected role==button or cell descendant, none found" >&2
  exit 1
fi

# Phase D: single-line JSON summary on stdout.
printf '{"health":%s,"tree_status":%s,"app_rawType":"%s","app_identifier":"%s","general_cell_found":%s,"node_count":%s,"role_filled_count":%s,"role_filled_button_or_cell":%s}\n' \
       "$HEALTH_STATUS" "$TREE_STATUS" "$APP_RAWTYPE" "$APP_IDENTIFIER" "$GENERAL_FOUND_JSON" "$NODE_COUNT" "$ROLE_FILLED_COUNT" "$ROLE_BC_FOUND"

rm -f "$TREE_BODY"
exit 0
