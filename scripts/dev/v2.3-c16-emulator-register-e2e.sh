#!/usr/bin/env bash
# v2.3-C16: `--kind emulator` becomes an input that can succeed.
#
# It could not, until 2026-08-06. `DeviceKind::Emulator` was declared,
# exposed as a clap value and mapped through `to_registry` — and the
# registration path put every non-physical kind behind a CoreSimulator
# UDID shape check that no adb serial can pass, then hard-wrote the kind
# as `Simulator` anyway. A flag with no input that succeeds.
#
# What replaced it is symmetric rather than lenient: a virtual device is
# checked against the catalogue its own platform keeps — a simulator
# against `simctl list devices`, an emulator against `adb devices` — and
# only a physical device is taken as given, because nothing on this
# machine can enumerate the world's phones.
#
# Two case rules fell out of it, both measured rather than assumed:
#   * `devicectl` rejects the lower-case spelling of a UDID it accepts in
#     upper-case, so Apple identifiers are normalised.
#   * `adb` matches serials byte for byte, so they are not.
# Normalising happens once, at registration, where the kind is known.
#
# The half that needs an emulator says so when there is none. A suite
# that goes green with nothing running has told you nothing.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
WORK="$(mktemp -d)"
OUT="$(mktemp)"

log()  { printf '[c16-emu-reg] %s\n' "$*"; }
step() { printf '[c16-emu-reg] --- %s\n' "$*"; }
fail() { printf '[c16-emu-reg] FAIL: %s\n' "$*" >&2; exit 1; }

cleanup() { rm -rf "$WORK" "$OUT"; }
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX"
smix() { "$SMIX" "$@" 2>&1 | grep -v '^kevy:' || true; }

step "0. the shape and case rules, which need no device"
cargo test -p smix-simctl --lib kind_tests > "$OUT" 2>&1 \
  || { tail -20 "$OUT"; fail "registry kind tests failed"; }
grep -qE "test result: ok\. [0-9]+ passed" "$OUT" || fail "unit tests reported no pass"
log "$(grep -oE 'test result: ok\. [0-9]+ passed' "$OUT" | head -1)"

cd "$WORK"
mkdir -p .smix

step "1. each kind is told about its own world when the shape is wrong"
smix sim register a --udid 47ACEAE5-36BA-4C62-811B-F09B397910D7 --kind emulator > "$OUT"
grep -q "emulator-<port>" "$OUT" || { cat "$OUT"; fail "wrong world named for an emulator"; }
grep -q "8-4-4-4-12" "$OUT" && { cat "$OUT"; fail "named the simulator shape at an emulator"; }
smix sim register b --udid emulator-5554 --kind simulator > "$OUT"
grep -q "8-4-4-4-12" "$OUT" || { cat "$OUT"; fail "wrong world named for a simulator"; }
log "each refusal describes the kind actually being registered"

step "2. a phone is taken as given, because there is no catalogue"
smix sim register c --udid R5CT52DF07D --kind physical-android > "$OUT"
grep -q "registered:" "$OUT" || { cat "$OUT"; fail "a physical serial was refused"; }
# And its case survives: adb matches verbatim.
smix sim register d --udid lower-case-serial --kind physical-android > "$OUT"
smix sim resolve d > "$OUT"
grep -qx "lower-case-serial" "$OUT" || { cat "$OUT"; fail "an adb serial was mangled"; }
log "physical identifiers accepted and returned verbatim"

step "3. an Apple identifier is normalised, because devicectl is case-sensitive"
smix sim register e --udid 00008120-001410c11a42201e --kind physical-ios > "$OUT"
smix sim resolve e > "$OUT"
grep -qx "00008120-001410C11A42201E" "$OUT" \
  || { cat "$OUT"; fail "a lower-case Apple UDID was not rescued"; }
log "lower-case Apple UDID normalised to the form devicectl answers to"

step "4. an emulator that is not running is refused"
smix sim register f --udid emulator-9999 --kind emulator > "$OUT"
grep -q "adb lists no running device" "$OUT" || { cat "$OUT"; fail "an absent emulator was accepted"; }
log "refused, and the message names the catalogue it consulted"

step "5. a running emulator registers, and its alias is usable"
SERIAL="$(adb devices 2>/dev/null | awk -F'\t' '$2=="device" && $1 ~ /^emulator-/ { print $1; exit }' || true)"
if [ -z "$SERIAL" ]; then
  log "no emulator running — the success path cannot be exercised"
  log "start one (emulator -avd sim-smix-android-01) and re-run for a PASS"
  echo "C16-EMULATOR-REGISTER-SKIP"
  exit 0
fi
log "running emulator: $SERIAL"

smix sim register emu --udid "$SERIAL" --kind emulator > "$OUT"
grep -q "registered: emu → $SERIAL (Android emulator)" "$OUT" \
  || { cat "$OUT"; fail "registering a running emulator failed"; }

# Usable means two things, and both have failed before: the alias must
# resolve to a string adb answers to, and the Android runner path must
# reach the device through it.
smix sim resolve emu > "$OUT"
grep -qx "$SERIAL" "$OUT" || { cat "$OUT"; fail "the alias resolved to something adb cannot use"; }

smix runner down --platform android --device emu > "$OUT"
grep -q "device=$SERIAL" "$OUT" || { cat "$OUT"; fail "the runner path did not reach the alias"; }
log "registered, resolved verbatim, and reached by alias"

echo "C16-EMULATOR-REGISTER-PASS"
