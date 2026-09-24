#!/usr/bin/env bash
# v10.2-C3 devicectl location e2e: a coordinate and a route, through the
# code, against a device — and no simulated location left on it after.
#
# The unit tests hold the argv, the route file and the reading of
# devicectl's reply, and may not reach a device. Whether `devicectl` takes
# a negative coordinate in the `=` form, reads the route file, and returns
# from `route` at once is what only a device answers. Same two kinds as
# C1 and C2:
#
#   36-character UDID  a simulator: booted through smix, shut down at the
#                      end only if this script booted it
#   25-character UDID  a phone: must be connected; otherwise exit 2
#
# A simulated location is the user's day: on a phone it moves every map
# and every weather app until it is cleared. So the clear is in the trap,
# installed before anything is written, and it calls `xcrun` itself — it
# has to work when the code under test does not.
#
# What this cannot show: devicectl has no verb that reads a device's
# current simulated location, and `clear` answers `cleared: true` with
# nothing to clear. "It was set" and "it is gone" are both devicectl's
# word; the last line says so.
#
# Usage:  bash scripts/dev/v10.2-c3-devicectl-location-e2e.sh <UDID>
# Exit:   0 every verdict agreed and the clear was accepted · 1 a verdict failed · 2 cannot judge
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
. "$ROOT/scripts/lib/e2e-binary.sh"
UDID="${1:-${SMIX_E2E_UDID:-}}"
[ -n "$UDID" ] || { echo "usage: $0 <UDID>   (or SMIX_E2E_UDID=<UDID>)" >&2; exit 2; }
OUT="$(mktemp -d)"
WE_BOOTED=0
WROTE=0
log()  { printf '[c3-location] %s\n' "$*"; }
fail() { printf '[c3-location] FAIL: %s\n' "$*" >&2; exit 1; }

# clear_location → 0 when devicectl exits 0 and says `cleared: true`
clear_location() {
  local j="$OUT/clear.$RANDOM.json"
  xcrun devicectl device simulate location clear --device "$UDID" --json-output "$j" -q >/dev/null 2>&1 || return 1
  python3 -c 'import json,sys; sys.exit(0 if json.load(open(sys.argv[1]))["result"].get("cleared") is True else 1)' "$j"
}
cleanup() {
  local rc=$?
  if [ "$WROTE" = 1 ] && ! clear_location; then
    printf '[c3-location] the simulated location MAY STILL BE ON THE DEVICE. Run:\n[c3-location]   xcrun devicectl device simulate location clear --device %s\n' "$UDID" >&2
    rc=1
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$OUT"
  exit "$rc"
}
trap cleanup EXIT
cd "$ROOT"
# shellcheck source=scripts/dev/lib/devicectl-e2e-device.sh
. "$ROOT/scripts/dev/lib/devicectl-e2e-device.sh"

case "${#UDID}" in
  36)
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
      echo "[c3-location] device not connected ($UDID is $state) — cannot judge" >&2
      exit 2
    fi
    log "phone $UDID — tunnel $state"
    log "for about a minute this phone's maps and weather will think it is somewhere else"
    ;;
  *) echo "[c3-location] $UDID is neither a simulator UDID (36) nor a device UDID (25)" >&2; exit 2 ;;
esac

OFFERS="$(device_offers "$UDID" com.apple.coredevice.feature.simulatelocation)"
[ "$OFFERS" = "yes" ] || { echo "[c3-location] the device's capability list says simulated location: $OFFERS — cannot judge" >&2; exit 2; }

cargo build -q -p smix-sdk --example devicectl_location
BIN="$ROOT/target/debug/examples/devicectl_location"

# From here the device may hold a location that is not its own.
WROTE=1

"$BIN" "$UDID" set 35.6812 139.7671 > "$OUT/set.log" 2>&1 || { sed 's/^/[c3-location]   /' "$OUT/set.log"; fail "location_set (35.6812, 139.7671) did not agree"; }
log "  set=agreed            (35.6812, 139.7671)"

# Both negative: whether `--latitude=-33.8688` gets through is the device's to say.
"$BIN" "$UDID" set -33.8688 -70.6693 > "$OUT/neg.log" 2>&1 || { sed 's/^/[c3-location]   /' "$OUT/neg.log"; fail "location_set (-33.8688, -70.6693) did not agree"; }
log "  set-negative=agreed   (-33.8688, -70.6693)"

T0="$(python3 -c 'import time; print(time.time())')"
"$BIN" "$UDID" route 5 -33.8688,151.2093 -33.8700,151.2110 > "$OUT/route.log" 2>&1 || { sed 's/^/[c3-location]   /' "$OUT/route.log"; fail "location_start over two waypoints did not agree"; }
TOOK="$(python3 -c 'import sys,time; print(f"{time.time()-float(sys.argv[1]):.1f}")' "$T0")"
# Beside the verdict: `travel` means "set it going and return". A route
# that started blocking would still agree, and would change what the verb is.
python3 -c 'import sys; sys.exit(0 if float(sys.argv[1]) < 10 else 1)' "$TOOK" || fail "location_start took ${TOOK}s — it is meant to return while the device travels"
log "  route=agreed          (2 waypoints at 5 m/s, returned in ${TOOK}s)"

clear_location || fail "devicectl did not accept the clear"
log "  cleared=true"
log "C3-LOCATION-E2E-PASS on $UDID (clear accepted; the device's actual location cannot be read back from the host)"
