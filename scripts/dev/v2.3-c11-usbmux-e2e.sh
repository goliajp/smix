#!/usr/bin/env bash
# v2.3-C11 usbmux e2e: the tunnel crate against a real device.
#
# Runs the crate's own live tests and judges the outcome. It does not
# invent a CLI verb to drive them — there is no `smix device tunnel`
# today, and adding one just so a script has something to call would put
# a command in the product that exists for the test.
#
# The interesting property is what happens with no device attached: the
# tests print why they are skipping and pass. That is deliberate — a
# suite that goes green on a machine with nothing plugged in has told you
# almost nothing, so the skip has to be visible. This script surfaces the
# same distinction in its own exit line: PASS when a device was actually
# exercised, SKIP when there was none.
#
# Nothing here writes to a device: a listing, a tunnel to lockdownd that
# is closed without sending a request, and a connection to a closed port.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$(mktemp)"

log()  { printf '[c11-usbmux] %s\n' "$*"; }
step() { printf '[c11-usbmux] --- %s\n' "$*"; }
fail() { printf '[c11-usbmux] FAIL: %s\n' "$*" >&2; exit 1; }

cleanup() { rm -f "$OUT"; }
trap cleanup EXIT

cd "$ROOT"

step "0. Apple's daemon is where the crate expects it"
[ -S /var/run/usbmuxd ] \
  || fail "no usbmux socket at /var/run/usbmuxd — this machine cannot reach iOS devices over USB"
log "usbmuxd socket present"

step "1. wire format and plist subset (no device needed)"
cargo test -p smix-usbmux --lib > "$OUT" 2>&1 \
  || { tail -20 "$OUT"; fail "wire/plist unit tests failed"; }
grep -qE "test result: ok\. [0-9]+ passed" "$OUT" || fail "unit tests did not report a pass"
log "$(grep -oE 'test result: ok\. [0-9]+ passed' "$OUT" | head -1)"

step "2. live tests against whatever is attached"
cargo test -p smix-usbmux --test live -- --nocapture > "$OUT" 2>&1 \
  || { tail -20 "$OUT"; fail "live tests failed"; }

if grep -q "^SKIP " "$OUT"; then
  # Not a pass. Say what was missing, in the words the test used.
  log "no device was exercised:"
  grep "^SKIP " "$OUT" | sed 's/^/  /' | sort -u
  echo "C11-USBMUX-SKIP"
  exit 0
fi

grep -qE "device [0-9]+ serial .+ over (USB|Network)" "$OUT" \
  || fail "live tests passed but never reported a device — check what they actually did"
log "$(grep -oE 'device [0-9]+ serial .+ over (USB|Network)' "$OUT" | head -1)"

step "3. every live test ran, none quietly skipped"
for t in an_attached_device_reports_a_serial_and_a_transport \
         a_tunnel_to_lockdownd_opens \
         a_port_nobody_listens_on_is_refused_by_the_device_not_by_a_timeout; do
  grep -q "test $t \.\.\. ok" "$OUT" || fail "$t did not pass"
done
log "listing, tunnel, and closed-port refusal all exercised on a real device"

echo "C11-USBMUX-PASS"
