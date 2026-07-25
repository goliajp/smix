#!/usr/bin/env bash
# SessionStart: say whether smix is here, and if not, what to run.
#
# The MCP server this plugin declares is a binary the user installs
# separately. When it is absent, Claude Code starts the server, the server
# does not exist, and the session simply has no smix tools in it — no
# error, no explanation, nothing to search for. This hook is the only
# thing that speaks in that moment.
#
# It never exits non-zero. Blocking SessionStart would mean a missing
# binary stops the whole conversation, and "you cannot start work" is not
# a proportionate answer to "one dependency is not installed yet".
set -uo pipefail

INSTALL_HINT='npm install -g @goliapkg/smix-cli   (or: cargo install smix-cli --locked)'

say() { printf 'smix plugin: %s\n' "$*"; }

# `${CLAUDE_PLUGIN_ROOT}` rather than a path relative to this script: the
# plugin can be loaded from a marketplace checkout, a --plugin-dir, or a
# zip, and only the variable is right in all three.
ROOT="${CLAUDE_PLUGIN_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
MANIFEST="$ROOT/.claude-plugin/plugin.json"

expected=""
if [ -r "$MANIFEST" ]; then
  expected="$(python3 -c '
import json, sys
try:
    print(json.load(open(sys.argv[1])).get("version", ""))
except Exception:
    print("")
' "$MANIFEST" 2>/dev/null)"
fi

# The version each binary reports, or empty when it is not on PATH or
# does not answer with one.
#
# Matched against a version shape rather than filtered down to digits.
# Stripping non-digits out of arbitrary output manufactures a version from
# noise: `smix-mcp --version` once printed a JSON-RPC parse error whose
# code is -32700, which came out of a `tr -cd` as "2.032700" and had this
# hook reporting a mismatch that did not exist — and a session that then
# declined to use smix at all. Saying nothing is the honest answer when
# the binary did not answer.
version_of() {
  command -v "$1" >/dev/null 2>&1 || return 0
  "$1" --version 2>/dev/null \
    | head -1 \
    | grep -oE '[0-9]+\.[0-9]+\.[0-9]+[0-9A-Za-z.+-]*' \
    | head -1
}

cli_version="$(version_of smix)"
mcp_version="$(version_of smix-mcp)"

if [ -z "$mcp_version" ] && [ -z "$cli_version" ]; then
  say "smix is not installed, so this plugin has no tools to offer."
  say "install it: $INSTALL_HINT"
  exit 0
fi

if [ -z "$mcp_version" ]; then
  if command -v smix-mcp >/dev/null 2>&1; then
    # Present but not answering with a version. Older builds had no
    # --version at all; that is a reason to say so, not to claim the
    # server is missing or to invent a number for it.
    say "smix-mcp is installed but did not report a version, so it cannot be"
    say "checked against this plugin's $expected. If the tools misbehave, update: $INSTALL_HINT"
    exit 0
  fi
  # The likelier half to be missing: someone with `cargo install smix-cli`
  # has the CLI and not the MCP server, which is a separate binary.
  say "the smix CLI is here (${cli_version:-unknown}) but smix-mcp is not, and the"
  say "MCP server is what this plugin's tools run through."
  say "install both: $INSTALL_HINT"
  exit 0
fi

if [ -n "$expected" ] && [ "$mcp_version" != "$expected" ]; then
  say "installed smix-mcp is $mcp_version; this plugin expects $expected."
  say "one of them is behind — update whichever you pin: $INSTALL_HINT"
  say "or install the plugin version matching your smix."
  exit 0
fi

say "smix $mcp_version ready — call smix_devices to see what you can drive."
exit 0
