#!/usr/bin/env bash
# v2.3-C15 addressability e2e: §9#1's first constraint, as a branch.
#
#   "a physical device must be registered before it can be addressed —
#    whichever one happens to be plugged in is never a target"
#
# Until 2026-08-06 that sentence had no code behind it. Two measurements
# that day:
#
#   * `smix sim erase <an unregistered 36-char UUID>` was not stopped by
#     the guard. It reached simctl, which said "Invalid device". The
#     device was safe because of what the executor happened to recognise,
#     not because anything refused — and C12 had just added a devicectl
#     path that recognises exactly that shape of UUID, with an Uninstall
#     verb on it.
#   * `crates/smix-capsule/src/runner_android.rs` never consulted the
#     registry at all. Any attached adb serial was addressable.
#
# Most of this runs against a throwaway workspace, because the rules are
# pure functions and proving them needs no hardware. One part is not:
# "an attached but unregistered device stays untouchable" cannot be
# proven with a fabricated serial, where everything fails for trivial
# reasons anyway. That half needs a real device on the bus, and it is the
# half that matters — it is the shape of the 2026-07-17 incident, when
# smix's runner landed on somebody's personal handset.
#
# Real hardware is reached only when a person names a phone
# (SMIX_E2E_PHYSICAL_ANDROID), and every call made against it is one this
# script has first proven must be REFUSED — the phone is unregistered in
# this script's own ledger. "Expects to be refused" was not enough: on
# 2026-09-25 the premise was false and the call ran on the owner's phone.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# A port of this gate's own, so a bystander runner cannot turn it red.
. "$ROOT/scripts/lib/gate-port.sh"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/e2e-devices.sh
source "$ROOT/scripts/lib/e2e-devices.sh"
WORK="$(mktemp -d)"
# The registrations below (`ghost`, and in step 6 the named phone) are
# this script's own. Until 2026-09-25 `ghost` was written into the real
# ledger.
e2e_isolate_machine "$WORK"
OUT="$(mktemp)"

log()  { printf '[c15-address] %s\n' "$*"; }
step() { printf '[c15-address] --- %s\n' "$*"; }
fail() { printf '[c15-address] FAIL: %s\n' "$*" >&2; exit 1; }

cleanup() { rm -rf "$WORK" "$OUT"; }
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX"

# Quieten the embedded store's replay chatter so greps read the command's
# own output rather than the KV log.
smix() { "$SMIX" "$@" 2>&1 || true; }

step "0. the judgement itself, which needs nothing"
cargo test -p smix-lease --lib may_address > "$OUT" 2>&1 \
  || { tail -20 "$OUT"; fail "may_address unit tests failed"; }
cargo test -p smix-lease --lib addressab >> "$OUT" 2>&1 || true
grep -qE "test result: ok\." "$OUT" || fail "unit tests reported no pass"
log "pure-function tests pass"

cd "$WORK"
mkdir -p .smix

step "1. a UUID nobody registered is refused before anything runs"
smix sim erase 99999999-8888-7777-6666-555555555555 > "$OUT"
grep -q "is not a device smix may address" "$OUT" || { cat "$OUT"; fail "not refused"; }
grep -q "smix sim register" "$OUT" || { cat "$OUT"; fail "refusal names no way forward"; }
# It must not have reached simctl. If it had, the message would be
# simctl's — and being saved by the executor is what this checkpoint
# exists to stop relying on.
grep -q "Invalid device" "$OUT" && fail "reached simctl — the guard did not fire"
log "refused at resolution, not by the executor"

step "2. a simulator the platform lists stays addressable unregistered"
# `smix sim boot <a udid nobody registered>` is an ordinary thing to do.
# Guarding it would buy no safety and break the common case.
SIM_UDID="$(xcrun simctl list devices -j 2>/dev/null \
  | python3 -c "
import json,sys
try: d=json.load(sys.stdin)
except Exception: sys.exit()
for rt,ds in d.get('devices',{}).items():
    for x in ds:
        if x.get('isAvailable') and x.get('state')!='Booted':
            print(x['udid']); sys.exit()
" || true)"
if [ -n "$SIM_UDID" ]; then
  smix sim screenshot "$SIM_UDID" /dev/null > "$OUT"
  grep -q "is not a device smix may address" "$OUT" \
    && { cat "$OUT"; fail "an unregistered real simulator was refused — the common case broke"; }
  log "unregistered simulator $SIM_UDID resolved (its own failure downstream is fine)"
else
  log "no shutdown simulator available to check the allow-path with"
fi

step "3. registering a device lifts the refusal"
smix sim register ghost --udid 99999999-8888-7777-6666-555555555555 --kind physical-ios > "$OUT"
grep -q "registered:" "$OUT" || { cat "$OUT"; fail "registration failed"; }
smix sim erase ghost > "$OUT"
grep -q "is not a device smix may address" "$OUT" \
  && { cat "$OUT"; fail "still unaddressable after registration"; }
# Addressable, and now the *other* gate speaks — the two are different
# questions and both must be asked.
grep -q "destructive actions are not allowed" "$OUT" \
  || { cat "$OUT"; fail "addressable but the destructive gate went quiet"; }
log "addressable, and destructive still refused — two gates, not one"

step "4. an emulator serial needs no registration"
# A serial with an emulator's shape and nothing behind it. It used to be
# emulator-5554, and a serial is a port: whichever AVD booted into it
# first — on 2026-09-24 a consumer's — would have had smix's runner
# uninstalled.
smix runner uninstall --platform android --device emulator-5998 > "$OUT"
grep -q "is not a device smix may address" "$OUT" \
  && { cat "$OUT"; fail "an emulator serial was refused"; }
log "an emulator serial is addressable without registration"

step "5. an attached, unregistered physical device stays untouchable"
# The half no fabricated serial can prove, and the half that ran on the
# owner's Samsung on 2026-09-25: the device had been registered (by a
# sibling of this script, into the real ledger), so the "refused" call
# was not refused and smix's runner was uninstalled from the phone.
#
# Two things changed. A phone is used only when a person names one —
# being attached is not permission. And the premise ("it is not
# registered") is proven before anything is sent, not read off the
# result afterwards: this script's ledger is its own, so the named phone
# is unregistered by construction, and the resolve below confirms it
# without reaching the device.
if [ -z "${SMIX_E2E_PHYSICAL_ANDROID:-}" ]; then
  log "no physical Android device named (SMIX_E2E_PHYSICAL_ANDROID) — steps 1-4 drove;"
  log "the half that needs a phone is not run, because attached is not permission"
  echo "C15-ADDRESSABILITY-PASS-WITHOUT-PHONE"
  exit 0
fi
e2e_physical_or_skip android c15-address
SERIAL="$E2E_PHYSICAL"
if "$SMIX" sim resolve "$SERIAL" >/dev/null 2>&1; then
  fail "premise does not hold: $SERIAL resolves in this script's own ledger ($SMIX_MACHINE_DIR), so a call expected to be refused would run — nothing was sent"
fi
log "attached physical device: $SERIAL (only refused calls are made against it)"

# `runner up` is deliberately NOT aimed at the real device, even though
# it is the path that matters most. A test that asserts "this is refused"
# does damage the day the refusal regresses, and what `up` does when it
# is not refused is install an APK on somebody's phone — the incident
# this checkpoint exists to prevent, performed by its own regression
# test. So the two verbs that would at worst disturb smix's own runner
# are aimed at the attached device, and the one that would install is
# aimed at a serial that reaches nothing.
for verb in "runner uninstall --platform android --device $SERIAL" \
            "runner down --platform android --device $SERIAL"; do
  # shellcheck disable=SC2086
  smix $verb > "$OUT"
  grep -q "is not a device smix may address" "$OUT" \
    || { cat "$OUT"; fail "'smix $verb' did not refuse an unregistered attached device"; }
  grep -q "$SERIAL" "$OUT" || { cat "$OUT"; fail "the refusal does not name the device"; }
  log "refused: smix $verb"
done

smix runner up --platform android NOSUCHSERIAL0001 > "$OUT"
grep -q "is not a device smix may address" "$OUT" \
  || { cat "$OUT"; fail "'runner up' did not refuse an unregistered serial"; }
log "refused: smix runner up (aimed at a serial that reaches nothing)"

step "6. and the refusal is about registration, not about being physical"
# A registered phone must become addressable — otherwise this checkpoint
# would have made physical devices unusable rather than governed, which
# is what a guard slides into when nobody checks its far side.
#
# Proven with a destructive verb, which stops at the *second* gate: past
# addressability, refused for being an un-opted-in phone. That shows both
# gates are live and distinct, and it reaches no device to show it.
smix sim register attached --udid "$SERIAL" --kind physical-android > "$OUT"
grep -q "registered:" "$OUT" || { cat "$OUT"; fail "registration failed"; }
smix sim uninstall attached com.example.nothing > "$OUT"
grep -q "is not a device smix may address" "$OUT" \
  && { cat "$OUT"; fail "registered and still unaddressable"; }
grep -q "destructive actions are not allowed" "$OUT" \
  || { cat "$OUT"; fail "past addressability but the destructive gate went quiet"; }
log "registered → addressable, then refused by the second gate — both live"

echo "C15-ADDRESSABILITY-PASS"
