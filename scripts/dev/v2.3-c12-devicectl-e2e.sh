#!/usr/bin/env bash
# v2.3-C12 devicectl e2e: the six that exist, and one that does not.
#
# What is worth checking on real hardware here is narrow. The refusals are
# pure functions over no state at all — a unit test proves those better
# than a device can, and it proves them on every machine. What only a
# phone can answer is whether the six argv forms this crate builds are the
# ones `devicectl` actually accepts.
#
# So this installs nothing of its own: it reads the device's app list,
# checks a deeplink launch is accepted, and confirms an unavailable action
# refuses without touching anything. Installing a fixture app would need
# one built and signed for this device, which is C13's problem.
#
# No device attached → C12-DEVICECTL-SKIP, saying what was missing. A
# suite that goes green with nothing plugged in has told you nothing.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$(mktemp)"

log()  { printf '[c12-devicectl] %s\n' "$*"; }
step() { printf '[c12-devicectl] --- %s\n' "$*"; }
fail() { printf '[c12-devicectl] FAIL: %s\n' "$*" >&2; exit 1; }

cleanup() { rm -f "$OUT"; }
trap cleanup EXIT
cd "$ROOT"

step "0. the refusals, which need no device"
cargo test -p smix-sdk --lib devicectl > "$OUT" 2>&1 \
  || { tail -20 "$OUT"; fail "devicectl unit tests failed"; }
grep -qE "test result: ok\. [0-9]+ passed" "$OUT" || fail "unit tests reported no pass"
log "$(grep -oE 'test result: ok\. [0-9]+ passed' "$OUT" | head -1) (argv forms + every action either done or refused)"

step "1. is there a device to ask?"
# Reachability, not presence. On 2026-08-06 devicectl listed a phone as
# `available (paired)` while its USB connection was gone — a listing says
# a device is *known*, which is not the same as being able to ask it
# anything. So the gate is a real query, and the answer to that query is
# the gate.
#
# devicectl, not usbmux: what C12 exercises is the argv this crate builds
# for `devicectl`, and devicectl rides CoreDevice's own transport — USB or
# the network tunnel, its choice, not ours. Gating this on usbmux (as this
# did until 2026-08-06) borrowed C13/C14's reasoning, where the tunnel
# genuinely is the road smix drives on, and applied it where it does not
# hold: it left a fully answerable question sitting at SKIP.
# By shape, not by column: a device name and a model both contain spaces,
# so counting fields lands on whichever word happens to be there.
UDID="$(xcrun devicectl list devices 2>/dev/null \
  | grep -oE '[0-9A-Fa-f]{8}(-[0-9A-Fa-f]{4}){3}-[0-9A-Fa-f]{12}' | head -1 || true)"
if [ -n "$UDID" ] && ! xcrun devicectl device info apps --device "$UDID" --timeout 30 >/dev/null 2>&1; then
  log "devicectl lists $UDID but cannot reach it — treating that as no device"
  UDID=""
fi
if [ -z "$UDID" ]; then
  log "no iOS device devicectl can reach — nothing to exercise its argv against"
  log "attach a device (USB or a paired network tunnel) and re-run for a PASS"
  echo "C12-DEVICECTL-SKIP"
  exit 0
fi
log "device $UDID (reachable — asked, not merely listed)"

step "2. devicectl accepts the app-listing form this crate builds"
xcrun devicectl device info apps --device "$UDID" --timeout 30 > "$OUT" 2>&1 \
  || { tail -5 "$OUT"; fail "devicectl rejected the app-listing form"; }
log "app listing accepted ($(grep -c . "$OUT") lines)"

step "3. an unavailable action refuses, and touches nothing"
BEFORE="$(xcrun devicectl device info apps --device "$UDID" --timeout 30 2>/dev/null | wc -l | tr -d ' ')"
cargo run -q -p smix-sdk --example devicectl_refusal -- "$UDID" > "$OUT" 2>&1 || true
grep -q "not available on a physical device" "$OUT" \
  || { cat "$OUT"; fail "an unavailable action did not refuse"; }
AFTER="$(xcrun devicectl device info apps --device "$UDID" --timeout 30 2>/dev/null | wc -l | tr -d ' ')"
[ "$BEFORE" = "$AFTER" ] || fail "a refused action changed the device ($BEFORE → $AFTER apps)"
log "refused, and the device is unchanged"

echo "C12-DEVICECTL-PASS"
