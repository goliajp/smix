#!/usr/bin/env bash
# v0.7 C5 — Claude Code plugin manifest contract validator.
# Regex/jq-asserts `.claude-plugin/plugin.json` carries the 8-top-field shape
# (name=simx / description / version semver / mcpServers.simx entry) plus
# 3 mcpServers args sub-fields (command=bun / args[0] uses ${CLAUDE_PLUGIN_ROOT}
# and contains src/cli/index.ts / args[1]=mcp). Emits an 11-field JSON gate
# (mirror of scripts/simx-c3-ci-matrix-validate.sh shape; 11 fields cover
# manifest + mcpServers + 3 args sub-fields per decision 5.B/5.C).
#
# This validator does NOT invoke `claude` CLI — interactive `/mcp` stdout
# is not pipe-able. The intent here is plugin-manifest structural contract,
# not session-level MCP enumeration. Real `/mcp` enumeration is verified
# manually per docs/plugin-install.md §3 step (a).
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

MANIFEST=".claude-plugin/plugin.json"

manifest_exists="fail"
valid_json="fail"
has_name="fail"
has_description="fail"
has_version="fail"
has_mcp_servers="fail"
mcp_command_bun="fail"
mcp_args_has_cli="fail"
mcp_args_has_mcp_sub="fail"
mcp_args_uses_plugin_root="fail"

if [[ -f "$MANIFEST" ]]; then
  manifest_exists="ok"
  if jq -e . "$MANIFEST" > /dev/null 2>&1; then
    valid_json="ok"
    jq -e '.name == "simx"' "$MANIFEST" > /dev/null 2>&1 && has_name="ok"
    jq -e '.description | type == "string" and (length >= 20)' "$MANIFEST" > /dev/null 2>&1 \
      && has_description="ok"
    jq -e '.version | type == "string" and test("^[0-9]+\\.[0-9]+\\.[0-9]+$")' "$MANIFEST" \
      > /dev/null 2>&1 && has_version="ok"
    jq -e '.mcpServers.simx | type == "object"' "$MANIFEST" > /dev/null 2>&1 \
      && has_mcp_servers="ok"
    jq -e '.mcpServers.simx.command == "bun"' "$MANIFEST" > /dev/null 2>&1 \
      && mcp_command_bun="ok"
    jq -e '.mcpServers.simx.args[0] | type == "string" and contains("src/cli/index.ts")' \
      "$MANIFEST" > /dev/null 2>&1 && mcp_args_has_cli="ok"
    jq -e '.mcpServers.simx.args[1] == "mcp"' "$MANIFEST" > /dev/null 2>&1 \
      && mcp_args_has_mcp_sub="ok"
    jq -e '.mcpServers.simx.args[0] | type == "string" and contains("${CLAUDE_PLUGIN_ROOT}")' \
      "$MANIFEST" > /dev/null 2>&1 && mcp_args_uses_plugin_root="ok"
  fi
fi

EXIT_CODE=0
ALL_OK="ok"
for v in "$manifest_exists" "$valid_json" "$has_name" "$has_description" \
         "$has_version" "$has_mcp_servers" "$mcp_command_bun" \
         "$mcp_args_has_cli" "$mcp_args_has_mcp_sub" \
         "$mcp_args_uses_plugin_root"; do
  if [[ "$v" != "ok" ]]; then ALL_OK="fail"; EXIT_CODE=1; fi
done

printf '{"exit_code":%d,"manifest_exists":"%s","valid_json":"%s","has_name":"%s","has_description":"%s","has_version":"%s","has_mcp_servers":"%s","mcp_command_bun":"%s","mcp_args_has_cli":"%s","mcp_args_has_mcp_sub":"%s","mcp_args_uses_plugin_root":"%s","all_ok":"%s"}\n' \
  "$EXIT_CODE" "$manifest_exists" "$valid_json" "$has_name" "$has_description" \
  "$has_version" "$has_mcp_servers" "$mcp_command_bun" "$mcp_args_has_cli" \
  "$mcp_args_has_mcp_sub" "$mcp_args_uses_plugin_root" "$ALL_OK"

exit "$EXIT_CODE"
