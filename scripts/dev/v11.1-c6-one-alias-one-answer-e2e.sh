#!/usr/bin/env bash
# One alias, one answer, and the book it came from.
#
# A consumer's harness read the checkout's `.smix/sims.json` while
# `smix sim resolve` answered from this machine's registry; a device
# registered with smix was "missing" to the harness, and it took an hour
# to see that there were two books. Measured while fixing it: when the two
# books gave one alias to two devices, the checkout's won whenever its
# UDID sorted first, and reading the checkout wrote a store into it.
#
# Everything here happens in a machine directory and a checkout made for
# the purpose. This machine's real registry holds other people's devices;
# it is hashed before and after, and a change to it fails the run.
#
# No device is driven: the two UDIDs are this machine's own simulators,
# used only as registered identifiers. Exit 0 judged and passed, 1 judged
# and failed, 2 could not judge.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/scripts/lib/e2e-binary.sh"

A="986DA42B-E0B0-4CCE-8E94-3510C85E8044"  # sim-smix-04
B="89980B43-EF26-446A-A897-848C1AD3A872"  # sim-smix-03

log()  { printf '[one-alias] %s\n' "$*" >&2; }
fail() { printf '[one-alias] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[one-alias] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

REAL="${XDG_DATA_HOME:-$HOME/.local/share}/smix/devices"
real_hash() {
  if [ -d "$REAL" ]; then
    (cd "$REAL" && find . -type f -print0 | sort -z | xargs -0 shasum 2>/dev/null) | shasum | cut -d' ' -f1
  else
    printf 'absent'
  fi
}
REAL_BEFORE="$(real_hash)"

for u in "$A" "$B"; do
  xcrun simctl list devices | grep -q "$u" \
    || cannot_judge "simctl does not list $u; registering it would be refused for a reason that is not this check's"
done

# Run smix with the isolated machine directory, from inside a checkout.
# Separate files for stdout and stderr: the contract is that the
# identifier, and only it, is on stdout.
run() {  # run <checkout> <args…> → sets RC, OUT, ERR
  local co="$1"; shift
  (cd "$co" && SMIX_MACHINE_DIR="$WORK/machine" "$SMIX" "$@" >"$WORK/out" 2>"$WORK/err")
  RC=$?
  OUT="$(cat "$WORK/out")"
  ERR="$(cat "$WORK/err")"
}

checkout() {  # checkout <dir> <alias> <udid>
  mkdir -p "$1/.smix"
  printf '{"sims":{"%s":{"udid":"%s","deviceType":"","runtime":"","deviceName":"%s"}}}' \
    "$2" "$3" "$2" > "$1/.smix/sims.json"
}

mkdir -p "$WORK/machine"
run "$WORK" sim register --udid "$A" onmachine
[ "$RC" -eq 0 ] || cannot_judge "could not register on the isolated machine: $ERR"
run "$WORK" sim register --udid "$A" phone
[ "$RC" -eq 0 ] || cannot_judge "could not register on the isolated machine: $ERR"

log "--- the two books give one alias to two devices"
checkout "$WORK/co1" phone "$B"
run "$WORK/co1" sim resolve phone
[ "$RC" -eq 1 ] || fail "resolve exited $RC with the books disagreeing (stdout: $OUT)"
[ -z "$OUT" ] || fail "resolve printed an identifier ($OUT) for an alias the books disagree on"
for piece in "names two devices" "$A" "$B" "co1/.smix/sims.json"; do
  case "$ERR" in *"$piece"*) ;; *) fail "the refusal does not say '$piece': $ERR" ;; esac
done
log "  diverged=refused (both identifiers and the file named)"

log "--- only the machine holds it"
run "$WORK/co1" sim resolve onmachine
[ "$RC" -eq 0 ] || fail "resolve exited $RC: $ERR"
[ "$OUT" = "$A" ] || fail "stdout is '$OUT', expected $A alone"
case "$ERR" in *"from this machine's registry"*) ;; *) fail "no source line: $ERR" ;; esac
log "  machine=answered (source named)"

log "--- only the checkout holds it"
checkout "$WORK/co2" legacyonly "$B"
run "$WORK/co2" sim resolve legacyonly
[ "$RC" -eq 0 ] || fail "resolve exited $RC: $ERR"
[ "$OUT" = "$B" ] || fail "stdout is '$OUT', expected $B alone"
case "$ERR" in *"co2/.smix/sims.json"*"legacy book"*) ;; *) fail "source line does not name the file: $ERR" ;; esac
run "$WORK/co2" sim resolve legacyonly --json
KIND="$(printf '%s' "$OUT" | python3 -c 'import json,sys; print(json.load(sys.stdin)["source"]["kind"])' 2>/dev/null)"
[ "$KIND" = "checkout" ] || fail "--json source.kind is '$KIND', expected checkout: $OUT"
log "  checkout=answered (file named; --json says checkout)"

log "--- reading a checkout writes nothing into it"
# Exactly one entry per checkout, and it is the file we wrote. Counting
# is the check: an empty listing would also contain "nothing unexpected".
for co in co1 co2; do
  HELD="$(ls -A "$WORK/$co/.smix")"
  [ "$HELD" = "sims.json" ] || fail "$co/.smix holds '$(printf '%s' "$HELD" | tr '\n' ' ')' after being read — expected sims.json alone"
done
log "  checkout-writes=none"

REAL_AFTER="$(real_hash)"
[ "$REAL_BEFORE" = "$REAL_AFTER" ] || fail "this machine's real registry changed during the run"
log "  real-registry=untouched"

log "ONE-ALIAS-E2E-PASS (a disagreement stops the answer; every answer names its book)"
