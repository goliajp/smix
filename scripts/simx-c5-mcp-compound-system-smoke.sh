#!/usr/bin/env bash
# v0.6 C5 MCP compound+system smoke gate
# - spawns `bun src/cli/index.ts mcp` as a real child process
# - writes JSON-RPC frames to stdin:
#     initialize / initialized notif / tools/list /
#     tools/call simulator_list /
#     tools/call pasteboard_set {udid, value:"simx-c5-marker-<epoch>"} /
#     tools/call pasteboard_get {udid} /
#     tools/call open_url {udid, url:"https://example.com"} /
#     tools/call wait_for {udid, selector:{text:"NonExistentMagicMarker_C5"}, timeoutMs:1000}
# - jq-verifies a 15-field gate.
# - pasteboard set+get exercises the simctl pbcopy/pbpaste round-trip (no runner
#   required; pure simctl IO).
# - open_url exercises simctl openurl (no runner required; foreground side
#   effect on Safari is acceptable for CI).
# - wait_for-nonexistent is the deterministic "must fail" assertion: the element
#   never exists so driver.waitFor times out → isError. We accept either
#   "timed out" or "runner not reachable" wording, since CI may lack a bound
#   SimxRunner (in which case driver.waitFor fails before timing out).
set -u
cd "$(dirname "$0")/.." || exit 99

RESPONSES_FILE="$(mktemp -t simx-c5-mcp.XXXXXX)"
STDERR_FILE="$(mktemp -t simx-c5-mcp-stderr.XXXXXX)"
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

# Unique marker so concurrent invocations on the same dev sim don't interfere.
MARKER="simx-c5-marker-$(date +%s)-$$"

REQUESTS=$(cat <<EOF
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"simx-c5-smoke","version":"0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"simulator_list","arguments":{}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"pasteboard_set","arguments":{"udid":"${TARGET_UDID}","value":"${MARKER}"}}}
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"pasteboard_get","arguments":{"udid":"${TARGET_UDID}"}}}
{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"open_url","arguments":{"udid":"${TARGET_UDID}","url":"https://example.com"}}}
{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"wait_for","arguments":{"udid":"${TARGET_UDID}","selector":{"text":"NonExistentMagicMarker_C5"},"timeoutMs":1000}}}
EOF
)

# Hold stdin open for a few seconds after the final frame so async handlers
# (especially wait_for which sleeps up to ~timeoutMs) complete before EOF
# triggers shutdown.
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

HAS_FIND_AND_TAP="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("find_and_tap")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_FIND_AND_TAP="ok"

HAS_WAIT_FOR="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("wait_for")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_WAIT_FOR="ok"

HAS_FLOW_RUN="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("flow_run")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_FLOW_RUN="ok"

HAS_OPEN_URL="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("open_url")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_OPEN_URL="ok"

HAS_PASTEBOARD_SET="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("pasteboard_set")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_PASTEBOARD_SET="ok"

HAS_PASTEBOARD_GET="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("pasteboard_get")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_PASTEBOARD_GET="ok"

HAS_PERMISSIONS_GRANT="no"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("permissions_grant")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_PERMISSIONS_GRANT="ok"

# pasteboard_roundtrip: pasteboard_set must succeed, then pasteboard_get must
# return the marker we just set. Equality is exact-string match.
PASTEBOARD_ROUNDTRIP="fail"
SET_IS_ERR="$(jq -r -s '[.[] | select(.id==4)][0].result.isError // false' \
  < "$RESPONSES_FILE" 2>/dev/null || echo "true")"
if [[ "$SET_IS_ERR" == "false" ]]; then
  GET_IS_ERR="$(jq -r -s '[.[] | select(.id==5)][0].result.isError // false' \
    < "$RESPONSES_FILE" 2>/dev/null || echo "true")"
  if [[ "$GET_IS_ERR" == "false" ]]; then
    GET_VALUE="$(jq -r -s '[.[] | select(.id==5)][0].result.content[0].text | fromjson | .value // ""' \
      < "$RESPONSES_FILE" 2>/dev/null || echo "")"
    if [[ "$GET_VALUE" == "$MARKER" ]]; then
      PASTEBOARD_ROUNDTRIP="ok"
    fi
  fi
fi

# open_url: simctl openurl is a synchronous side effect on Safari. Non-isError → ok.
OPEN_URL_OK="fail"
OPEN_URL_IS_ERR="$(jq -r -s '[.[] | select(.id==6)][0].result.isError // false' \
  < "$RESPONSES_FILE" 2>/dev/null || echo "true")"
if [[ "$OPEN_URL_IS_ERR" == "false" ]]; then
  OPEN_URL_OK="ok"
fi

# wait_for-nonexistent: this MUST surface as isError. Either driver.waitFor
# times out (preferred) or runner-not-reachable bubbles up — both are accepted
# since the only failure mode we forbid is "wait_for silently succeeded on a
# nonexistent element".
WAIT_FOR_TIMEOUT_IS_ERROR="fail"
WF_IS_ERR="$(jq -r -s '[.[] | select(.id==7)][0].result.isError // false' \
  < "$RESPONSES_FILE" 2>/dev/null || echo "false")"
if [[ "$WF_IS_ERR" == "true" ]]; then
  WAIT_FOR_TIMEOUT_IS_ERROR="ok"
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
   && "$HAS_FIND_AND_TAP" == "ok" \
   && "$HAS_WAIT_FOR" == "ok" \
   && "$HAS_FLOW_RUN" == "ok" \
   && "$HAS_OPEN_URL" == "ok" \
   && "$HAS_PASTEBOARD_SET" == "ok" \
   && "$HAS_PASTEBOARD_GET" == "ok" \
   && "$HAS_PERMISSIONS_GRANT" == "ok" \
   && "$PASTEBOARD_ROUNDTRIP" == "ok" \
   && "$OPEN_URL_OK" == "ok" \
   && "$WAIT_FOR_TIMEOUT_IS_ERROR" == "ok" \
   && "$STDOUT_PURE_JSONRPC" == "ok" ]]; then
  ALL_OK="ok"
fi

printf '{"exit_code":%s,"initialize_ok":"%s","tools_count":%s,"has_find_and_tap":"%s","has_wait_for":"%s","has_flow_run":"%s","has_open_url":"%s","has_pasteboard_set":"%s","has_pasteboard_get":"%s","has_permissions_grant":"%s","pasteboard_roundtrip":"%s","open_url_ok":"%s","wait_for_timeout_isError":"%s","stdout_pure_jsonrpc":"%s","all_ok":"%s"}\n' \
  "$EXIT_CODE" \
  "$INITIALIZE_OK" \
  "$TOOLS_COUNT" \
  "$HAS_FIND_AND_TAP" \
  "$HAS_WAIT_FOR" \
  "$HAS_FLOW_RUN" \
  "$HAS_OPEN_URL" \
  "$HAS_PASTEBOARD_SET" \
  "$HAS_PASTEBOARD_GET" \
  "$HAS_PERMISSIONS_GRANT" \
  "$PASTEBOARD_ROUNDTRIP" \
  "$OPEN_URL_OK" \
  "$WAIT_FOR_TIMEOUT_IS_ERROR" \
  "$STDOUT_PURE_JSONRPC" \
  "$ALL_OK"

if [[ "$ALL_OK" == "ok" ]]; then exit 0; else exit 1; fi
