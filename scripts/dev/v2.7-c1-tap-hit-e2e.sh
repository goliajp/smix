#!/usr/bin/env bash
# v2.7-C1 tap-hit e2e: a tap that opens a screen must not be reported as
# a miss.
#
# The hit verdict compares what the host aimed at against what the
# runner found under that coordinate, and where that snapshot is taken
# decides whether the check is worth anything. Taken after the touch, a
# tap that navigates has the destination under its own coordinate by the
# time the snapshot returns — so the successful taps are exactly the
# ones it calls misses. That shipped, and the release corpus caught it
# only because two of twenty flows happen to navigate.
#
# This pins the invariant by name: drive a navigating tap on a stock
# system app, and require both a clean exit and the absence of
# TAP_MISSED in the output. The verdict function's negative side (a
# moved target, an overlay swallowing the touch) is unit tested in
# smix-driver/tests/tap_hit_verdict.rs; what needs a device is the
# ordering, which no unit test can see.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# A port of this gate's own, so a bystander runner cannot turn it red.
. "$ROOT/scripts/lib/gate-port.sh"
FLOW="scripts/release/stress-corpus/nav-general-and-back.yaml"
BUNDLE="com.apple.Preferences"

log()  { printf '[c1-taphit] %s\n' "$*"; }
fail() { printf '[c1-taphit] FAIL: %s\n' "$*" >&2; exit 1; }

# A precondition this script detects and cannot satisfy is a SKIP with
# what to do about it — not a FAIL. Yielding to somebody else's batch, or
# an unset target, says nothing about whether smix works, and FAIL says it
# does not to whoever reads the suite next.
skip() { printf '[c1-taphit] %s\n' "$*" >&2; printf '%s\n' "C1-TAP-HIT-SKIP"; exit 0; }


# --- guards --------------------------------------------------------------

# §9#1: sims only, and always by explicit UDID — never a name, "booted"
# or "all", which is what the sim guard hook refuses. The picker holds
# the other half of that: a booted sim is not automatically ours, and
# this machine has a consumer's sim up most days.
UDID="${SMIX_TAPHIT_SIM:-}"
if [[ -z "$UDID" ]]; then
  UDID="$(bash "$ROOT/scripts/dev/pick-dev-sim.sh")" \
    || skip "set SMIX_TAPHIT_SIM to a UDID"
fi
[[ -n "$UDID" ]] || fail "no dev sim — set SMIX_TAPHIT_SIM to a UDID"

log "guard: no batch owner on this machine (yield, never seize)"
pgrep -f 'runner.ts|smix run|supervise' >/dev/null \
  && skip "batch owner active — yielding"

[[ -f "$ROOT/$FLOW" ]] || fail "flow missing: $FLOW"

SMIX_BIN="${SMIX_BIN:-$ROOT/target/release/smix}"
[[ -x "$SMIX_BIN" ]] || fail "smix binary missing: $SMIX_BIN (cargo build -p smix-cli --release)"

OUT="$(mktemp)"
cleanup() {
  log "teardown: runner down"
  "$SMIX_BIN" runner down >/dev/null 2>&1 || true
  rm -f "$OUT"
}
trap cleanup EXIT

# --- run -----------------------------------------------------------------

log "runner up on $UDID (bundle $BUNDLE)"
"$SMIX_BIN" runner up "$UDID" --bundle "$BUNDLE" >/dev/null 2>&1 \
  || fail "runner up failed on $UDID"

# No --retry: a navigating tap must pass on its first attempt. Retrying
# is what let the original defect read as an intermittent animation race
# for as long as it did.
log "run $FLOW (no retry)"
if "$SMIX_BIN" run "$ROOT/$FLOW" --device "$UDID" >"$OUT" 2>&1; then
  :
else
  rc=$?
  cat "$OUT" >&2
  fail "navigating tap flow exited $rc"
fi

if grep -q 'TAP_MISSED' "$OUT"; then
  cat "$OUT" >&2
  fail "tap that opened a screen was reported TAP_MISSED"
fi

log "PASS: navigating tap confirmed, no TAP_MISSED"
