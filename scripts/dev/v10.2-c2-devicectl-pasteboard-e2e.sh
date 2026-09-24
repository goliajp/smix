#!/usr/bin/env bash
# v10.2-C2 devicectl pasteboard e2e: text written to a device's pasteboard
# through the code comes back the same bytes.
#
# The unit tests check the argv and the byte accounting and may not reach
# a device. Whether `devicectl` takes the text on stdin and hands the same
# bytes back is what only a device answers. Same two kinds as C1:
#
#   36-character UDID  a simulator: booted through smix, shut down at the
#                      end only if this script booted it
#   25-character UDID  a phone: must be connected; otherwise exit 2
#
# A pasteboard is the user's, and not only the device's: a simulator's
# general pasteboard is synced to this Mac's, and a phone's travels by
# Universal Clipboard to every device on the same Apple ID. The first run
# of this script seeded a simulator's general pasteboard and the seed was
# on the Mac and on the phone beside it a moment later, over whatever the
# user had copied. So the verdict is taken on a pasteboard of our own,
# `smix.e2e.probe`, which was measured not to travel, and the general one
# is not written at all unless SMIX_E2E_TOUCH_GENERAL_PASTEBOARD=1 says
# so — and then only when what is on it can be put back exactly: one item,
# text only. devicectl has no `clear` and writes one type at a time, so an
# image or rich text could not be restored. Nothing here prints what a
# pasteboard holds: byte counts, item counts and type names only.
#
# Usage:  bash scripts/dev/v10.2-c2-devicectl-pasteboard-e2e.sh <UDID>
# Exit:   0 equal (and restored, where touched) · 1 a verdict failed · 2 cannot judge
# Env:    SMIX_E2E_TOUCH_GENERAL_PASTEBOARD=1 also round-trips the general pasteboard
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
. "$ROOT/scripts/lib/e2e-binary.sh"
UDID="${1:-${SMIX_E2E_UDID:-}}"
[ -n "$UDID" ] || { echo "usage: $0 <UDID>   (or SMIX_E2E_UDID=<UDID>)" >&2; exit 2; }
OUT="$(mktemp -d)"
KEEP="$(mktemp -d)"
chmod 700 "$KEEP"
WE_BOOTED=0
log()  { printf '[c2-pasteboard] %s\n' "$*"; }
fail() { printf '[c2-pasteboard] FAIL: %s\n' "$*" >&2; exit 1; }
cleanup() {
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$OUT"
  # The keep dir holds the user's original pasteboard only while it is off
  # the device. If it is still there, putting it back did not finish.
  if [ -e "$KEEP/original" ]; then
    printf '[c2-pasteboard] the original pasteboard text was NOT put back; it is kept at %s\n' "$KEEP/original" >&2
  else
    rm -rf "$KEEP"
  fi
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
      echo "[c2-pasteboard] device not connected ($UDID is $state) — cannot judge" >&2
      exit 2
    fi
    log "phone $UDID — tunnel $state"
    ;;
  *) echo "[c2-pasteboard] $UDID is neither a simulator UDID (36) nor a device UDID (25)" >&2; exit 2 ;;
esac

OFFERS="$(device_offers "$UDID" com.apple.coredevice.feature.pasteboard)"
[ "$OFFERS" = "yes" ] || { echo "[c2-pasteboard] the device's capability list says pasteboard: $OFFERS — cannot judge" >&2; exit 2; }

# pasteboard_info <name|general> → "<changeCount> <itemCount> <text-only yes|no>"
# Types and counts. `info` does not carry content, and this prints none.
pasteboard_info() {
  local j="$OUT/info.$$.json" args=()
  [ "$1" = general ] || args=(--device-pasteboard "$1")
  xcrun devicectl device pasteboard info --device "$UDID" ${args[@]+"${args[@]}"} --json-output "$j" -q >/dev/null 2>&1 \
    || { echo "unreadable 0 no"; return; }
  python3 -c '
import json, sys
r = json.load(open(sys.argv[1]))["result"]
text = {"public.utf8-plain-text", "public.plain-text", "public.text"}
items = r.get("items", [])
flavors = {f["type"] for i in items for f in i.get("flavors", [])}
print(r.get("changeCount", "none"), len(items), "yes" if flavors and flavors <= text else "no")' "$j"
}

# Made per run, so an equal read-back cannot be last run's leftovers. A
# multi-byte character and an inner newline, no trailing one.
PROBE="$(printf 'smix-probe-%s-%s 剪\nsecond line' "$$" "$(python3 -c 'import time; print(time.time_ns())')")"

cargo build -q -p smix-sdk --example devicectl_pasteboard
BIN="$ROOT/target/debug/examples/devicectl_pasteboard"

read -r G_BEFORE G_ITEMS G_TEXT_ONLY < <(pasteboard_info general)
read -r N_BEFORE _ _ < <(pasteboard_info smix.e2e.probe)

log "named pasteboard smix.e2e.probe: write, read back"
"$BIN" "$UDID" named "$PROBE" > "$OUT/named.log" 2>&1 || { sed 's/^/[c2-pasteboard]   /' "$OUT/named.log"; fail "the named round trip failed"; }
grep -q 'probe_equal=true' "$OUT/named.log" || fail "the named round trip did not say equal"
log "  $(cat "$OUT/named.log")"

# Beside the verdict, not part of it: the pasteboard written to changed,
# and the one not written to did not.
read -r N_AFTER _ _ < <(pasteboard_info smix.e2e.probe)
read -r G_MID _ _ < <(pasteboard_info general)
[ "$N_AFTER" != "$N_BEFORE" ] || fail "smix.e2e.probe's changeCount is $N_AFTER before and after a write — nothing was written there"
[ "$G_MID" = "$G_BEFORE" ] || fail "the general pasteboard's changeCount moved ($G_BEFORE -> $G_MID) during a named round trip"
log "  named=equal   (probe changeCount $N_BEFORE -> $N_AFTER; general stayed at $G_MID)"

if [ "${SMIX_E2E_TOUCH_GENERAL_PASTEBOARD:-}" != 1 ]; then
  log "  general=left-alone(not asked to: it is the user's, here and on every device it syncs to)"
elif [ "$G_ITEMS" = 1 ] && [ "$G_TEXT_ONLY" = yes ]; then
  log "general pasteboard: 1 text item — save, write, read back, put back"
  log "  (the probe travels to this Mac or this Apple ID's other devices for a few seconds; putting it back travels the same way)"
  "$BIN" "$UDID" general "$PROBE" "$KEEP" > "$OUT/general.log" 2>&1 || { sed 's/^/[c2-pasteboard]   /' "$OUT/general.log"; fail "the general round trip failed"; }
  log "  $(cat "$OUT/general.log")"
  grep -q 'probe_equal=true restored_equal=true' "$OUT/general.log" || fail "the general round trip did not say equal and restored"
  [ ! -e "$KEEP/original" ] || fail "the kept original is still on disk after a restore that said equal"
  log "  general=equal-and-restored"
else
  log "  general=left-alone($G_ITEMS item(s), text-only=$G_TEXT_ONLY — it could not be put back exactly)"
fi

log "C2-PASTEBOARD-E2E-PASS on $UDID"
