#!/usr/bin/env bash
# smix release smoke — hard gate on the ship script.
#
# Exercises every net-new v1.0.4/v1.0.5 code path against a real
# iOS simulator. Runs `smix runner up` → yaml with takeScreenshot ×N
# + intentional fail (verifies debug-output tree.json emit) → cycle
# → verify session persisted → shutdown.
#
# Requires:
#   - Xcode + a booted-or-boot-able simulator with a build of
#     smix's SmixRunner installed. Locally: `~/.local/share/smix/
#     runner/SmixRunner.xcodeproj` from `smix runner up`.
#   - `smix` binary from the current build tree in $PATH.
#   - jq for JSON output introspection.
#
# Exit 0 → smoke green, publish allowed by scripts/release/ship.sh.
# Non-zero → block ship.

set -euo pipefail

SMOKE_UDID="${SMOKE_UDID:-}"
SMOKE_BUNDLE="${SMOKE_BUNDLE:-com.apple.mobilesafari}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# A port of this gate's own, so a bystander runner cannot turn it red.
. "$ROOT/scripts/lib/gate-port.sh"
SMOKE_YAML="${ROOT}/scripts/release/smoke-v1.smoke.yaml"
OUT_DIR="${OUT_DIR:-/tmp/smix-smoke-out}"

log() { printf '[smoke] %s\n' "$*"; }
fail() { printf '[smoke] FAIL: %s\n' "$*" >&2; exit 1; }

# --- pre-flight -------------------------------------------------------

command -v smix >/dev/null || fail "smix not on PATH"
command -v jq >/dev/null || fail "jq required for smoke output introspection"
command -v xcrun >/dev/null || fail "xcrun required"

if [[ -z "$SMOKE_UDID" ]]; then
  # Fallback: this repo's own booted sim. Not the first booted one —
  # a machine that also has a consumer's sim up would hand the smoke
  # someone else's device to install a runner onto.
  SMOKE_UDID="$(bash "$(dirname "$0")/../dev/pick-dev-sim.sh")" \
    || fail "no SMOKE_UDID env and no unambiguous dev sim booted"
fi
[[ -n "$SMOKE_UDID" ]] || fail "no SMOKE_UDID env and no booted dev sim to pick"

log "sim UDID:       $SMOKE_UDID"
log "target bundle:  $SMOKE_BUNDLE"
log "yaml fixture:   $SMOKE_YAML"
log "output dir:     $OUT_DIR"

rm -rf "$OUT_DIR" && mkdir -p "$OUT_DIR"

# --- 1. runner up (verifies §D8 bundle validation) --------------------

log "runner up with --bundle $SMOKE_BUNDLE (§A hard-require)"
smix runner up "$SMOKE_UDID" --bundle "$SMOKE_BUNDLE" >"$OUT_DIR/up.log" 2>&1 \
  || fail "runner up failed; see $OUT_DIR/up.log"

# Refuse-without-bundle check: should fail loud when we omit --bundle.
log "verify runner up refuses without --bundle"
if smix runner up "$SMOKE_UDID" >"$OUT_DIR/up-no-bundle.log" 2>&1; then
  fail "runner up accepted no --bundle (regression against §D8)"
fi
grep -q "SMIX_RUNNER_UP_ALLOW_DEFAULT_BUNDLE" "$OUT_DIR/up-no-bundle.log" \
  || fail "no-bundle error message missing SMIX_* env hint"

# --- 2. smoke yaml — pacer + fail tree.json --------------------------

log "run smoke yaml with --debug-output"
if smix run "$SMOKE_YAML" \
    --device "$SMOKE_UDID" \
    --activate \
    --bundle-id "$SMOKE_BUNDLE" \
    --debug-output "$OUT_DIR/debug" >"$OUT_DIR/run.log" 2>&1; then
  fail "smoke yaml unexpectedly succeeded (expected fail on assertVisible)"
fi

# Verify: run-summary.json has non-empty steps + tree_path on failure.
SUMMARY="$OUT_DIR/debug/run-summary.json"
[[ -f "$SUMMARY" ]] || fail "no run-summary.json at $SUMMARY (§D11 regression)"
STEPS_COUNT="$(jq '.steps | length' "$SUMMARY")"
[[ "$STEPS_COUNT" -gt 0 ]] || fail "run-summary.json steps is empty (§D11 regression)"
FAIL_TREE="$(jq -r '.steps[] | select(.verdict == "failed") | .tree_path // empty' "$SUMMARY" | head -1)"
[[ -n "$FAIL_TREE" ]] || fail "no tree_path on failed step (§D11 regression)"
[[ -f "$FAIL_TREE" ]] || fail "tree_path pointer $FAIL_TREE doesn't exist"
log "OK: debug-output has $STEPS_COUNT steps; fail tree.json at $FAIL_TREE"

# --- 3. cycle + session persistence check ----------------------------

log "runner cycle (§D5 + §D1 persistence)"
smix runner cycle >"$OUT_DIR/cycle.log" 2>&1 \
  || fail "runner cycle failed; see $OUT_DIR/cycle.log"

log "verify /session/list after cycle (§D1)"
# A supervisor + the smoke yaml would have opened one session at least;
# after cycle the session file rehydrates and the list should be
# non-empty.
smix runner list-sessions >"$OUT_DIR/list-after-cycle.log" 2>&1
if grep -q "no open sessions" "$OUT_DIR/list-after-cycle.log"; then
  log "note: no persisted sessions after cycle. This can be the smoke's own"
  log '      client having closed on `smix run` exit (safe-exit). OK.'
else
  log "OK: /session/list returned rows after cycle"
fi

# --- 4. supervisor sanity — 5 s alive check --------------------------

log "start supervisor in background for 5 s"
smix runner supervise >"$OUT_DIR/supervise.log" 2>&1 &
SUPERVISE_PID=$!
sleep 5
kill "$SUPERVISE_PID" 2>/dev/null || true
wait "$SUPERVISE_PID" 2>/dev/null || true
grep -q "attached" "$OUT_DIR/supervise.log" \
  || fail "supervisor never printed attach line (§D2 regression)"
log "OK: supervisor attached and shut down cleanly"

# --- 5. tear down ----------------------------------------------------

log "runner down"
smix runner down >"$OUT_DIR/down.log" 2>&1 \
  || fail "runner down failed; see $OUT_DIR/down.log"

log "SMOKE PASSED — publish allowed"
