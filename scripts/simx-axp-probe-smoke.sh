#!/usr/bin/env bash
# simx-axp-probe-smoke.sh — v0.3 C2 host-side smoke for
# `AccessibilityPlatformTranslation.framework` probe-only capability gate.
#
#   1. swift build (incremental) of simx-host-hid.
#   2. `simx-host-hid axp-probe` — JSON wire schema must satisfy 7 jq gates
#      (ok / framework loaded+path / 5 classes / 3 selectors / sharedInstance).
#   3. `bun src/cli/index.ts doctor --json` — 6 checks total, axp ok.
#   4. Emit single-line JSON summary, exit 0.
#
# No simulator interaction — AXP framework load is host-side only and
# does not depend on a booted iOS sim.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/swift-bridge/.build/debug/simx-host-hid"
PROBE_OUT="${SIMX_AXP_PROBE_OUT:-/tmp/simx-axp.json}"
DOCTOR_OUT="${SIMX_AXP_DOCTOR_OUT:-/tmp/simx-doctor.json}"

EXPECTED_PATH="/System/Library/PrivateFrameworks/AccessibilityPlatformTranslation.framework/AccessibilityPlatformTranslation"

# Phase A — swift build (incremental, fast no-op if up-to-date).
if ! swift build --package-path "$ROOT/swift-bridge" --product simx-host-hid >&2; then
  echo "swift build simx-host-hid failed" >&2
  exit 1
fi
if [[ ! -x "$BIN" ]]; then
  echo "simx-host-hid binary not produced at $BIN" >&2
  exit 1
fi

# Phase B — axp-probe.
if ! "$BIN" axp-probe > "$PROBE_OUT" 2> /tmp/simx-axp-err.log; then
  echo "simx-host-hid axp-probe failed" >&2
  cat /tmp/simx-axp-err.log >&2 || true
  exit 1
fi

# Phase C — jq gates on probe wire JSON.
if ! jq -e '.ok == true' "$PROBE_OUT" > /dev/null; then
  echo "axp-probe: .ok != true" >&2; cat "$PROBE_OUT" >&2; exit 1
fi
if ! jq -e '.framework.loaded == true' "$PROBE_OUT" > /dev/null; then
  echo "axp-probe: .framework.loaded != true" >&2; cat "$PROBE_OUT" >&2; exit 1
fi
if ! jq --arg p "$EXPECTED_PATH" -e '.framework.path == $p' "$PROBE_OUT" > /dev/null; then
  echo "axp-probe: .framework.path != $EXPECTED_PATH" >&2; cat "$PROBE_OUT" >&2; exit 1
fi
AXP_CLASSES_N="$(jq -r '.classes.resolved | length' "$PROBE_OUT")"
if [[ "$AXP_CLASSES_N" != "5" ]]; then
  echo "axp-probe: .classes.resolved | length != 5 (got $AXP_CLASSES_N)" >&2
  cat "$PROBE_OUT" >&2; exit 1
fi
if ! jq -e '(.classes.missing | length) == 0' "$PROBE_OUT" > /dev/null; then
  echo "axp-probe: .classes.missing not empty" >&2; cat "$PROBE_OUT" >&2; exit 1
fi
AXP_SELECTORS_N="$(jq -r '.selectors.resolved | length' "$PROBE_OUT")"
if [[ "$AXP_SELECTORS_N" != "3" ]]; then
  echo "axp-probe: .selectors.resolved | length != 3 (got $AXP_SELECTORS_N)" >&2
  cat "$PROBE_OUT" >&2; exit 1
fi
if ! jq -e '.sharedInstance.available == true' "$PROBE_OUT" > /dev/null; then
  echo "axp-probe: .sharedInstance.available != true" >&2
  cat "$PROBE_OUT" >&2; exit 1
fi

# Phase D — doctor --json: 6 checks, axp ok.
if ! bun "$ROOT/src/cli/index.ts" doctor --json > "$DOCTOR_OUT" 2> /tmp/simx-doctor-err.log; then
  echo "simx doctor --json failed" >&2
  cat /tmp/simx-doctor-err.log >&2 || true
  cat "$DOCTOR_OUT" >&2 || true
  exit 1
fi
DOCTOR_CHECKS_N="$(jq -r '.checks | length' "$DOCTOR_OUT")"
if [[ "$DOCTOR_CHECKS_N" != "6" ]]; then
  echo "doctor: .checks | length != 6 (got $DOCTOR_CHECKS_N)" >&2
  cat "$DOCTOR_OUT" >&2; exit 1
fi
DOCTOR_AXP_OK="$(jq -r '[.checks[] | select(.name == "axp") | .ok] | first' "$DOCTOR_OUT")"
if [[ "$DOCTOR_AXP_OK" != "true" ]]; then
  echo "doctor: axp.ok != true (got $DOCTOR_AXP_OK)" >&2
  cat "$DOCTOR_OUT" >&2; exit 1
fi
DOCTOR_EXIT="$(jq -r '.exitCode' "$DOCTOR_OUT")"
if [[ "$DOCTOR_EXIT" != "0" ]]; then
  echo "doctor: .exitCode != 0 (got $DOCTOR_EXIT)" >&2
  cat "$DOCTOR_OUT" >&2; exit 1
fi

# Phase E — single-line JSON summary on stdout (9-field gate).
printf '{"axp_probe":"ok","axp_loaded":true,"axp_classes":%s,"axp_selectors":%s,"axp_shared_instance":true,"doctor_checks":%s,"doctor_axp_ok":true,"doctor_exit_code":%s}\n' \
  "$AXP_CLASSES_N" "$AXP_SELECTORS_N" "$DOCTOR_CHECKS_N" "$DOCTOR_EXIT"

exit 0
