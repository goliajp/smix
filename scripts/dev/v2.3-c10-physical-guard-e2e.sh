#!/usr/bin/env bash
# v2.3-C10 physical-device guard e2e: the two hard constraints of the
# amended §9#1, checked without touching any phone.
#
#   1. a physical device must be registered before it can be addressed
#   2. destructive actions on one are refused until opted in, once
#
# Everything here runs against a throwaway workspace with hand-written
# registry entries. That is the point: the rules are pure functions over
# the registry, so proving them needs no device — and a guard that could
# only be tested by risking a real phone would never be tested.
#
# Ordering note: this lands BEFORE the physical-device capability (C11).
# During the R1-R5 research the capability was exercised by hand with no
# guard in place, which is recorded in the decision log as exactly that —
# research, not the product path.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
WORK="$(mktemp -d)"

log()  { printf '[c10-guard] %s\n' "$*"; }
step() { printf '[c10-guard] --- %s\n' "$*"; }
fail() { printf '[c10-guard] FAIL: %s\n' "$*" >&2; exit 1; }

cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX"
cd "$WORK"
mkdir -p .smix

# A phone and a simulator, registered through the real commands.
#
# An earlier draft used to hand-write the registry file directly. A gate
# in smix-store caught it: that file is retired — the registry lives in
# the store now, and the file used to be the source of truth but is only
# a legacy import today. The test was exercising a path no user walks,
# and it took the gate to say so.
#
# Registering the phone through the real command is what turned up the
# actual gap: there was no way to register a physical device at all.
# `sim register` looked every device up in simctl, which lists no phones.
"$SMIX" sim register phone --udid 00008120-001410C11A42201E \
  --kind physical-ios --name panda-phone >/dev/null 2>&1 \
  || fail "could not register a physical device"

step "1. an unregistered physical serial cannot be addressed"
set +e
OUT="$("$SMIX" sim resolve R5CT52DF07D 2>&1)"
RC=$?
set -e
[ "$RC" -ne 0 ] || fail "an unregistered device resolved: $OUT"
echo "$OUT" | grep -qi "unknown device\|register" \
  || fail "refusal does not point at registration: $OUT"
log "unregistered device refused, pointed at the registry"

step "2. a registered phone is addressable — registration is the gate, not a ban"
OUT="$("$SMIX" sim resolve phone 2>&1 | tail -1)"
echo "$OUT" | grep -q "00008120-001410C11A42201E" || fail "registered phone did not resolve: $OUT"
log "registered phone resolves"

step "3. destructive on that phone is refused, by name and with the way out"
set +e
OUT="$("$SMIX" sim keychain-reset phone 2>&1)"
RC=$?
set -e
[ "$RC" -ne 0 ] || fail "keychain-reset ran on a phone with no opt-in"
echo "$OUT" | grep -q "phone" || fail "refusal does not name the device: $OUT"
echo "$OUT" | grep -q "smix sim allow-destructive phone" \
  || fail "refusal does not give the command that lifts it: $OUT"
echo "$OUT" | grep -qi "not undoable" \
  || fail "refusal does not say why: $OUT"
log "refused, named, and told how"

step "4. the two registration paths differ in exactly the right way"
# An earlier draft registered a real simulator here and asserted the guard
# stayed quiet. That was a weak judge: if the registration failed, the
# later command failed with "unknown device" — whose message also lacks
# the word `allow-destructive`, so the assertion passed while testing
# nothing. Replaced by a pair that needs no device on this machine and
# says more.
FAKE="11111111-2222-3333-4444-555555555555"
set +e
OUT="$("$SMIX" sim register ghost --udid "$FAKE" 2>&1)"
RC=$?
set -e
[ "$RC" -ne 0 ] || fail "a simulator that simctl never listed was registered anyway"
echo "$OUT" | grep -qi "simctl knows no device" \
  || fail "the simulator path stopped checking simctl: $OUT"
log "simulator path still verified against simctl"

"$SMIX" sim register ghost-phone --udid "$FAKE" --kind physical-android >/dev/null 2>&1 \
  || fail "a physical device could not be registered by its identifier"
log "physical path takes the identifier as given — there is no catalogue of phones"

# And the guard follows the kind, not the name.
set +e
OUT="$("$SMIX" sim keychain-reset ghost-phone 2>&1)"
RC=$?
set -e
[ "$RC" -ne 0 ] || fail "destructive ran on a freshly registered phone"
echo "$OUT" | grep -q "allow-destructive ghost-phone" \
  || fail "guard did not fire on the physical registration: $OUT"
log "guard follows the recorded kind"

step "5. opt-in is recorded once, and is idempotent"
OUT="$("$SMIX" sim allow-destructive phone 2>&1 | tail -1)"
echo "$OUT" | grep -q "allowed" || fail "opt-in did not report success: $OUT"
OUT="$("$SMIX" sim allow-destructive phone 2>&1 | tail -1)"
echo "$OUT" | grep -qi "already" \
  || fail "second opt-in did not report it was already allowed: $OUT"
log "recorded once, second time says so"

step "6. after opt-in the action is no longer refused by the guard"
set +e
OUT="$("$SMIX" sim keychain-reset phone 2>&1)"
set -e
echo "$OUT" | grep -q "allow-destructive" \
  && fail "still gated after opt-in: $OUT"
log "gate lifted"

echo "C10-PHYSICAL-GUARD-PASS"
