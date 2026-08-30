#!/usr/bin/env bash
# TS-driving device e2e: the TS SDK drives a real iOS Simulator through the real
# napi addon and a real runner. Mirrors smoke-v1.smoke.sh's discipline.
#
#   SMIX_E2E_UDID=<explicit-udid> bash scripts/release/ts-driving-e2e.sh
#
# The UDID is REQUIRED and explicit — never a booted/all placeholder (sim-guard).
# Refuses to run if another runner owns the batch. Sweeps its own sim on exit.
set -euo pipefail

BUNDLE=com.apple.Preferences
# The literal fallback here was 22087 -- the very default this gate
# exists to avoid. Ask the OS instead; SMIX_RUNNER_PORT reaches
# startup, every flow and teardown alike through clap's env.
# shellcheck source=/dev/null
. "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

log()  { printf '[ts-e2e] %s\n' "$*"; }
fail() { printf '[ts-e2e] FAIL: %s\n' "$*" >&2; exit 1; }

command -v smix  >/dev/null || fail "smix not on PATH"
command -v xcrun >/dev/null || fail "xcrun required"
command -v node  >/dev/null || fail "node required"

# sim-guard: explicit UDID only.
[ -n "${SMIX_E2E_UDID:-}" ] || fail "SMIX_E2E_UDID is required (an explicit UDID; never booted/all)"
case "$SMIX_E2E_UDID" in
  booted|all|"") fail "SMIX_E2E_UDID must be an explicit UDID, got '$SMIX_E2E_UDID'" ;;
esac
xcrun simctl list devices 2>/dev/null | grep -q "$SMIX_E2E_UDID" \
  || fail "UDID $SMIX_E2E_UDID not found in simctl device list"

# batch-owner: do not stomp a runner someone else brought up.
if pgrep -fl 'runner\.ts|smix run|supervise' | grep -viE 'pgrep|grep' >/dev/null 2>&1; then
  fail "another runner/supervise owns the batch — refusing to interfere"
fi

cleanup() {
  log "teardown: runner down + simx-sweep (own sim only)"
  # Not silenced. A teardown that fails leaves a runner on this device,
  # and an Android device has only one -- so the next thing to start one
  # there gets two instrumentations, or two xcodebuild sessions on one
  # sim, and every failure after that is about the wrong thing. What it
  # cost when it was silent, measured 2026-08-29: 23 of 26 corpus flows.
  if ! down_said="$(smix runner down 2>&1)"; then
    printf 'warning: the runner was not stopped:\n%s\n' "$(printf '%s' "$down_said" | tail -3)" >&2
  fi
  bash "$ROOT/scripts/dev/simx-sweep.sh" >/dev/null 2>&1 || true
}
trap cleanup EXIT

log "runner up $SMIX_E2E_UDID --bundle $BUNDLE (port $PORT)"
smix runner up "$SMIX_E2E_UDID" --bundle "$BUNDLE" >/tmp/ts-e2e-up.log 2>&1 \
  || fail "runner up failed; see /tmp/ts-e2e-up.log"

log "drive Preferences from the TS SDK through the real napi addon"
SMIX_RUNNER_PORT="$PORT" node "$ROOT/npm/smix-rn/e2e/drive-preferences.mjs" \
  || fail "TS driving e2e did not pass"

log "C5-DEVICE-PASS"
