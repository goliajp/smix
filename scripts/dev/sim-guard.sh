#!/usr/bin/env bash
#
# sim-guard — PreToolUse Bash hook enforcing explicit-UDID simulator
# addressing. Wired from `.claude/settings.json`; receives the hook
# JSON on stdin, inspects `tool_input.command`, and BLOCKS (exit 2)
# any `simctl` invocation that:
#
#   1. uses the `booted` placeholder (ambiguous when >1 sim is booted;
#      has hit the wrong device in past sessions), or
#   2. uses a global verb over every device (`shutdown all`,
#      `erase all`, `delete all` — blast radius beyond the project's
#      own sims), or
#   3. addresses a device by NAME (names are not stable; the registry
#      `.smix/sims.json` maps names → UDIDs, resolve there first).
#
# Everything else passes (exit 0). Non-simctl commands are never
# touched. Keep this file committed — the hook config referencing it
# lives in version control, and a missing script surfaces as a
# non-blocking hook error on EVERY Bash call.
#
# History note: the original v5.x implementation was never committed
# and was lost with the local checkout; recreated 2026-07-13 from the
# documented semantics (memory: simx_sim_guard_hook).

set -euo pipefail

payload="$(cat)"

command="$(printf '%s' "$payload" | python3 -c '
import json, sys
try:
    print(json.load(sys.stdin).get("tool_input", {}).get("command", ""))
except Exception:
    print("")
' 2>/dev/null || true)"

# Fast path: nothing simctl-shaped → allow.
case "$command" in
  *simctl*) ;;
  *) exit 0 ;;
esac

deny() {
  echo "sim-guard: $1" >&2
  echo "sim-guard: use an explicit UDID (see .smix/sims.json registry); never 'booted' / 'all' / device names" >&2
  exit 2
}

# 1. `booted` placeholder as a device argument.
if printf '%s' "$command" | grep -qE 'simctl[^|;&]*[[:space:]]booted([[:space:]]|$)'; then
  deny "'booted' placeholder is ambiguous — address the sim by UDID"
fi

# 2. Global verbs over every device.
if printf '%s' "$command" | grep -qE 'simctl[^|;&]*[[:space:]](shutdown|erase|delete)[[:space:]]+all([[:space:]]|$)'; then
  deny "global '<verb> all' hits every simulator on the machine"
fi

# 3. Device addressed by quoted NAME (device args to common verbs that
#    contain a space = a human-readable name, not a UDID). UDIDs are
#    hex-and-dash tokens; anything quoted with spaces is a name.
if printf '%s' "$command" | grep -qE "simctl[[:space:]]+(boot|shutdown|erase|terminate|launch|openurl|install|uninstall|io|spawn|privacy)[[:space:]]+[\"'][^\"']*[[:space:]][^\"']*[\"']"; then
  deny "device addressed by name — names are unstable; resolve to a UDID via .smix/sims.json"
fi

exit 0
