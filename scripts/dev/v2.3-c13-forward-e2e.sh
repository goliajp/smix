#!/usr/bin/env bash
# v2.3-C13 forward e2e: the pipe, and the ledger row that outlives it.
#
# The forwarding logic itself is proven without hardware — the unit tests
# put a local echo server where the device would be and check both
# directions, per-connection tunnels, immediate close on refusal, and
# shutdown. What only a phone can answer is whether a real usbmux tunnel
# behaves the same behind that listener.
#
# No device → C13-FORWARD-SKIP, saying what was missing.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$(mktemp)"

log()  { printf '[c13-forward] %s\n' "$*"; }
step() { printf '[c13-forward] --- %s\n' "$*"; }
fail() { printf '[c13-forward] FAIL: %s\n' "$*" >&2; exit 1; }

cleanup() { rm -f "$OUT"; }
trap cleanup EXIT
cd "$ROOT"

step "0. forwarding logic, proven without a device"
cargo test -p smix-usbmux --lib forward > "$OUT" 2>&1 \
  || { tail -20 "$OUT"; fail "forwarder unit tests failed"; }
log "$(grep -oE 'test result: ok\. [0-9]+ passed' "$OUT" | head -1) (both directions, per-connection tunnels, refusal, shutdown)"

step "1. ledger ordering, proven without a device"
cargo test -p smix-lease forward_ordering > "$OUT" 2>&1 \
  || { tail -20 "$OUT"; fail "ledger ordering tests failed"; }
log "$(grep -oE 'test result: ok\. [0-9]+ passed' "$OUT" | head -1) (supervisor → runner → pipe)"

step "2. is there a device to tunnel to?"
UDID="$(cargo run -q -p smix-usbmux --example first_device 2>/dev/null || true)"
if [ -z "$UDID" ]; then
  log "no iOS device on usbmux — the tunnel half cannot be exercised"
  log "attach a device over USB and re-run to turn this into a PASS"
  echo "C13-FORWARD-SKIP"
  exit 0
fi
log "device $UDID"

step "3. a real tunnel behaves like the echo stand-in did"
# lockdownd is the target again: it is the one port every iOS device has,
# so this proves the pipe forms through the forwarder without depending
# on anything smix installed.
cargo run -q -p smix-usbmux --example forward_probe -- "$UDID" > "$OUT" 2>&1 \
  || { cat "$OUT"; fail "forwarding to lockdownd failed"; }
grep -q "forwarded" "$OUT" || { cat "$OUT"; fail "probe did not report a forward"; }
log "$(grep 'forwarded' "$OUT" | head -1)"

echo "C13-FORWARD-PASS"
