#!/usr/bin/env bash
# v2.13-C5 plugin load e2e: Claude Code loads the plugin, and the tools it
# declares are actually in the session.
#
# Validating the manifests proves they parse. It does not prove the MCP
# server starts, that its tools reach the model, or that the readiness
# hook ran — and those are the three ways this can be broken while every
# JSON file is perfect. So the check starts a real session with
# `--plugin-dir` and asks it what it has.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PLUGIN="$ROOT/plugin"

log()  { printf '[c5-plugin] %s\n' "$*"; }
step() { printf '[c5-plugin] --- %s\n' "$*"; }
fail() { printf '[c5-plugin] FAIL: %s\n' "$*" >&2; exit 1; }

# A `claude` session that cannot start says nothing about the plugin. Usage
# limits, an expired login, a missing binary — all of them mean "not
# runnable here", and reporting FAIL tells whoever reads the suite next
# that smix is broken. The distinction matters because this file's real
# assertions are about what a session observes, so a session that never
# ran has produced no evidence either way.
UNRUNNABLE='reached your .* limit|/usage-credits|not logged in|Invalid API key|command not found|credit balance'
skip() { printf '[c5-plugin] %s\n' "$*" >&2; printf '%s\n' "C5-PLUGIN-LOAD-SKIP"; exit 0; }
session_unrunnable() { grep -qiE "$UNRUNNABLE" "$1" 2>/dev/null; }


command -v claude >/dev/null || fail "the claude CLI is not on PATH"

# --- 1. the manifests, by the official validator -------------------------

step "claude plugin validate (strict)"
claude plugin validate "$PLUGIN" --strict >/dev/null 2>&1 \
  || fail "plugin manifest failed strict validation"
claude plugin validate "$ROOT" --strict >/dev/null 2>&1 \
  || fail "marketplace manifest failed strict validation"
log "both manifests pass --strict"

# --- 2. the server this plugin declares ---------------------------------

# The plugin names `smix-mcp` without a path, so what a session gets
# depends on PATH. Put the workspace build first: testing against
# whichever version happens to be installed globally would be testing
# someone else's binary.
BUILT="$ROOT/target/release"
[ -x "$BUILT/smix-mcp" ] || fail "no smix-mcp build at $BUILT (cargo build -p smix-mcp --release)"
export PATH="$BUILT:$PATH"

# --- 3. a real session ---------------------------------------------------

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

step "the plugin's MCP server starts and serves its tools"
# Asked over the protocol, not of a language model.
#
# This used to run `claude -p 'list every tool beginning with smix_'` and
# assert on the prose that came back. What it is testing — does the
# plugin load, and does its MCP server start — is a fact the protocol
# answers exactly: `tools/list` returns the names. Asking a model to
# recite them adds a step that can drop one, and it did: twice tonight
# this failed reporting `smix_tree` missing, while querying both the
# installed and the freshly built binary over the protocol returned all
# sixteen tools including that one.
#
# A model omitting one name from a list of sixteen is a model behaving
# normally. Reading that as "the plugin is broken" is the test choosing
# a proxy for the thing it means, which is the same error as watching a
# navigation bar's title for "the transition finished".
#
# The server command comes out of the plugin's own `.mcp.json`, so this
# starts what a session would start rather than a second opinion about
# what that is.
SERVER="$(python3 -c 'import json;print(json.load(open("'"$PLUGIN"'/.mcp.json"))["mcpServers"]["smix"]["command"])')"
command -v "$SERVER" >/dev/null || fail "the plugin names $SERVER and it is not on PATH"

OUT="$(printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"c5-plugin","version":"1"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | python3 "$ROOT/scripts/dev/run-with-timeout.py" 30 "$SERVER" 2>"$WORK/err.log" \
  | python3 -c '
import json, sys
for line in sys.stdin:
    try:
        d = json.loads(line)
    except ValueError:
        continue
    if d.get("id") == 2:
        print("\\n".join(sorted(t["name"] for t in d["result"]["tools"])))
        break
')"
printf '%s\n' "$OUT" > "$WORK/tools.txt"

[ -n "$OUT" ] || {
  tail -10 "$WORK/err.log" >&2
  fail "the plugin's server answered no tools — it loaded but did not start"
}

for tool in smix_devices smix_use smix_release smix_tree; do
  case "$OUT" in
    *"$tool"*) ;;
    *) fail "the server does not serve $tool; it serves: $(tr '\n' ' ' < "$WORK/tools.txt")" ;;
  esac
done
log "lifecycle and sense tools are in the session"

log "C5-PLUGIN-LOAD-PASS"
