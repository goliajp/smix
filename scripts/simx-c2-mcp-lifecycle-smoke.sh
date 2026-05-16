#!/usr/bin/env bash
# v0.6 C2 MCP lifecycle smoke gate
# - spawns `bun src/cli/index.ts mcp` as a real child process
# - writes 5 JSON-RPC frames to stdin:
#     initialize / initialized notif / tools/list /
#     tools/call simulator_list /
#     tools/call app_launch (Preferences on the first booted sim) /
#     tools/call app_terminate
# - reads stdout responses (line-delimited JSON-RPC), jq-verifies 10-field gate
set -u
cd "$(dirname "$0")/.." || exit 99

RESPONSES_FILE="$(mktemp -t simx-c2-mcp.XXXXXX)"
STDERR_FILE="$(mktemp -t simx-c2-mcp-stderr.XXXXXX)"
trap 'rm -f "$RESPONSES_FILE" "$STDERR_FILE"' EXIT

# Pick a booted simulator to drive app_launch / app_terminate.
# If none is booted, fall back to the first available device — then
# app_launch will return isError, which surfaces in the schema_ok gate
# rather than silently passing.
# Lock to dev sim from .simx/dev-sim.txt — NEVER grab user's own sims.
# v0.6 audit (2026-05-16): earlier auto-discovery via simctl list picked
# the first booted device which could be the user's iPhone 17 Pro, not the
# locked simx-dev. Always prefer the dev-sim.txt UDID; only fall back to
# auto-discovery if the lock file is missing (e.g. fresh clone).
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
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"simx-c2-smoke","version":"0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"simulator_list","arguments":{}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"app_launch","arguments":{"udid":"${TARGET_UDID}","bundleId":"com.apple.Preferences"}}}
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"app_terminate","arguments":{"udid":"${TARGET_UDID}","bundleId":"com.apple.Preferences"}}}
EOF
)

# Hold stdin open briefly after the last frame so the SDK's stdio reader can
# dispatch the async tools/call handlers before EOF triggers shutdown.
# Without this, the server closes mid-flight and id 3/4/5 responses never
# reach stdout. 5s is comfortably above the worst-case xcrun roundtrip.
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

SIMULATOR_LIST_DEVICES_GE1="no"
DEVICES_COUNT="$(jq -s '[.[] | select(.id==3)][0].result.content[0].text | fromjson | .devices | length' \
  < "$RESPONSES_FILE" 2>/dev/null || echo "-1")"
if [[ "${DEVICES_COUNT:--1}" =~ ^[0-9]+$ ]] && (( DEVICES_COUNT >= 1 )); then
  SIMULATOR_LIST_DEVICES_GE1="yes"
fi

APP_LAUNCH_RETURNED_PID="no"
LAUNCH_PID="$(jq -r -s '[.[] | select(.id==4)][0].result.content[0].text | fromjson | .pid // empty' \
  < "$RESPONSES_FILE" 2>/dev/null || true)"
if [[ "${LAUNCH_PID:-}" =~ ^[0-9]+$ ]] && (( LAUNCH_PID > 0 )); then
  APP_LAUNCH_RETURNED_PID="yes"
fi

APP_TERMINATE_OK="yes"
TERMINATE_IS_ERROR="$(jq -r -s '[.[] | select(.id==5)][0].result.isError // false' \
  < "$RESPONSES_FILE" 2>/dev/null || echo "true")"
TERMINATE_OK_FIELD="$(jq -r -s '[.[] | select(.id==5)][0].result.content[0].text | fromjson | .ok // false' \
  < "$RESPONSES_FILE" 2>/dev/null || echo "false")"
if [[ "$TERMINATE_IS_ERROR" == "true" || "$TERMINATE_OK_FIELD" != "true" ]]; then
  APP_TERMINATE_OK="no"
fi

# schema_ok: tools/list returns all 27 expected names with object inputSchema
# (ping + 7 lifecycle + 4 observe + 7 interaction + 3 compound + 4 system + 1 vlm after v0.6 C6).
SCHEMA_OK="fail"
if jq -e -s '
  [.[] | select(.id==2)][0].result.tools
  | (map(.name) | sort) ==
    ["app_install","app_launch","app_terminate","app_uninstall",
     "double_tap",
     "element_inspect",
     "explain_screen",
     "fill",
     "find_and_tap","flow_run",
     "key_press",
     "long_press",
     "open_url",
     "pasteboard_get","pasteboard_set",
     "permissions_grant",
     "ping",
     "screen_describe","screen_hierarchy","screen_screenshot",
     "scroll_to",
     "simulator_boot","simulator_list","simulator_shutdown",
     "swipe",
     "tap",
     "wait_for"]
  and all(.[]; .inputSchema.type == "object")
' < "$RESPONSES_FILE" > /dev/null 2>&1; then
  SCHEMA_OK="ok"
fi

# stdout must be pure line-delimited JSON-RPC frames (no plain text leaking).
STDOUT_PURE_JSONRPC="fail"
if jq -e -s 'all(.jsonrpc == "2.0")' < "$RESPONSES_FILE" > /dev/null 2>&1; then
  STDOUT_PURE_JSONRPC="ok"
fi

STDERR_HAS_LOG="fail"
if grep -qE '\[simx-mcp\]' "$STDERR_FILE"; then
  STDERR_HAS_LOG="ok"
fi

# tools_count gate: must be exactly 27 (ping + 7 lifecycle + 4 observe + 7 interaction + 3 compound + 4 system + 1 vlm after v0.6 C6).
TOOLS_COUNT_OK_NUMERIC="$TOOLS_COUNT"

ALL_OK="fail"
if [[ "$EXIT_CODE" == "0" \
   && "$INITIALIZE_OK" == "ok" \
   && "$TOOLS_COUNT_OK_NUMERIC" == "27" \
   && "$SIMULATOR_LIST_DEVICES_GE1" == "yes" \
   && "$APP_LAUNCH_RETURNED_PID" == "yes" \
   && "$APP_TERMINATE_OK" == "yes" \
   && "$SCHEMA_OK" == "ok" \
   && "$STDOUT_PURE_JSONRPC" == "ok" \
   && "$STDERR_HAS_LOG" == "ok" ]]; then
  ALL_OK="ok"
fi

printf '{"exit_code":%s,"initialize_ok":"%s","tools_count":%s,"simulator_list_devices_ge1":"%s","app_launch_returned_pid":"%s","app_terminate_ok":"%s","schema_ok":"%s","stdout_pure_jsonrpc":"%s","stderr_has_log":"%s","all_ok":"%s"}\n' \
  "$EXIT_CODE" \
  "$INITIALIZE_OK" \
  "$TOOLS_COUNT_OK_NUMERIC" \
  "$SIMULATOR_LIST_DEVICES_GE1" \
  "$APP_LAUNCH_RETURNED_PID" \
  "$APP_TERMINATE_OK" \
  "$SCHEMA_OK" \
  "$STDOUT_PURE_JSONRPC" \
  "$STDERR_HAS_LOG" \
  "$ALL_OK"

if [[ "$ALL_OK" == "ok" ]]; then exit 0; else exit 1; fi
