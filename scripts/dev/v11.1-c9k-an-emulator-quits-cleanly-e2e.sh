#!/usr/bin/env bash
# v11.1-C9k: an emulator smix starts survives its caller's process group
# being ended, and `smix sim shutdown` returns once it has quit — with no
# crash report left behind either way.
#
# 2026-09-25: an emulator started from a short-lived shell died with it.
# The launcher received the group's signal and passed it on, and the
# headless qemu aborted in `skin_winsys_quit_request` called from a signal
# handler — an "Android Emulator quit unexpectedly" dialog on the owner's
# desktop. The owner had seen that dialog three or four times before.
#
# The judgement is the one the owner sees: no new `qemu-system-*.ips` in
# ~/Library/Logs/DiagnosticReports. The subject is our third AVD, which
# nothing else drives; its ledger is this script's own.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/e2e-devices.sh
source "$ROOT/scripts/lib/e2e-devices.sh"

AVD="${SMIX_C9K_AVD:-$E2E_ANDROID_THIRD}"
ALIAS="c9k-quits-cleanly"
REPORTS="$HOME/Library/Logs/DiagnosticReports"
WORK="$(mktemp -d)"

log()  { printf '[c9k-quit] %s\n' "$*"; }
step() { printf '[c9k-quit] --- %s\n' "$*"; }
fail() { printf '[c9k-quit] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c9k-quit] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

SERIAL=""
WE_BOOTED=no
cleanup() {
  [ -n "$SERIAL" ] && { e2e_stop_emulator "$SERIAL" || true; }
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX"
command -v adb >/dev/null 2>&1 || cannot_judge "no adb"
[ -d "$REPORTS" ] || cannot_judge "no $REPORTS to read crash reports from"

reports() { find "$REPORTS" -maxdepth 2 -name 'qemu-system-*.ips' 2>/dev/null | sort; }
listed() { adb devices 2>/dev/null | awk -v s="$1" '$1 == s { f = 1 } END { exit !f }'; }
booted() {
  for _ in $(seq 1 90); do
    [ "$(adb -s "$1" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && return 0
    sleep 2
  done
  return 1
}
free_port() {
  local p="$1"
  while [ "$p" -lt 5680 ]; do
    listed "emulator-$p" || { lsof -nP -iTCP:"$p" -sTCP:LISTEN >/dev/null 2>&1 || { echo "$p"; return 0; }; }
    p=$((p + 2))
  done
  return 1
}

# Asked of the process table, by the AVD's name: nothing here chooses a
# device from what adb lists.
pgrep -f -- "-avd $AVD( |$)" >/dev/null 2>&1 \
  && cannot_judge "$AVD is already running — this starts it from nothing"
PORT="$(free_port 5650)" || cannot_judge "no free emulator console port"
BEFORE="$(reports)"

e2e_isolate_machine "$WORK"
cd "$WORK"

step "0. our third AVD, registered in this script's ledger"
e2e_start_emulator "$WORK/register.log" -avd "$AVD" -port "$PORT" -no-boot-anim
SERIAL="emulator-$PORT"
booted "$SERIAL" || fail "$AVD did not come up to be registered — see $WORK/register.log"
"$SMIX" sim register "$ALIAS" --udid "$SERIAL" --kind emulator >"$WORK/reg.log" 2>&1 \
  || { cat "$WORK/reg.log" >&2; fail "could not register $AVD in this script's ledger"; }
e2e_stop_emulator "$SERIAL" || fail "the registration boot of $AVD did not quit"
SERIAL=""

step "1. smix boots it from a caller whose process group is then ended"
# `set -m` gives the subshell a process group of its own, as a terminal
# or a harness gives each command; the SIGINT below is what Ctrl-C or a
# deadline sends to that group.
set -m
( "$SMIX" sim boot "$ALIAS" >"$WORK/boot.log" 2>&1; sleep 30 ) &
CALLER=$!
set +m
# The caller is ended while `smix sim boot` is still waiting for the
# boot to finish — the moment a person reaches for Ctrl-C. It used to end
# at whatever point the boot had reached, which on a loaded machine was
# after the emulator had booted and before smix had recorded it
# (2026-09-25): the emulator lived, and smix refused to stop a device its
# ledger said it had not booted. The serial is the registered one; an
# emulator shows in `adb devices` as soon as its console is up.
for _ in $(seq 1 120); do
  listed "emulator-$PORT" && break
  sleep 0.5
done
listed "emulator-$PORT" || { cat "$WORK/boot.log" >&2; fail "emulator-$PORT never appeared"; }
SERIAL="emulator-$PORT"
WE_BOOTED=yes
T0=$SECONDS
kill -INT -- "-$CALLER" 2>/dev/null || true
wait "$CALLER" 2>/dev/null || true
# Witness that the signal landed mid-boot: `sim boot` waits up to 180 s
# and the caller then sleeps thirty, so an ended group is the only way
# back this fast. Without this, a signal that went nowhere would pass the
# judgement below.
[ $((SECONDS - T0)) -lt 20 ] || fail "the caller's group was not ended — the SIGINT went nowhere"
booted "$SERIAL" || fail "$SERIAL did not finish booting after its caller was ended"
sleep 5
listed "$SERIAL" || fail "the emulator died with its caller's process group"
NEW="$(comm -13 <(printf '%s\n' "$BEFORE") <(reports))"
[ -z "$NEW" ] || fail "a crash report appeared after the caller's group was ended: $NEW"
log "caller ended; $SERIAL still up, no crash report"

step "2. smix sim shutdown returns once it has quit"
[ "$WE_BOOTED" = yes ] || fail "shutting down an emulator this script did not boot"
"$SMIX" sim shutdown "$ALIAS" >"$WORK/shutdown.log" 2>&1 \
  || { cat "$WORK/shutdown.log" >&2; fail "smix sim shutdown failed"; }
listed "$SERIAL" && fail "sim shutdown returned while adb still listed $SERIAL"
SERIAL=""
sleep 5
NEW="$(comm -13 <(printf '%s\n' "$BEFORE") <(reports))"
[ -z "$NEW" ] || fail "a crash report appeared after shutdown: $NEW"
log "stopped and gone, no crash report"

echo "C9K-AN-EMULATOR-QUITS-CLEANLY-PASS"
