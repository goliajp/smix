#!/usr/bin/env bash
# v0.6 C1 MCP stdio JSON-RPC smoke gate
# - spawns `bun src/cli/index.ts mcp` as a real child process
# - writes 4 JSON-RPC frames to stdin (initialize + initialized notif +
#   tools/list + tools/call ping), then closes stdin to trigger EOF shutdown
# - reads stdout responses line-by-line (line-delimited JSON-RPC)
# - jq-verifies serverInfo, tools/list shape, ping pong, error handling
# - outputs a single-line 8-field JSON gate for CI
set -u
cd "$(dirname "$0")/.." || exit 99

RESPONSES_FILE="$(mktemp -t simx-c1-mcp.XXXXXX)"
STDERR_FILE="$(mktemp -t simx-c1-mcp-stderr.XXXXXX)"
trap 'rm -f "$RESPONSES_FILE" "$STDERR_FILE"' EXIT

# Frame 1: initialize. Frame 2: notifications/initialized (no id).
# Frame 3: tools/list. Frame 4: tools/call ping.
REQUESTS=$(cat <<'EOF'
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"simx-c1-smoke","version":"0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ping","arguments":{}}}
EOF
)

printf '%s\n' "$REQUESTS" \
  | bun src/cli/index.ts mcp \
    > "$RESPONSES_FILE" \
    2> "$STDERR_FILE"
EXIT_CODE=$?

INITIALIZE_OK="fail"
jq -e -s '[.[] | select(.id==1)][0].result.serverInfo.name == "simx"' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && INITIALIZE_OK="ok"

TOOLS_LIST_COUNT="fail"
# After v0.6 C6, tools/list returns ping + 7 lifecycle + 4 observe + 7 interaction + 3 compound + 4 system + 1 vlm (explain_screen) = 27.
COUNT="$(jq -s '[.[] | select(.id==2)][0].result.tools | length' \
  < "$RESPONSES_FILE" 2>/dev/null || echo "-1")"
if [[ "$COUNT" == "27" ]]; then TOOLS_LIST_COUNT="ok"; fi

TOOLS_LIST_HAS_PING="fail"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("ping")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && TOOLS_LIST_HAS_PING="ok"

PING_CALL_OK="fail"
jq -e -s '[.[] | select(.id==3)][0].result.content[0].type == "text"' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && PING_CALL_OK="ok"

PING_CALL_TEXT="fail"
jq -e -s '[.[] | select(.id==3)][0].result.content[0].text == "pong"' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && PING_CALL_TEXT="ok"

# stdout must be pure line-delimited JSON-RPC (no plain text logs leaking)
STDOUT_PURE_JSONRPC="fail"
if jq -e -s 'all(.jsonrpc == "2.0")' < "$RESPONSES_FILE" > /dev/null 2>&1; then
  STDOUT_PURE_JSONRPC="ok"
fi

# stderr must carry the simx-mcp log prefix at least once (log → stderr, not stdout)
STDERR_HAS_LOG="fail"
if grep -qE '\[simx-mcp\]' "$STDERR_FILE"; then
  STDERR_HAS_LOG="ok"
fi

# unknown method handling — fresh child process. Send a single unknown method
# request, expect JSON-RPC error with code -32601 (Method not found).
UNKNOWN_RESP_FILE="$(mktemp -t simx-c1-mcp-unknown.XXXXXX)"
UNKNOWN_REQUEST='{"jsonrpc":"2.0","id":99,"method":"some_unknown_method_xyz","params":{}}'
printf '%s\n' "$UNKNOWN_REQUEST" \
  | bun src/cli/index.ts mcp \
    > "$UNKNOWN_RESP_FILE" \
    2>/dev/null

UNKNOWN_METHOD_NEG32601="fail"
if jq -e -s '[.[] | select(.id==99)][0].error.code == -32601' \
  < "$UNKNOWN_RESP_FILE" > /dev/null 2>&1; then
  UNKNOWN_METHOD_NEG32601="ok"
fi
rm -f "$UNKNOWN_RESP_FILE"

# malformed JSON — the SDK must not emit anything that breaks line-delimited JSON
# on stdout, and the process must still terminate cleanly on stdin EOF.
MALFORMED_RESP_FILE="$(mktemp -t simx-c1-mcp-malformed.XXXXXX)"
printf '%s\n' '{not valid json}' \
  | bun src/cli/index.ts mcp \
    > "$MALFORMED_RESP_FILE" \
    2>/dev/null
MALFORMED_EXIT=$?
MALFORMED_HANDLED="fail"
# Pass if server didn't crash (exit 0) and stdout (if any) is either empty or
# valid JSON-RPC (the SDK silently drops unparseable input).
if [[ "$MALFORMED_EXIT" == "0" ]]; then
  if [[ ! -s "$MALFORMED_RESP_FILE" ]] \
    || jq -e -s 'all(.jsonrpc == "2.0")' < "$MALFORMED_RESP_FILE" > /dev/null 2>&1; then
    MALFORMED_HANDLED="ok"
  fi
fi
rm -f "$MALFORMED_RESP_FILE"

ALL_OK="fail"
if [[ "$EXIT_CODE" == "0" \
   && "$INITIALIZE_OK" == "ok" \
   && "$TOOLS_LIST_COUNT" == "ok" \
   && "$TOOLS_LIST_HAS_PING" == "ok" \
   && "$PING_CALL_OK" == "ok" \
   && "$PING_CALL_TEXT" == "ok" \
   && "$STDOUT_PURE_JSONRPC" == "ok" \
   && "$STDERR_HAS_LOG" == "ok" \
   && "$UNKNOWN_METHOD_NEG32601" == "ok" \
   && "$MALFORMED_HANDLED" == "ok" ]]; then
  ALL_OK="ok"
fi

printf '{"exit_code":%s,"initialize_ok":"%s","tools_list_count":"%s","tools_list_has_ping":"%s","ping_call_ok":"%s","ping_call_text":"%s","stdout_pure_jsonrpc":"%s","stderr_has_log":"%s","unknown_method_neg32601":"%s","malformed_handled":"%s","all_ok":"%s"}\n' \
  "$EXIT_CODE" \
  "$INITIALIZE_OK" \
  "$TOOLS_LIST_COUNT" \
  "$TOOLS_LIST_HAS_PING" \
  "$PING_CALL_OK" \
  "$PING_CALL_TEXT" \
  "$STDOUT_PURE_JSONRPC" \
  "$STDERR_HAS_LOG" \
  "$UNKNOWN_METHOD_NEG32601" \
  "$MALFORMED_HANDLED" \
  "$ALL_OK"

if [[ "$ALL_OK" == "ok" ]]; then exit 0; else exit 1; fi
