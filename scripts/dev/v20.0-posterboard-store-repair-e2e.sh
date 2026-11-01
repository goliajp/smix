#!/usr/bin/env bash
# v20.0: a simulator whose PosterBoard store PosterBoard cannot load is
# mended by the next boot through smix.
#
# PosterBoard can leave poster configurations with no descriptor identifier
# (a commit of its own fails and is rolled back incompletely); from then on
# it dies in -[PRPosterDescriptor _initWithPath:] at every start, restarted
# and dying again, each time with a crash report on this Mac's desktop. The
# fixture is the clock part of such a store, taken from sim-smix-02 on
# 2026-11-01 with this Mac's paths taken out; planted on a simulator and
# booted with plain `simctl boot`, it crashed PosterBoard three times in
# 25 s. Judged on a simulator that is off when the script starts:
#   1. the planted store has configurations with no descriptor identifier
#      (so what the boot below mends is this run's)
#   2. `smix sim boot` says it rebuilt the store, PosterBoard writes a new
#      one with every configuration whole, and no PosterBoard crash report
#      names this simulator for 60 s after the boot
#   3. `smix sim repair` on the running simulator finds nothing to do
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/e2e-devices.sh
source "$ROOT/scripts/lib/e2e-devices.sh"
# a simulator no other script uses, so planting a broken store takes
# nothing from anyone
DEVICE="${SMIX_PB_IOS:-4C36509F-4512-48E5-9398-4C6BFE1F84A5}"
FIXTURE="$ROOT/test-fixtures/posterboard-broken-store/PRBPosterExtensionDataStore"
REPORTS="$HOME/Library/Logs/DiagnosticReports"
QUIET_FOR=60
WORK="$(mktemp -d)"

log()  { printf '[v20.0-pb] %s\n' "$*" >&2; }
fail() { printf '[v20.0-pb] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v20.0-pb] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" WE_BOOTED=0
cleanup() {
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v xcrun >/dev/null 2>&1 || cannot_judge "no xcrun"
[ -d "$FIXTURE" ] || fail "no fixture at $FIXTURE"
UDID="$("$SMIX" sim resolve "$DEVICE" 2>/dev/null | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$DEVICE'"
state="$(simulator_state "$UDID")"
[ "$state" = Shutdown ] || cannot_judge "$UDID is $state; the store can only be planted on one that is off"
SUPPORT="$HOME/Library/Developer/CoreSimulator/Devices/$UDID/data/Library/Application Support"

# configurations under the store, and how many have no descriptor identifier
count_configs() {
  find "$SUPPORT/PRBPosterExtensionDataStore" -path '*/Extensions/*/configurations/*' \
    -maxdepth 5 -mindepth 5 -type d 2>/dev/null | wc -l | tr -d ' '
}
count_without_id() {
  local n=0 c
  while IFS= read -r c; do
    [ -f "$c/com.apple.posterkit.provider.descriptor.identifier" ] || n=$((n + 1))
  done < <(find "$SUPPORT/PRBPosterExtensionDataStore" -path '*/Extensions/*/configurations/*' \
    -maxdepth 5 -mindepth 5 -type d 2>/dev/null)
  echo "$n"
}
# PosterBoard crash reports for this simulator written at or after $1
# (`YYYY-MM-DD-HHMMSS`, the form report names carry)
crashes_since() {
  local n=0 f
  for f in "$REPORTS"/PosterBoard-*.ips "$REPORTS"/Retired/PosterBoard-*.ips; do
    [ -f "$f" ] || continue
    [[ "$(basename "$f")" < "PosterBoard-$1" ]] && continue
    grep -q "SimDevice.$UDID" "$f" && n=$((n + 1))
  done
  echo "$n"
}

log "1. plant the broken store"
rm -rf "$SUPPORT/PRBPosterExtensionDataStore" "$SUPPORT/com.apple.PosterKit"
cp -R "$FIXTURE" "$SUPPORT/"
planted="$(count_configs)" broken="$(count_without_id)"
[ "$planted" -gt 0 ] && [ "$broken" -eq "$planted" ] \
  || fail "the planted store has $broken of $planted configurations without a descriptor identifier"
log "  $broken of $planted configurations have no descriptor identifier"

log "2. smix sim boot"
since="$(date '+%Y-%m-%d-%H%M%S')"
WE_BOOTED=1
"$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
grep -q "could not load its store" "$WORK/boot.log" \
  || fail "the boot did not say it rebuilt the store: $(cat "$WORK/boot.log")"
deadline=$((SECONDS + QUIET_FOR))
while [ "$SECONDS" -lt "$deadline" ]; do
  n="$(crashes_since "$since")"
  [ "$n" -eq 0 ] || fail "PosterBoard crashed $n time(s) on $UDID after the boot"
  sleep 5
done
[ -d "$SUPPORT/PRBPosterExtensionDataStore" ] || fail "PosterBoard wrote no new store in ${QUIET_FOR}s"
[ "$(count_without_id)" -eq 0 ] || fail "the new store has configurations without a descriptor identifier"
log "  rebuilt; no PosterBoard crash in ${QUIET_FOR}s; $(count_configs) configurations, all whole"

log "3. smix sim repair on the running simulator"
"$SMIX" sim repair "$UDID" >"$WORK/repair.log" 2>&1 || fail "repair: $(tail -3 "$WORK/repair.log")"
grep -q "nothing to do" "$WORK/repair.log" || fail "repair on a whole store: $(cat "$WORK/repair.log")"

log "V20.0-POSTERBOARD-STORE-REPAIR-PASS"
