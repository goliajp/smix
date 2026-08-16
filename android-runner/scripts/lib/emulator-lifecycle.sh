#!/usr/bin/env bash
# The one emulator lifecycle every smoke script shares.
#
# Six scripts each carried their own copy: start an AVD on port 5554,
# poll for boot, and on teardown `adb -s emulator-5554 emu kill`. That
# last line stops whatever is on 5554 — and on a machine with two people
# on it, one day that was somebody else's emulator, six times a day.
# Nobody did anything wrong. Six copies of the same rule are six places
# for it to be applied to the wrong device.
#
# So: one lifecycle, and the port is not fixed. The emulator is started
# through smix on the alias the caller names (or a private one this lib
# registers), which writes the boot to the machine's ledger; teardown
# goes through smix too, and smix refuses to stop a device its ledger
# does not say it booted. What this lib stops is what this lib started.
#
# Callers source this and then use $SERIAL. They do not touch the
# emulator's start or stop themselves.
#
#   . "$(dirname "$0")/lib/emulator-lifecycle.sh"
#   smoke_emulator_up            # sets SERIAL, blocks until booted
#   ...
#   smoke_emulator_down          # only what smoke_emulator_up started
#
# Env:
#   SMIX_BIN          which smix; defaults to a workspace build, then PATH
#   SMOKE_ALIAS       registered alias to boot; default: smoke-android
#   SMOKE_AVD         AVD name to register under that alias if it is not
#                     registered yet; default: sim-smix-android-01
#   SMOKE_PORT        console port when registering; default: 5580 — off
#                     the 5554 everybody else's tools land on
set -euo pipefail

_smoke_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
_smoke_repo="$(cd "$_smoke_root/.." && pwd)"

_smoke_pick_smix() {
  if [ -n "${SMIX_BIN:-}" ] && [ -x "$SMIX_BIN" ]; then
    printf '%s' "$SMIX_BIN"; return 0
  fi
  for c in "$_smoke_repo/target/release/smix" "$_smoke_repo/target/debug/smix" \
           "$(command -v smix 2>/dev/null || true)"; do
    [ -n "$c" ] && [ -x "$c" ] && { printf '%s' "$c"; return 0; }
  done
  echo "emulator-lifecycle: no smix binary — build the workspace or set SMIX_BIN" >&2
  return 1
}

SMOKE_ALIAS="${SMOKE_ALIAS:-smoke-android}"
SMOKE_AVD="${SMOKE_AVD:-sim-smix-android-01}"
SMOKE_PORT="${SMOKE_PORT:-5580}"
SERIAL=""
_smoke_we_booted=0

smoke_emulator_up() {
  local smix; smix="$(_smoke_pick_smix)" || return 1

  # Registration is what makes an emulator addressable through smix, and
  # what lets the ledger record who booted it. Register once if needed;
  # a registration needs the emulator running to be checked against
  # adb, so this is the one place a raw `emulator` command is allowed —
  # for the first-ever boot of a private alias on a private port.
  if ! "$smix" sim resolve "$SMOKE_ALIAS" >/dev/null 2>&1; then
    echo "emulator-lifecycle: '$SMOKE_ALIAS' is not registered — first-time setup on port $SMOKE_PORT" >&2
    "${ANDROID_HOME:-$HOME/Library/Android/sdk}/emulator/emulator" -avd "$SMOKE_AVD" \
      -port "$SMOKE_PORT" -no-audio -no-boot-anim -no-snapshot-save \
      >/tmp/smix-smoke-emulator-first-boot.log 2>&1 &
    local first_pid=$!
    local s="emulator-$SMOKE_PORT"
    until [ "$(adb -s "$s" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; do sleep 5; done
    "$smix" sim register "$SMOKE_ALIAS" --udid "$s" --kind emulator >/dev/null
    # Hand it back off so the real boot below goes through smix and is
    # recorded as smix's. A first boot is setup, not the run.
    adb -s "$s" emu kill >/dev/null 2>&1 || true
    wait "$first_pid" 2>/dev/null || true
    sleep 3
  fi

  SERIAL="$("$smix" sim resolve "$SMOKE_ALIAS" 2>/dev/null | grep -v '^kevy:' | tr -d '[:space:]')"
  [ -n "$SERIAL" ] || { echo "emulator-lifecycle: could not resolve $SMOKE_ALIAS" >&2; return 1; }

  echo "=== boot $SMOKE_ALIAS ($SERIAL) through smix ==="
  # `smix sim boot` waits for sys.boot_completed itself and records the
  # boot in the machine's ledger as smix's — or refuses to record it if
  # the device did not come up. Either way the ledger tells the truth.
  "$smix" sim boot "$SMOKE_ALIAS" 2>&1 | grep -v '^kevy:'
  _smoke_we_booted=1
  echo "boot ok — API $(adb -s "$SERIAL" shell getprop ro.build.version.sdk | tr -d '\r')"
}

smoke_emulator_down() {
  [ "$_smoke_we_booted" = 1 ] || return 0
  local smix; smix="$(_smoke_pick_smix)" || return 1
  echo "=== teardown $SMOKE_ALIAS ($SERIAL) through smix ==="
  # smix consults the ledger: this stops the device only if the ledger
  # says smix booted it, which smoke_emulator_up made true. An emulator
  # somebody else put on this port in the meantime is refused, loudly.
  "$smix" sim shutdown "$SMOKE_ALIAS" 2>&1 | grep -v '^kevy:' || true
}
