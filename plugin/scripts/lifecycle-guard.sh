#!/usr/bin/env bash
#
# lifecycle-guard — PreToolUse Bash hook: devices are started, stopped,
# erased and created through smix, which keeps who booted each one, who
# may shut it down and its PIN. A bare `emulator -avd`, `qemu-system-*`,
# `adb emu kill`, `avdmanager create|delete` or `simctl boot|shutdown|
# erase|create|delete|clone` is refused (exit 2) with the smix command to
# use instead. One command may go around it with a reason, which smix
# logs: `SMIX_FENCE_EXCEPTION="<why>" <command>`.
#
# The judgement is the installed smix binary's (`smix guard lifecycle`),
# so the rules update with smix and not with this plugin. This file only
# hands the command over. When there is no smix on PATH, or one too old
# to have `guard`, a coarse copy below still refuses the commands that
# start a device: a fence that opens whenever smix is missing is not one.

set -uo pipefail

here="$(dirname "$0")"
command="$(python3 "$here/hook-command.py" 2>/dev/null || true)"

# Fast path: most commands name no device tool at all.
case "$command" in
  *simctl*|*emulator*|*qemu-system-*|*avdmanager*|*emu\ kill*) ;;
  *) exit 0 ;;
esac

if command -v smix >/dev/null 2>&1; then
  printf '%s' "$command" | smix guard lifecycle
  case $? in
    0) exit 0 ;;
    3) exit 2 ;;
  esac
fi

# No smix here that can judge. Refuse a segment that runs a lifecycle step,
# unless it carries a non-empty exception (which nothing can log here).
segment_starts_a_device() {
  printf '%s\n' "$1" | tr ';&|()`' '\n\n\n\n\n\n' | grep -v 'SMIX_FENCE_EXCEPTION=[^[:space:]]' | grep -qE \
    '^[[:space:]]*([A-Za-z_][A-Za-z0-9_]*=[^[:space:]]*[[:space:]]+)*((sudo|nohup|env|nice|timeout|command|exec)[[:space:]]+)*((xcrun[[:space:]]+)?simctl[[:space:]]+(boot|shutdown|erase|create|delete|clone)([[:space:]]|$)|[^[:space:]]*/?emulator[[:space:]]+(.*[[:space:]])?(-avd|@[^[:space:]]+)|[^[:space:]]*qemu-system-|avdmanager[[:space:]]+(create|delete)|adb[[:space:]].*emu[[:space:]]+kill)'
}

if segment_starts_a_device "$command"; then
  echo "smix fence: this starts, stops, erases or creates a device outside smix, and there is no smix on PATH new enough to judge it (\`smix guard lifecycle\`). Use \`smix sim boot|shutdown|erase\`; install or update smix: npm install -g @goliapkg/smix-cli (or: cargo install smix-cli smix-mcp --locked)." >&2
  exit 2
fi
exit 0
