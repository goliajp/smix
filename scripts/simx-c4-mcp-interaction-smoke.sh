#!/usr/bin/env bash
# v0.6 C4 MCP interaction smoke gate
# - spawns `bun src/cli/index.ts mcp` as a real child process
# - writes JSON-RPC frames to stdin:
#     initialize / initialized notif / tools/list /
#     tools/call simulator_list /
#     tools/call key_press {udid, key:"return", returnSummary:false} /
#     tools/call tap {udid, selector:{text:"NonExistentMagicMarker_C4"}, returnSummary:false}
# - jq-verifies a 13-field gate.
# - key_press exercises the real driver path; tolerated if runner unreachable.
# - tap-nonexistent is the deterministic "must fail" assertion (the selector
#   resolves to zero elements so driver.tap throws → isError).
set -u
cd "$(dirname "$0")/.." || exit 99

RESPONSES_FILE="$(mktemp -t simx-c4-mcp.XXXXXX)"
STDERR_FILE="$(mktemp -t simx-c4-mcp-stderr.XXXXXX)"
trap 'rm -f "$RESPONSES_FILE" "$STDERR_FILE"' EXIT

# Lock to dev sim from .simx/dev-sim.txt — NEVER grab user's own sims.
# See simx-c2-mcp-lifecycle-smoke.sh for v0.6 audit context.
DEV_SIM_FILE="$(cd "$(dirname "$0")/.." && pwd)/.simx/dev-sim.txt"
TARGET_UDID=""
if [[ -f "${DEV_SIM_FILE}" ]]; then
  TARGET_UDID="$(tr -d '[:space:]' < "${DEV_SIM_FILE}" 2>/dev/null || true)"
fi
if [[ -z "${TARGET_UDID}" ]]; then
  TARGET_UDID="$(xcrun simctl list devices -j 2>/dev/null \
    | jq -r '
        [ .devices | to_entries[]
          | .value[]
          | select(.isAvailable == true)
        ]
        | (map(select(.state == "Booted"))[0] // .[0])
        | .udid // empty
      ' 2>/dev/null || true)"
fi

if [[ -z "${TARGET_UDID}" ]]; then
  TARGET_UDID="00000000-0000-0000-0000-000000000000"
fi

REQUESTS=$(cat <<EOF
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"simx-c4-smoke","version":"0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"simulator_list","arguments":{}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"key_press","arguments":{"udid":"${TARGET_UDID}","key":"return","returnSummary":false}}}
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"tap","arguments":{"udid":"${TARGET_UDID}","selector":{"text":"NonExistentMagicMarker_C4"},"returnSummary":false}}}
EOF
)

# Hold stdin open briefly after the last frame so async handlers can complete
# before EOF triggers shutdown. key_press / tap drive the runner-based path
# (XCUITest server); without a SimxRunner bound they fail with isError, which
# is OK for the gate (we have separate fields covering each outcome).
( printf '%s\n' "$REQUESTS"; sleep 5 ) \
  | bun src/cli/index.ts mcp \
    > "$RESPONSES_FILE" \
    2> "$STDERR_FILE"
EXIT_CODE=$?

INITIALIZE_OK="fail"
jq -e -s '[.[] | select(.id==1)][0].result.serverInfo.name == "simx"' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && INITIALIZE_OK="ok"

TOOLS_COUNT_RAW="$(jq -s '[.[] | select(.id==2)][0].result.tools | length' \
  < "$RESPONSES_FILE" 2>/dev/null || echo "-1")"
TOOLS_COUNT="${TOOLS_COUNT_RAW:--1}"

HAS_TAP="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("tap")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_TAP="ok"

HAS_DOUBLE_TAP="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("double_tap")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_DOUBLE_TAP="ok"

HAS_LONG_PRESS="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("long_press")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_LONG_PRESS="ok"

HAS_FILL="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("fill")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_FILL="ok"

HAS_SWIPE="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("swipe")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_SWIPE="ok"

HAS_SCROLL_TO="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("scroll_to")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_SCROLL_TO="ok"

HAS_KEY_PRESS="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("key_press")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_KEY_PRESS="ok"

# key_press: non-isError → "ok"; isError with runner-unreachable / not-supported text
# → "runner-unavailable-ok" (CI without a bound SimxRunner is tolerated, and
# pressKey may also be flagged "not supported by SimctlDriver" on platforms
# where the driver routes solely through the runner). Anything else → "fail".
KEY_PRESS_OK="fail"
KEY_PRESS_IS_ERR="$(jq -r -s '[.[] | select(.id==4)][0].result.isError // false' \
  < "$RESPONSES_FILE" 2>/dev/null || echo "true")"
if [[ "$KEY_PRESS_IS_ERR" == "false" ]]; then
  KEY_PRESS_OK="ok"
else
  KEY_PRESS_TEXT="$(jq -r -s '[.[] | select(.id==4)][0].result.content[0].text // ""' \
    < "$RESPONSES_FILE" 2>/dev/null || echo "")"
  if echo "$KEY_PRESS_TEXT" | grep -qiE 'runner|connect|reachable|refused|ECONN|fetch|timeout|EOF|not supported|unsupported|unimplemented'; then
    KEY_PRESS_OK="runner-unavailable-ok"
  fi
fi

# tap-nonexistent: this MUST surface as isError (selector resolves to 0 elements
# → driver.tap throws). The smoke fails if it ever succeeds silently.
TAP_NONEXISTENT_IS_ERROR="fail"
TAP_IS_ERR="$(jq -r -s '[.[] | select(.id==5)][0].result.isError // false' \
  < "$RESPONSES_FILE" 2>/dev/null || echo "false")"
if [[ "$TAP_IS_ERR" == "true" ]]; then
  TAP_NONEXISTENT_IS_ERROR="ok"
fi

# stdout must be pure line-delimited JSON-RPC frames.
STDOUT_PURE_JSONRPC="fail"
if jq -e -s 'all(.jsonrpc == "2.0")' < "$RESPONSES_FILE" > /dev/null 2>&1; then
  STDOUT_PURE_JSONRPC="ok"
fi

ALL_OK="fail"
if [[ "$EXIT_CODE" == "0" \
   && "$INITIALIZE_OK" == "ok" \
   && "$TOOLS_COUNT" == "27" \
   && "$HAS_TAP" == "ok" \
   && "$HAS_DOUBLE_TAP" == "ok" \
   && "$HAS_LONG_PRESS" == "ok" \
   && "$HAS_FILL" == "ok" \
   && "$HAS_SWIPE" == "ok" \
   && "$HAS_SCROLL_TO" == "ok" \
   && "$HAS_KEY_PRESS" == "ok" \
   && ( "$KEY_PRESS_OK" == "ok" || "$KEY_PRESS_OK" == "runner-unavailable-ok" ) \
   && "$TAP_NONEXISTENT_IS_ERROR" == "ok" \
   && "$STDOUT_PURE_JSONRPC" == "ok" ]]; then
  ALL_OK="ok"
fi

printf '{"exit_code":%s,"initialize_ok":"%s","tools_count":%s,"has_tap":"%s","has_double_tap":"%s","has_long_press":"%s","has_fill":"%s","has_swipe":"%s","has_scroll_to":"%s","has_key_press":"%s","key_press_ok":"%s","tap_nonexistent_isError":"%s","stdout_pure_jsonrpc":"%s","all_ok":"%s"}\n' \
  "$EXIT_CODE" \
  "$INITIALIZE_OK" \
  "$TOOLS_COUNT" \
  "$HAS_TAP" \
  "$HAS_DOUBLE_TAP" \
  "$HAS_LONG_PRESS" \
  "$HAS_FILL" \
  "$HAS_SWIPE" \
  "$HAS_SCROLL_TO" \
  "$HAS_KEY_PRESS" \
  "$KEY_PRESS_OK" \
  "$TAP_NONEXISTENT_IS_ERROR" \
  "$STDOUT_PURE_JSONRPC" \
  "$ALL_OK"

if [[ "$ALL_OK" == "ok" ]]; then exit 0; else exit 1; fi
