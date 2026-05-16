#!/usr/bin/env bash
# v0.6 C6 MCP explain_screen e2e smoke gate
# - spawns `bun src/cli/index.ts mcp` as a real child process
# - writes JSON-RPC frames to stdin: initialize / initialized notif / tools/list
#   + tools/call explain_screen on a booted sim (happy path; skipped when
#     `claude` is not in PATH or no booted sim)
# - runs a second child with PATH stripped of claude to verify the
#   not-installed path returns isError + "not found" text
# - jq-verifies an 8-field gate
set -u
cd "$(dirname "$0")/.." || exit 99

RESPONSES_FILE="$(mktemp -t simx-c6-mcp.XXXXXX)"
STDERR_FILE="$(mktemp -t simx-c6-mcp-stderr.XXXXXX)"
NOT_INSTALLED_RESP="$(mktemp -t simx-c6-mcp-notinst.XXXXXX)"
NOT_INSTALLED_STDERR="$(mktemp -t simx-c6-mcp-notinst-stderr.XXXXXX)"
trap 'rm -f "$RESPONSES_FILE" "$STDERR_FILE" "$NOT_INSTALLED_RESP" "$NOT_INSTALLED_STDERR"' EXIT

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

HAS_BOOTED_SIM="no"
BOOTED_UDID=""
if [[ -n "${TARGET_UDID}" ]] \
  && xcrun simctl list devices -j 2>/dev/null \
    | jq -e --arg u "$TARGET_UDID" '
        any(.devices[][]; .udid == $u and .state == "Booted")
      ' > /dev/null 2>&1; then
  HAS_BOOTED_SIM="yes"
  BOOTED_UDID="$TARGET_UDID"
fi

HAS_CLAUDE="no"
if command -v claude >/dev/null 2>&1; then
  HAS_CLAUDE="yes"
fi

# ============================================================
# Run 1: real server with full PATH. Always lists tools.
# Optionally exercises the happy path when claude + booted sim are present.
# ============================================================
if [[ "$HAS_CLAUDE" == "yes" && "$HAS_BOOTED_SIM" == "yes" ]]; then
  REQUESTS=$(cat <<EOF
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"simx-c6-smoke","version":"0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"explain_screen","arguments":{"udid":"${BOOTED_UDID}","timeoutMs":120000}}}
EOF
  )
  EXPLAIN_HAPPY_RAN="yes"
  # Hold stdin open ~150s to let the real claude call complete.
  ( printf '%s\n' "$REQUESTS"; sleep 150 ) \
    | bun src/cli/index.ts mcp \
      > "$RESPONSES_FILE" \
      2> "$STDERR_FILE"
  EXIT_CODE=$?
else
  REQUESTS=$(cat <<EOF
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"simx-c6-smoke","version":"0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
EOF
  )
  EXPLAIN_HAPPY_RAN="no"
  printf '%s\n' "$REQUESTS" \
    | bun src/cli/index.ts mcp \
      > "$RESPONSES_FILE" \
      2> "$STDERR_FILE"
  EXIT_CODE=$?
fi

INITIALIZE_OK="fail"
jq -e -s '[.[] | select(.id==1)][0].result.serverInfo.name == "simx"' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && INITIALIZE_OK="ok"

TOOLS_COUNT_RAW="$(jq -s '[.[] | select(.id==2)][0].result.tools | length' \
  < "$RESPONSES_FILE" 2>/dev/null || echo "-1")"
TOOLS_COUNT="${TOOLS_COUNT_RAW:--1}"

HAS_EXPLAIN_SCREEN="fail"
jq -e -s '[.[] | select(.id==2)][0].result.tools | map(.name) | index("explain_screen")' \
  < "$RESPONSES_FILE" > /dev/null 2>&1 && HAS_EXPLAIN_SCREEN="ok"

SCHEMA_OK="fail"
if jq -e -s '
    [.[] | select(.id==2)][0].result.tools
    | map(select(.name == "explain_screen"))[0]
    | (.inputSchema.required // []) | index("udid")
  ' < "$RESPONSES_FILE" > /dev/null 2>&1; then
  SCHEMA_OK="ok"
fi

# happy_path: skipped when claude or booted sim absent.
HAPPY_PATH="skipped"
if [[ "$EXPLAIN_HAPPY_RAN" == "yes" ]]; then
  HAPPY_IS_ERR="$(jq -r -s '[.[] | select(.id==3)][0].result.isError // false' \
    < "$RESPONSES_FILE" 2>/dev/null || echo "true")"
  if [[ "$HAPPY_IS_ERR" == "false" ]]; then
    DESC_LEN="$(jq -r -s '[.[] | select(.id==3)][0].result.content[0].text | fromjson | .description | length' \
      < "$RESPONSES_FILE" 2>/dev/null || echo "0")"
    if [[ "${DESC_LEN:-0}" -gt 0 ]]; then
      HAPPY_PATH="ok"
    else
      HAPPY_PATH="fail"
    fi
  else
    HAPPY_PATH="fail"
  fi
fi

STDOUT_PURE_JSONRPC="fail"
if jq -e -s 'all(.jsonrpc == "2.0")' < "$RESPONSES_FILE" > /dev/null 2>&1; then
  STDOUT_PURE_JSONRPC="ok"
fi

# ============================================================
# Run 2: claude not installed (PATH stripped). Always runs.
# We feed simulator_list first to grab a udid, then explain_screen, expecting
# isError with the "not found" text.
# ============================================================
PROBE_UDID="${BOOTED_UDID:-00000000-0000-0000-0000-000000000000}"
NOT_INSTALLED_REQUESTS=$(cat <<EOF
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"simx-c6-smoke-noclaude","version":"0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"explain_screen","arguments":{"udid":"${PROBE_UDID}","timeoutMs":5000}}}
EOF
)

# Strip claude from PATH. Keep bun + xcrun + jq accessible via /usr/bin:/bin
# + the Homebrew prefix (bun lives there on this dev machine). The whole
# point is to make `claude` resolve to ENOENT inside the spawned bun process.
SAFE_PATH="/usr/bin:/bin:/usr/sbin:/sbin"
# Add bun's directory if needed
BUN_BIN="$(command -v bun || true)"
if [[ -n "$BUN_BIN" ]]; then
  SAFE_PATH="${SAFE_PATH}:$(dirname "$BUN_BIN")"
fi
# Make sure claude *does* resolve to ENOENT under SAFE_PATH (defensive: if a
# `claude` binary lives in /usr/bin we abort with a fail signal).
NOT_INSTALLED_PATH="fail"
if env -i HOME="$HOME" PATH="$SAFE_PATH" command -v claude >/dev/null 2>&1; then
  : # claude still resolvable; cannot run the not-installed assertion cleanly
else
  ( printf '%s\n' "$NOT_INSTALLED_REQUESTS"; sleep 10 ) \
    | env -i HOME="$HOME" PATH="$SAFE_PATH" bun src/cli/index.ts mcp \
      > "$NOT_INSTALLED_RESP" \
      2> "$NOT_INSTALLED_STDERR"
  NI_EXIT=$?
  NI_IS_ERR="$(jq -r -s '[.[] | select(.id==2)][0].result.isError // false' \
    < "$NOT_INSTALLED_RESP" 2>/dev/null || echo "false")"
  NI_TEXT="$(jq -r -s '[.[] | select(.id==2)][0].result.content[0].text // ""' \
    < "$NOT_INSTALLED_RESP" 2>/dev/null || echo "")"
  if [[ "$NI_EXIT" == "0" && "$NI_IS_ERR" == "true" ]] \
    && [[ "$NI_TEXT" == *"not found"* ]]; then
    NOT_INSTALLED_PATH="ok"
  fi
fi

ALL_OK="fail"
if [[ "$EXIT_CODE" == "0" \
   && "$INITIALIZE_OK" == "ok" \
   && "$TOOLS_COUNT" == "27" \
   && "$HAS_EXPLAIN_SCREEN" == "ok" \
   && "$SCHEMA_OK" == "ok" \
   && ( "$HAPPY_PATH" == "ok" || "$HAPPY_PATH" == "skipped" ) \
   && "$NOT_INSTALLED_PATH" == "ok" \
   && "$STDOUT_PURE_JSONRPC" == "ok" ]]; then
  ALL_OK="ok"
fi

printf '{"exit_code":%s,"initialize_ok":"%s","tools_count":%s,"has_explain_screen":"%s","schema_ok":"%s","happy_path":"%s","not_installed_path":"%s","stdout_pure_jsonrpc":"%s","all_ok":"%s"}\n' \
  "$EXIT_CODE" \
  "$INITIALIZE_OK" \
  "$TOOLS_COUNT" \
  "$HAS_EXPLAIN_SCREEN" \
  "$SCHEMA_OK" \
  "$HAPPY_PATH" \
  "$NOT_INSTALLED_PATH" \
  "$STDOUT_PURE_JSONRPC" \
  "$ALL_OK"

if [[ "$ALL_OK" == "ok" ]]; then exit 0; else exit 1; fi
