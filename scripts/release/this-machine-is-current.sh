#!/usr/bin/env bash
# Is the machine that shipped this version running it?
#
# The release list said "install it the way a user would" and that was
# the whole of the mechanism: a paragraph. 10.0.0 shipped on 2026-09-02
# and sixteen days later this machine had `smix` 9.0.0, no `smix-mcp` at
# all, and the plugin at 6.0.0, 6.8.0, 9.0.0 and 10.0.0 depending on
# which Claude profile a session happened to start under. Nothing was
# red, because nothing asked.
#
# Two things made it easy to get half of:
#
#   - `smix` and `smix-mcp` are separate crates. `cargo install smix-cli`
#     leaves a machine with a CLI and a plugin that cannot start its
#     server, and the two are only ever useful together.
#   - A plugin is installed per Claude profile (`CLAUDE_CONFIG_DIR`).
#     `claude plugin update` moves the one profile it ran under; the rest
#     keep whatever they had, and look identical from inside any one of
#     them.
#
# So this asks each binary and each profile what it has, and compares.
#
# Usage:
#   bash scripts/release/this-machine-is-current.sh <VERSION>
#   bash scripts/release/this-machine-is-current.sh --profiles
#   bash scripts/release/this-machine-is-current.sh --selftest
set -euo pipefail

PLUGIN_KEY="smix@goliajp"
# Where Claude profiles live. Overridable because the selftest builds its
# own, and must be judged by the same code the real run uses.
PROFILE_HOME="${SMIX_PROFILE_HOME:-$HOME}"

# Matched against a version shape, not filtered down to digits — see
# plugin/scripts/readiness.sh for the JSON-RPC error code that once came
# out of a `tr -cd` as a version number.
version_of() {
  command -v "$1" >/dev/null 2>&1 || return 0
  "$1" --version 2>/dev/null \
    | head -1 \
    | grep -oE '[0-9]+\.[0-9]+\.[0-9]+[0-9A-Za-z.+-]*' \
    | head -1 || true
}

# One line per profile that has the plugin: "<dir>\t<version>".
profiles_with_plugin() {
  local d f
  for d in "$PROFILE_HOME"/.claude "$PROFILE_HOME"/.claude-profile-*; do
    f="$d/plugins/installed_plugins.json"
    [ -r "$f" ] || continue
    python3 - "$d" "$f" "$PLUGIN_KEY" <<'PY'
import json, sys
d, f, key = sys.argv[1:4]
for entry in json.load(open(f)).get("plugins", {}).get(key, []):
    print(f"{d}\t{entry['version']}")
PY
  done
}

judge() {
  local version="$1" fail=0 name got count=0 dir
  for name in smix smix-mcp; do
    got="$(version_of "$name")"
    if [ "$got" = "$version" ]; then
      echo "this-machine-is-current: $name is $version"
    else
      echo "this-machine-is-current: FAIL — $name is ${got:-not on PATH (or reported no version)}, not $version" >&2
      fail=1
    fi
  done

  while IFS=$'\t' read -r dir got; do
    [ -n "$dir" ] || continue
    count=$((count + 1))
    if [ "$got" = "$version" ]; then
      echo "this-machine-is-current: plugin in $dir is $version"
    else
      echo "this-machine-is-current: FAIL — plugin in $dir is $got, not $version" >&2
      fail=1
    fi
  done < <(profiles_with_plugin)

  # A walk over no profiles passes every comparison it never made.
  if [ "$count" = 0 ]; then
    echo "this-machine-is-current: FAIL — no Claude profile under $PROFILE_HOME has $PLUGIN_KEY installed, so the plugin half of this check read nothing" >&2
    fail=1
  fi

  if [ "$fail" != 0 ]; then
    echo "  bring it up to date: bash scripts/release/install-on-this-machine.sh $version" >&2
  fi
  return "$fail"
}

# A machine built in a temp dir, judged by running this very script
# against it — the fixture and the real run go through the same code.
selftest() {
  local self tmp bin home rc fail=0
  self="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  tmp="$(mktemp -d)"
  bin="$tmp/bin"; home="$tmp/home"
  mkdir -p "$bin"
  ln -s "$(command -v python3)" "$bin/python3"

  fake_binary() { printf '#!/bin/sh\necho "%s %s"\n' "$1" "$2" > "$bin/$1"; chmod +x "$bin/$1"; }
  fake_profile() {
    mkdir -p "$home/$1/plugins"
    printf '{"plugins":{"%s":[{"version":"%s"}]}}' "$2" "$3" > "$home/$1/plugins/installed_plugins.json"
  }
  expect() {
    local want="$1" what="$2"
    rc=0
    PATH="$bin:/usr/bin:/bin" SMIX_PROFILE_HOME="$home" bash "$self" 4.0.0 >/dev/null 2>&1 || rc=$?
    if [ "$rc" != "$want" ]; then
      echo "selftest: $what — expected exit $want, got $rc" >&2
      fail=1
    fi
  }

  fake_binary smix 4.0.0; fake_binary smix-mcp 4.0.0
  fake_profile .claude-profile-1 "$PLUGIN_KEY" 4.0.0
  fake_profile .claude-profile-2 "$PLUGIN_KEY" 4.0.0
  expect 0 "a machine that is current"

  rm "$bin/smix-mcp"
  expect 1 "smix without smix-mcp (the cargo-install-smix-cli machine)"
  fake_binary smix-mcp 3.0.0
  expect 1 "smix-mcp one version behind"
  fake_binary smix-mcp 4.0.0

  fake_profile .claude-profile-2 "$PLUGIN_KEY" 3.0.0
  expect 1 "one profile's plugin left behind"

  rm -rf "$home"
  fake_profile .claude-profile-1 "someone-else@elsewhere" 4.0.0
  expect 1 "no profile has the plugin"

  rm -rf "$tmp"
  [ "$fail" = 0 ] || exit 1
  echo "this-machine-is-current selftest: a missing smix-mcp, a stale binary, a stale profile and an empty walk are each refused"
}

case "${1:-}" in
  --selftest) selftest ;;
  --profiles) profiles_with_plugin | cut -f1 ;;
  "") echo "usage: this-machine-is-current.sh <VERSION> | --profiles | --selftest" >&2; exit 2 ;;
  *) judge "$1" ;;
esac
