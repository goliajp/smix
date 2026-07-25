#!/usr/bin/env bash
# Table-driven test for the plugin's SessionStart readiness hook.
#
# The hook is what someone sees when the plugin is installed and smix is
# not — the case where every other part of this is silent, because an MCP
# server whose command does not exist produces no tools and no explanation.
# So what it says is the whole product in that moment, and it is worth a
# test that reads its actual output rather than trusting it was written.
#
# Device-free: the hook is told where to look via PATH, so a fake PATH is
# the whole fixture.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HOOK="$ROOT/plugin/scripts/readiness.sh"

pass=0
fail=0

check() {
  local name="$1" expected="$2" actual="$3"
  if [[ "$actual" == *"$expected"* ]]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FAIL: %s\n  expected to contain: %s\n  got: %s\n' "$name" "$expected" "$actual" >&2
  fi
}

[ -x "$HOOK" ] || { echo "readiness hook missing or not executable: $HOOK" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# A plugin root whose manifest declares a version, so the hook has
# something to compare a binary against.
mkdir -p "$WORK/plugin/.claude-plugin"
printf '{"name":"smix","version":"2.0.0"}\n' > "$WORK/plugin/.claude-plugin/plugin.json"

# --- fake binaries -------------------------------------------------------

fake_bin() { # <dir> <name> <version-output>
  mkdir -p "$1"
  printf '#!/bin/sh\necho "%s"\n' "$3" > "$1/$2"
  chmod +x "$1/$2"
}

run_hook() { # <PATH>
  env PATH="$1" CLAUDE_PLUGIN_ROOT="$WORK/plugin" \
    bash "$HOOK" 2>&1 <<< '{"hook_event_name":"SessionStart"}'
}

# 1. both present, versions agree
BOTH="$WORK/bin-both"
fake_bin "$BOTH" smix "smix 2.0.0"
fake_bin "$BOTH" smix-mcp "smix-mcp 2.0.0"
OUT="$(run_hook "$BOTH:/usr/bin:/bin")"
check "ready says which version is driving" "2.0.0" "$OUT"

# The hook must never block a session: a missing binary is a thing to
# report, not a reason the conversation cannot start.
set +e
run_hook "$BOTH:/usr/bin:/bin" >/dev/null 2>&1
rc=$?
set -e
check "ready exits 0" "0" "$rc"

# 2. the MCP server is absent — the case that otherwise looks like nothing
ONLY_CLI="$WORK/bin-cli"
fake_bin "$ONLY_CLI" smix "smix 2.0.0"
OUT="$(run_hook "$ONLY_CLI:/usr/bin:/bin")"
check "missing server names the install command" "npm install -g @goliapkg/smix-cli" "$OUT"
set +e
run_hook "$ONLY_CLI:/usr/bin:/bin" >/dev/null 2>&1
rc=$?
set -e
check "missing server still exits 0" "0" "$rc"

# 3. nothing installed at all
OUT="$(run_hook "/usr/bin:/bin")"
check "nothing installed names the install command" "@goliapkg/smix-cli" "$OUT"

# 4. version skew — both numbers, and which way to move
SKEW="$WORK/bin-skew"
fake_bin "$SKEW" smix "smix 1.0.27"
fake_bin "$SKEW" smix-mcp "smix-mcp 1.0.27"
OUT="$(run_hook "$SKEW:/usr/bin:/bin")"
check "skew names the installed version" "1.0.27" "$OUT"
check "skew names the expected version" "2.0.0" "$OUT"
check "skew says what to do" "update" "$OUT"

# 5. The real binaries, because the fakes above encode an assumption.
#
# Every case so far hands the hook a stand-in that echoes a version on
# demand. That is what let this ship believing `smix-mcp --version` worked
# when the server had no such flag: it answered an empty stdin as a
# request and printed a JSON-RPC parse error, whose -32700 came out of the
# old digit filter as "2.032700" — a version manufactured from noise, and
# a reported mismatch that did not exist. A fake can only ever confirm the
# shape you already believed.
REAL_BIN="$ROOT/target/release"
if [ -x "$REAL_BIN/smix" ] && [ -x "$REAL_BIN/smix-mcp" ]; then
  REAL_VERSION="$("$REAL_BIN/smix" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)"
  printf '{"name":"smix","version":"%s"}\n' "$REAL_VERSION" \
    > "$WORK/plugin/.claude-plugin/plugin.json"
  OUT="$(run_hook "$REAL_BIN:/usr/bin:/bin")"
  check "the real binaries report ready" "smix $REAL_VERSION ready" "$OUT"
  case "$OUT" in
    *mismatch*|*expects*)
      echo "FAIL: the real pair reported a mismatch against its own version: $OUT" >&2
      fail=$((fail + 1)) ;;
    *) pass=$((pass + 1)) ;;
  esac
else
  echo "note: no release build at $REAL_BIN — the real-binary case was skipped" >&2
fi

printf 'plugin-readiness: %d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
