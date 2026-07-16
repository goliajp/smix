#!/usr/bin/env bash
# The AI-assertion tier must stay out of the sense path.
#
# smix resolves every selector through the accessibility tree and Vision OCR.
# Those are deterministic. The AI tier is a judgement, and it sits beside the
# resolver rather than inside it — an authoring and CI aid, opt-in, one
# provider, marked non-deterministic.
#
# A comment claiming that would rot. This asserts it: no crate on the sense
# path may reach smix-ai-tier, directly or transitively. If that ever becomes
# false, the fence is gone and "vision is a dev tool, not core sense" stops
# being true no matter what the docs say.
#
# Usage: scripts/dev/fence-check.sh
set -euo pipefail

cd "$(dirname "$0")/../.."

# Crates that sense, decide how to resolve, or drive the device.
SENSE_PATH=(
  smix-selector
  smix-selector-resolver
  smix-host-coord-resolver
  smix-screen
  smix-driver
  smix-error
  smix-input
  smix-runner-wire
  smix-runner-client
)

breached=0
for crate in "${SENSE_PATH[@]}"; do
  if cargo tree -p "$crate" -e normal 2>/dev/null | grep -q 'smix-ai-tier'; then
    echo "FENCE BREACH: $crate reaches smix-ai-tier"
    cargo tree -p "$crate" -e normal -i smix-ai-tier 2>/dev/null | head -20
    breached=1
  fi
done

if [ "$breached" -ne 0 ]; then
  echo
  echo "The AI tier has leaked into the sense path. Either the dependency is a"
  echo "mistake, or the fence is being abandoned — and abandoning it is a §9"
  echo "decision, not a refactor."
  exit 1
fi

echo "fence-check: clean — the sense path does not reach smix-ai-tier"
