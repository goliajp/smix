#!/usr/bin/env bash
# v10.2-C13d: an alias names a device, not the port it answered on once.
#
# `emulator-5554` is a slot. Whoever boots first takes it, so a registry
# row holding only a serial names whichever emulator is there today. On
# 2026-09-23 the row for `sim-smix-android-01` recorded `emulator-5554`
# while that port belonged to a consumer's `qip-consumer-36`, and every
# alias-driven install, flow and `runner up` would have gone to their
# device (open-items P1).
#
# The identity has been written down since emulators became registrable
# — `smix sim register` reads the AVD name, `smix sim boot` starts the
# device by it. Nothing read it back when resolving. This drives that
# reading, over four rows built here rather than found:
#
#   probe   the AVD is live on a different port than recorded → follow it
#   ghost   the AVD is not running, its old port is somebody else → refuse, name them
#   legacy  no identity recorded at all → refuse, even though the port is right
#   phone   a physical serial is an identity already → unchanged
#
# The registry it reads is built in a scratch directory (SMIX_SIMS_JSON),
# so this never writes to the machine's device records.
#
# Three codes, per the contract: 0 judged and right, 1 judged and wrong
# or the setup this run owns did not come up, 2 could not judge.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"

source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/e2e-devices.sh"
AVD_ABSENT="${SMIX_C13D_AVD_ABSENT:-$E2E_ANDROID_SECOND}"
WORK="$(mktemp -d)"

log()  { printf '[c13d] %s\n' "$*" >&2; }
step() { printf '[c13d] --- %s\n' "$*" >&2; }
fail() { printf '[c13d] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c13d] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

# This gate starts nothing and stops nothing: it reads a registry it
# wrote itself and asks devices who they are.
cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || cannot_judge "adb is not on PATH"
[ -x "$SMIX" ] || cannot_judge "no smix binary at $SMIX"

# The device this run drives is the ledger's answer or the caller's —
# never "the first emulator adb lists". That rule is what this whole
# segment is about, and a gate that broke it while fixing it would be
# arguing with itself (`no-script-picks-a-device-by-accident` said so,
# about this file's first draft).
step "the device this run owns"
MINE="${SMIX_ANDROID_SERIAL:-$(bash "$ROOT/scripts/dev/pick-dev-emulator.sh" 2>/dev/null || true)}"
[ -n "$MINE" ] || cannot_judge "no emulator this machine's ledger says smix booted. \
Start yours (\`smix sim boot <alias>\`) or name it with SMIX_ANDROID_SERIAL"
case "$MINE" in
  emulator-*) : ;;
  *) cannot_judge "$MINE is not an emulator; this gate is about emulator ports" ;;
esac
# Its identity, asked of the device itself. Not `smix sim resolve`: that
# is the code under test, and a gate that asks the code under test for
# the truth it is checking has stopped checking anything.
AVD_MINE="$(adb -s "$MINE" emu avd name 2>/dev/null | tr -d '\r' | grep -v '^OK$' | head -1)"
[ -n "$AVD_MINE" ] || cannot_judge "$MINE did not say which AVD it is running"
log "  $MINE is running $AVD_MINE"

# A port this alias was NOT registered on. The caller may name a second
# emulator (then the refusals can name its AVD too); otherwise a console
# port two slots past this one, asserted to hold nothing. It is never
# driven — it goes into a row precisely because nothing answers there.
OTHER="${SMIX_C13D_OTHER:-}"
if [ -z "$OTHER" ]; then
  OTHER="emulator-$(( ${MINE##*-} + 44 ))"
  if adb -s "$OTHER" get-state >/dev/null 2>&1; then
    cannot_judge "$OTHER answers, so it is not the unheld port this needs; \
name a second emulator with SMIX_C13D_OTHER"
  fi
  OTHER_AVD=""
  log "  the port this row will name is $OTHER, and nothing answers there"
else
  OTHER_AVD="$(adb -s "$OTHER" emu avd name 2>/dev/null | tr -d '\r' | grep -v '^OK$' | head -1)"
  log "  the port this row will name is $OTHER, running $OTHER_AVD (not touched)"
fi

step "a registry built for this run, in $WORK"
cat >"$WORK/sims.json" <<JSON
{"sims": {
  "c13d-probe":  {"deviceName":"c13d-probe","kind":"emulator","destructiveOptIn":false,
                  "udid":"$OTHER","runtime":"","deviceType":"","avdName":"$AVD_MINE"},
  "c13d-ghost":  {"deviceName":"c13d-ghost","kind":"emulator","destructiveOptIn":false,
                  "udid":"$OTHER","runtime":"","deviceType":"","avdName":"$AVD_ABSENT"},
  "c13d-legacy": {"deviceName":"c13d-legacy","kind":"emulator","destructiveOptIn":false,
                  "udid":"$MINE","runtime":"","deviceType":""},
  "c13d-legacy-free": {"deviceName":"c13d-legacy-free","kind":"emulator","destructiveOptIn":false,
                  "udid":"$OTHER","runtime":"","deviceType":""},
  "c13d-phone":  {"deviceName":"c13d-phone","kind":"physicalAndroid","destructiveOptIn":false,
                  "udid":"R5NOTAREALPHONE","runtime":"","deviceType":""}
}}
JSON
export SMIX_SIMS_JSON="$WORK/sims.json"

resolve() { "$SMIX" sim resolve "$1" 2>"$WORK/$1.err" | tail -1; }

step "an alias follows its AVD to whatever port it took"
got="$(resolve c13d-probe || true)"
[ "$got" = "$MINE" ] || fail "c13d-probe records the port $OTHER and the AVD $AVD_MINE, \
which is on $MINE — resolve said '$got'"
grep -q "not the $OTHER it was registered on" "$WORK/c13d-probe.err" \
  || fail "c13d-probe resolved to $got without saying it had moved: \
$(tr '\n' ' ' <"$WORK/c13d-probe.err")"
log "  probe=followed ($AVD_MINE on $MINE, registered on $OTHER)"

step "an AVD that is not running is refused, and the port's real tenant is named"
if resolve c13d-ghost >/dev/null 2>&1; then
  fail "c13d-ghost names $AVD_ABSENT, which is not running, and resolve answered anyway"
fi
grep -q "$AVD_ABSENT" "$WORK/c13d-ghost.err" \
  || fail "the refusal for c13d-ghost does not name the AVD: $(tr '\n' ' ' <"$WORK/c13d-ghost.err")"
if [ -n "$OTHER_AVD" ]; then
  grep -q "$OTHER_AVD" "$WORK/c13d-ghost.err" \
    || fail "the refusal does not name $OTHER_AVD, which holds the port it was registered on: \
$(tr '\n' ' ' <"$WORK/c13d-ghost.err")"
  log "  ghost=refused (named $AVD_ABSENT and $OTHER_AVD)"
else
  log "  ghost=refused (named $AVD_ABSENT; no tenant on $OTHER to name)"
fi

step "a row with no identity is refused even when its port is the right one"
if resolve c13d-legacy >/dev/null 2>&1; then
  fail "c13d-legacy records only the port $MINE and resolve answered with it — \
a port that is right today is still not an identity"
fi
grep -q "records the port" "$WORK/c13d-legacy.err" \
  || fail "the refusal for c13d-legacy does not say what is wrong with the row: \
$(tr '\n' ' ' <"$WORK/c13d-legacy.err")"
log "  legacy=refused (its port was the right one, and that is not the point)"

# The other direction. An identityless row whose port nobody holds is
# the tempting one: there is no stranger to collide with, so answering
# with the port looks harmless. It is the same unknown, and letting an
# empty port stand in for it is exactly how a port becomes an identity.
if resolve c13d-legacy-free >/dev/null 2>&1; then
  fail "c13d-legacy-free records only the port $OTHER and resolve answered \
with it — an empty port is not evidence about which device this row means"
fi
grep -q "records the port" "$WORK/c13d-legacy-free.err" \
  || fail "the refusal for c13d-legacy-free does not say what is wrong with the row: \
$(tr '\n' ' ' <"$WORK/c13d-legacy-free.err")"
log "  legacy-free=refused (nobody holds that port, and it is still not an identity)"

step "a physical serial is an identity already"
got="$(resolve c13d-phone || true)"
[ "$got" = "R5NOTAREALPHONE" ] || fail "c13d-phone should resolve verbatim, got '$got'"
log "  phone=verbatim"

printf '[c13d] C13D-ALIAS-E2E-PASS (%s on %s, registered on %s)\n' "$AVD_MINE" "$MINE" "$OTHER" >&2
