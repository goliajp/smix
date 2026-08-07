#!/usr/bin/env bash
#
# Exercises sim-guard against the command shapes it exists to judge.
#
# It went untested from v5.x to here, which is how nobody noticed it
# read heredoc bodies as commands: writing a paragraph that mentioned
# `simctl shutdown all` was refused as if it were running one. The
# Android counterpart shipped with a harness on day one and that is
# where the shape was caught; this file closes the older gap.
#
# The UDID below is a syntactically valid placeholder, not a device on
# this machine — these cases never reach simctl.

set -uo pipefail

# The guard ships in the plugin, and the repo's own hook runs that same
# copy — one implementation, so a regression bites us before it bites
# anyone who installed it.
GUARD="$(cd "$(dirname "$0")/../.." && pwd)/plugin/scripts/sim-guard.sh"
UDID="A1B2C3D4-1111-2222-3333-444455556666"
fails=0

feed() {
  local want="$1"; shift
  local cmd="$1"
  local json got
  json="$(python3 -c 'import json,sys; print(json.dumps({"tool_input":{"command":sys.argv[1]}}))' "$cmd")"
  printf '%s' "$json" | bash "$GUARD" >/dev/null 2>&1
  got=$?
  if [ "$got" != "$want" ]; then
    echo "FAIL want=$want got=$got: $cmd"
    fails=$((fails + 1))
  fi
}

BLOCK=2
ALLOW=0

# --- must BLOCK: the three shapes the header names ---
feed $BLOCK "xcrun simctl boot booted"
feed $BLOCK "xcrun simctl io booted screenshot /tmp/shot.png"
feed $BLOCK "xcrun simctl shutdown all"
feed $BLOCK "xcrun simctl erase all"
feed $BLOCK "xcrun simctl delete all"
feed $BLOCK "xcrun simctl boot 'iPhone 17 Pro'"
feed $BLOCK "xcrun simctl launch \"sim-smix-001 spare\" com.example.app"

# --- must ALLOW: explicit UDID, and everything unrelated ---
feed $ALLOW "xcrun simctl boot $UDID"
feed $ALLOW "xcrun simctl shutdown $UDID"
feed $ALLOW "xcrun simctl io $UDID screenshot /tmp/shot.png"
feed $ALLOW "xcrun simctl list devices"
feed $ALLOW "xcrun simctl list runtimes -j"
feed $ALLOW "cargo test -p smix-simctl"
feed $ALLOW "git status"

# --- heredoc bodies: data vs code ---
# Documenting a dangerous invocation is not performing one.
feed $ALLOW "$(printf 'cat >> .claude/docs/v2.md <<%s\nsimctl shutdown all hits every simulator\nEOF\n' "'EOF'")"
# A shell reading its body is running it, so the body still counts.
feed $BLOCK "$(printf 'bash <<%s\nxcrun simctl shutdown all\nEOF\n' "'EOF'")"
# The line opening an inert heredoc is itself still judged.
feed $BLOCK "$(printf 'xcrun simctl erase all && cat <<%s\nharmless\nEOF\n' "'EOF'")"

SMIX_WAY_CASE='xcrun simctl shutdown all'
# A refusal that names no alternative gets routed around rather than
# obeyed, so the way out is part of what is under test.
refusal="$(printf '{"tool_name":"Bash","tool_input":{"command":"%s"}}' \
  "$(printf '%s' "$SMIX_WAY_CASE" | sed 's/"/\\"/g')" | bash "$GUARD" 2>&1 || true)"
case "$refusal" in
  *"smix sim list"*) ;;
  *) echo "FAIL: the refusal names no smix command; said: $refusal"; fails=$((fails + 1)) ;;
esac

if [ "$fails" -eq 0 ]; then
  echo "sim-guard: all cases pass"
  exit 0
fi
echo "sim-guard: $fails case(s) failed"
exit 1
