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

step "start a session with --plugin-dir and ask what it has"
# `-p` for one non-interactive turn. The prompt asks for the tool names
# because that is the thing under test: a plugin that loads but whose MCP
# server never starts has no smix tools and says nothing about why.
OUT="$(cd "$WORK" && claude --plugin-dir "$PLUGIN" \
  --tools "" \
  -p 'List every tool name available to you that begins with smix_. Output only the names, one per line. If there are none, output the single word NONE.' \
  2>"$WORK/err.log")" || { tail -10 "$WORK/err.log" >&2; fail "the session did not complete"; }

printf '%s\n' "$OUT" > "$WORK/tools.txt"

case "$OUT" in
  *NONE*) fail "the session has no smix tools — the plugin loaded but its server did not" ;;
esac

for tool in smix_devices smix_use smix_release smix_tree; do
  case "$OUT" in
    *"$tool"*) ;;
    *) fail "the session is missing $tool; it reported: $(tr '\n' ' ' < "$WORK/tools.txt")" ;;
  esac
done
log "lifecycle and sense tools are in the session"

log "C5-PLUGIN-LOAD-PASS"
