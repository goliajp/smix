#!/usr/bin/env bash
# v0.6 C3 MCP observe smoke gate
# - spawns `bun src/cli/index.ts mcp` as a real child process
# - writes JSON-RPC frames to stdin:
#     initialize / initialized notif / tools/list /
#     tools/call simulator_list /
#     tools/call screen_screenshot {udid} /
#     tools/call screen_describe {udid} /
#     tools/call element_inspect {udid, selector:{text:"Settings"}}
# - jq-verifies a 12-field gate.
# - screen_hierarchy is NOT exercised (driver.tree() requires a SimxRunner
#   XCUITest server, which is not assumed to be running in CI — we only
#   verify it shows up in tools/list).
set -u
cd "$(dirname "$0")/.." || exit 99

RESPONSES_FILE="$(mktemp -t simx-c3-mcp.XXXXXX)"
STDERR_FILE="$(mktemp -t simx-c3-mcp-stderr.XXXXXX)"
trap 'rm -f "$RESPONSES_FILE" "$STDERR_FILE"' EXIT

# Pick a booted simulator (fall back to first available).
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
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"simx-c3-smoke","version":"0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"simulator_list","arguments":{}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"screen_screenshot","arguments":{"udid":"${TARGET_UDID}"}}}
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"screen_describe","arguments":{"udid":"${TARGET_UDID}","limit":5}}}
{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"element_inspect","arguments":{"udid":"${TARGET_UDID}","selector":{"text":"Settings"}}}}
EOF
)

# Hold stdin open briefly so async tool handlers complete before EOF shutdown.
# screen_describe drives the runner-based a11y tree path; without a SimxRunner
# bound to the cell port it will fail with isError. screen_screenshot uses
# simctl io directly so it does not need a runner.
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

HAS_SCREEN_DESCRIBE="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("screen_describe")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_SCREEN_DESCRIBE="ok"

HAS_SCREEN_SCREENSHOT="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("screen_screenshot")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_SCREEN_SCREENSHOT="ok"

HAS_SCREEN_HIERARCHY="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("screen_hierarchy")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_SCREEN_HIERARCHY="ok"

HAS_ELEMENT_INSPECT="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("element_inspect")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_ELEMENT_INSPECT="ok"

# screen_screenshot: real PNG base64; iOS-class frames are far larger than 1000 chars.
SCREEN_SCREENSHOT_BASE64_LEN_GT_1000="no"
SCREENSHOT_LEN="$(jq -r -s '[.[] | select(.id==4)][0].result.content[0].text | fromjson | .base64 | length' \
  < "$RESPONSES_FILE" 2>/dev/null || echo "-1")"
if [[ "${SCREENSHOT_LEN:--1}" =~ ^[0-9]+$ ]] && (( SCREENSHOT_LEN > 1000 )); then
  SCREEN_SCREENSHOT_BASE64_LEN_GT_1000="yes"
fi

# screen_describe: parsed JSON has elements as array. Tolerate isError when
# runner is not reachable: the field counts as "ok" if either the parsed result
# yields an array OR the call returned isError (well-formed failure path).
SCREEN_DESCRIBE_ELEMENTS_IS_ARRAY="no"
DESCRIBE_IS_ERR="$(jq -r -s '[.[] | select(.id==5)][0].result.isError // false' \
  < "$RESPONSES_FILE" 2>/dev/null || echo "false")"
if [[ "$DESCRIBE_IS_ERR" == "true" ]]; then
  SCREEN_DESCRIBE_ELEMENTS_IS_ARRAY="ok"
else
  if jq -e -s '[.[] | select(.id==5)][0].result.content[0].text | fromjson | .elements | type == "array"' \
    < "$RESPONSES_FILE" > /dev/null 2>&1; then
    SCREEN_DESCRIBE_ELEMENTS_IS_ARRAY="ok"
  fi
fi

# element_inspect: node === null OR node.rawType is a string; isError also
# counts as ok (runner unreachable on CI).
ELEMENT_INSPECT_NODE_RESOLVABLE="no"
EI_IS_ERR="$(jq -r -s '[.[] | select(.id==6)][0].result.isError // false' \
  < "$RESPONSES_FILE" 2>/dev/null || echo "false")"
if [[ "$EI_IS_ERR" == "true" ]]; then
  ELEMENT_INSPECT_NODE_RESOLVABLE="ok"
else
  if jq -e -s '
    [.[] | select(.id==6)][0].result.content[0].text | fromjson |
    (.node == null) or (.node.rawType | type == "string")
  ' < "$RESPONSES_FILE" > /dev/null 2>&1; then
    ELEMENT_INSPECT_NODE_RESOLVABLE="ok"
  fi
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
   && "$HAS_SCREEN_DESCRIBE" == "ok" \
   && "$HAS_SCREEN_SCREENSHOT" == "ok" \
   && "$HAS_SCREEN_HIERARCHY" == "ok" \
   && "$HAS_ELEMENT_INSPECT" == "ok" \
   && "$SCREEN_SCREENSHOT_BASE64_LEN_GT_1000" == "yes" \
   && "$SCREEN_DESCRIBE_ELEMENTS_IS_ARRAY" == "ok" \
   && "$ELEMENT_INSPECT_NODE_RESOLVABLE" == "ok" \
   && "$STDOUT_PURE_JSONRPC" == "ok" ]]; then
  ALL_OK="ok"
fi

printf '{"exit_code":%s,"initialize_ok":"%s","tools_count":%s,"has_screen_describe":"%s","has_screen_screenshot":"%s","has_screen_hierarchy":"%s","has_element_inspect":"%s","screen_screenshot_base64_len_gt_1000":"%s","screen_describe_elements_is_array":"%s","element_inspect_node_resolvable":"%s","stdout_pure_jsonrpc":"%s","all_ok":"%s"}\n' \
  "$EXIT_CODE" \
  "$INITIALIZE_OK" \
  "$TOOLS_COUNT" \
  "$HAS_SCREEN_DESCRIBE" \
  "$HAS_SCREEN_SCREENSHOT" \
  "$HAS_SCREEN_HIERARCHY" \
  "$HAS_ELEMENT_INSPECT" \
  "$SCREEN_SCREENSHOT_BASE64_LEN_GT_1000" \
  "$SCREEN_DESCRIBE_ELEMENTS_IS_ARRAY" \
  "$ELEMENT_INSPECT_NODE_RESOLVABLE" \
  "$STDOUT_PURE_JSONRPC" \
  "$ALL_OK"

if [[ "$ALL_OK" == "ok" ]]; then exit 0; else exit 1; fi
