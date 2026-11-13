#!/usr/bin/env bash
# v26.1-b: an emulator is registered while it is off, by its AVD's name,
# and `smix sim boot` then starts that AVD on the serial's port.
#
# 2026-11-13: once devices could only be started through smix, a new AVD
# had no way in. Registering an emulator read the AVD name off the running
# device, so it had to be running first, and starting it outside smix was
# what the fence had just forbidden.
#
# The ledger and registry are this script's own (SMIX_MACHINE_DIR).
#   SMIX_E2E_REGISTER_AVD=<name>  default: the third smix AVD
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/e2e-devices.sh
source "$ROOT/scripts/lib/e2e-devices.sh"

AVD="${SMIX_E2E_REGISTER_AVD:-$E2E_ANDROID_THIRD}"
ALIAS="register-while-off"
WORK="$(mktemp -d)"

log()  { printf '[register-off] %s\n' "$*"; }
step() { printf '[register-off] --- %s\n' "$*"; }
fail() { printf '[register-off] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[register-off] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

# yes once this script has started the emulator itself; it refuses to
# start when the AVD is already running
WE_BOOTED=no
SERIAL=""
cleanup() {
  [ -n "$SERIAL" ] && e2e_stop_emulator "$SERIAL"
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || cannot_judge "no adb"
pgrep -f -- "-avd $AVD( |$)" >/dev/null 2>&1 && cannot_judge "$AVD is already running"
listed() { adb devices 2>/dev/null | awk -v s="$1" '$1 == s { f = 1 } END { exit !f }'; }
free_console_port() {
  local p=5660
  while [ "$p" -lt 5680 ]; do
    listed "emulator-$p" || { lsof -nP -iTCP:"$p" -sTCP:LISTEN >/dev/null 2>&1 || { echo "$p"; return 0; }; }
    p=$((p + 2))
  done
  return 1
}
PORT="$(free_console_port)" || cannot_judge "no free emulator console port"
WANT="emulator-$PORT"
e2e_isolate_machine "$WORK"

step "1. off, and no AVD named: refused, and told about --avd"
"$SMIX" sim register "$ALIAS" --udid "$WANT" --kind emulator > "$WORK/r0.txt" 2>&1 \
  && fail "an emulator that is off was registered with nothing saying which AVD it is"
grep -q -- "--avd" "$WORK/r0.txt" || { cat "$WORK/r0.txt"; fail "the refusal does not name --avd"; }

step "2. an AVD that is not here: refused"
"$SMIX" sim register "$ALIAS" --udid "$WANT" --kind emulator --avd "no-such-avd-$$" > "$WORK/r1.txt" 2>&1 \
  && fail "an AVD that does not exist was registered"
grep -q "no AVD named" "$WORK/r1.txt" || { cat "$WORK/r1.txt"; fail "the refusal does not say the AVD is missing"; }

step "3. off, with --avd: registered"
"$SMIX" sim register "$ALIAS" --udid "$WANT" --kind emulator --avd "$AVD" > "$WORK/r2.txt" 2>&1 \
  || { cat "$WORK/r2.txt"; fail "registering $AVD while it is off failed"; }
listed "$WANT" && fail "registering started something on $WANT"

step "4. smix sim boot starts that AVD on that port, and answers for it"
"$SMIX" sim boot "$ALIAS" --wait 600 > "$WORK/boot.txt" 2>&1 \
  || { cat "$WORK/boot.txt"; fail "smix sim boot $ALIAS failed"; }
SERIAL="$WANT"
WE_BOOTED=yes
args="$(ps -axo args | grep -E -- "-avd $AVD( |$)" | grep -v grep | head -1)"
case " $args " in *" -port $PORT "*) ;; *) fail "$AVD did not start on port $PORT: $args" ;; esac
listed "$WANT" || fail "adb does not list $WANT after the boot"
"$SMIX" lease owner "$WANT" > "$WORK/owner.txt" 2>&1 || { cat "$WORK/owner.txt"; fail "the ledger does not say smix booted $WANT"; }
"$SMIX" sim list 2>&1 | sed -n '/not started by smix/,$p' | grep -q "$WANT" && fail "sim list names $WANT as started outside smix"

step "5. smix sim shutdown stops it"
[ "$WE_BOOTED" = yes ] || fail "nothing here booted $WANT"
"$SMIX" sim shutdown "$ALIAS" > "$WORK/down.txt" 2>&1 || { cat "$WORK/down.txt"; fail "smix sim shutdown $ALIAS failed"; }
SERIAL=""

log "PASS"
