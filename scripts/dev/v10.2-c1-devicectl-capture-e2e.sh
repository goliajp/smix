#!/usr/bin/env bash
# v10.2-C1 devicectl capture e2e: a screenshot and a recording, through
# the code, against a device.
#
# The unit tests check the argv and the JSON parser and may not reach a
# device. Whether `devicectl` accepts that argv, writes a PNG with pixels
# in it and a movie a player can open, is what only a device answers —
# and Xcode 27's devicectl treats a simulator as one, so the same script
# takes either:
#
#   36-character UDID  a simulator: booted through smix (which records
#                      that smix booted it) and shut down at the end
#   25-character UDID  a phone: must be connected. A phone devicectl
#                      knows but cannot reach is exit 2, "cannot judge" —
#                      not red, and never green.
#
# Usage:  bash scripts/dev/v10.2-c1-devicectl-capture-e2e.sh <UDID>
# Exit:   0 both files are what they claim · 1 a verdict failed · 2 cannot judge
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
. "$ROOT/scripts/lib/e2e-binary.sh"
# The device tier runs every *-e2e.sh with no arguments and names the
# device in SMIX_E2E_UDID; by hand, the argument wins.
UDID="${1:-${SMIX_E2E_UDID:-}}"
[ -n "$UDID" ] || { echo "usage: $0 <UDID>   (or SMIX_E2E_UDID=<UDID>)" >&2; exit 2; }
OUT="$(mktemp -d)"
WE_BOOTED=0
log()  { printf '[c1-capture] %s\n' "$*"; }
fail() { printf '[c1-capture] FAIL: %s\n' "$*" >&2; exit 1; }
cleanup() {
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$OUT"
}
trap cleanup EXIT
cd "$ROOT"
# shellcheck source=scripts/dev/lib/devicectl-e2e-device.sh
. "$ROOT/scripts/dev/lib/devicectl-e2e-device.sh"

case "${#UDID}" in
  36)
    # Shut down at the end only what this script booted. Inside the
    # device tier the simulator is somebody else's — the ship booted it
    # and the scripts after this one still need it.
    was="$(simulator_state "$UDID")"
    if [ "$was" = "Booted" ]; then
      log "simulator $UDID — already booted, and not ours to shut down"
    else
      log "simulator $UDID — booting through smix"
      "$SMIX" sim boot "$UDID" >/dev/null 2>&1 || true
      WE_BOOTED=1
    fi
    ;;
  25)
    state="$(phone_tunnel_state "$UDID")"
    if [ "$state" = "disconnected" ] || [ "$state" = "absent" ]; then
      echo "[c1-capture] device not connected ($UDID is $state) — cannot judge" >&2
      exit 2
    fi
    log "phone $UDID — tunnel $state"
    ;;
  *) echo "[c1-capture] $UDID is neither a simulator UDID (36) nor a device UDID (25)" >&2; exit 2 ;;
esac

# What the device itself says it can do decides which verdict applies to
# the recording. Not "phones cannot": a booted simulator lists the
# capability, a phone on iOS 26.6.2 does not, and a later phone may.
OFFERS_RECORDING="$(device_offers "$UDID" com.apple.coredevice.feature.screenrecording)"
[ "$OFFERS_RECORDING" != "unlisted" ] || { echo "[c1-capture] devicectl does not list $UDID — cannot judge" >&2; exit 2; }
log "the device says it offers screen recording: $OFFERS_RECORDING"

log "driving DevicectlClient: screenshot, then a 3 s recording"
rc=0
cargo run -q -p smix-sdk --example devicectl_capture -- "$UDID" "$OUT" > "$OUT/run.log" 2>&1 || rc=$?
sed 's/^/[c1-capture]   /' "$OUT/run.log"

read -r W H < <(sips -g pixelWidth -g pixelHeight "$OUT/shot.png" 2>/dev/null | awk '/pixelWidth/{w=$2} /pixelHeight/{h=$2} END{print w+0, h+0}')
file "$OUT/shot.png" | grep -q 'PNG image data' || fail "shot.png is not a PNG: $(file "$OUT/shot.png")"
[ "$W" -gt 0 ] && [ "$H" -gt 0 ] || fail "shot.png has no pixels (${W}x${H})"
log "screenshot: PNG ${W}x${H}, $(stat -f %z "$OUT/shot.png") bytes"

if [ "$OFFERS_RECORDING" = "no" ]; then
  # The right answer here is a refusal — at the start, by name. A
  # recording that "started" and was found empty three seconds later is
  # the failure this verdict exists for.
  [ "$rc" = 3 ] || fail "a device that does not offer recording should be refused at start_recording (exit 3), got exit $rc"
  grep -q 'com.apple.coredevice.feature.screenrecording' "$OUT/run.log" || fail "the refusal does not name the capability"
  [ ! -s "$OUT/rec.mp4" ] || fail "a refused recording left a file behind"
  log "recording: refused at the start, naming the capability the device does not offer"
  log "C1-CAPTURE-PASS on $UDID (screenshot driven; recording refused by name)"
  exit 0
fi
[ "$rc" = 0 ] || fail "the capture example failed (exit $rc)"
[ -s "$OUT/rec.mp4" ] || fail "rec.mp4 is empty or absent"
# Spotlight has not indexed a file this new, so ask the importer directly.
DUR="$(mdimport -t -d2 "$OUT/rec.mp4" 2>&1 | sed -n 's/.*kMDItemDurationSeconds = "\{0,1\}\([0-9.]*\).*/\1/p' | head -1)"
[ -n "$DUR" ] || fail "rec.mp4 has no readable duration — the trailer was not written, which is what a killed recording looks like"
python3 -c 'import sys; sys.exit(0 if float(sys.argv[1]) >= 2.0 else 1)' "$DUR" || fail "rec.mp4 is ${DUR}s, shorter than the 3 s it was given"
log "recording: ${DUR}s, $(stat -f %z "$OUT/rec.mp4") bytes"
log "C1-CAPTURE-PASS on $UDID"
