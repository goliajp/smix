#!/usr/bin/env bash
# Run a command while one of this repository's emulators is under load.
#
#   load-our-emulator.sh run <serial> --guest-hog <N> -- <command…>
#   load-our-emulator.sh run <serial> --vm-pause <percent> -- <command…>
#
# Some defects show only when the machine is loaded: a probe that says
# "settled" in the middle of a scroll, a refusal that takes longer than it
# promised, a look that stalls for seconds. Until now they reproduced only
# inside a release dry-run, at whatever load the other sessions on the
# machine happened to make, and loading the whole machine on purpose is
# not allowed — it is every other session's machine too.
#
# So the load goes only to the emulator under test, in two shapes, one
# per way a loaded machine reaches an app:
#
#   --guest-hog N    N busy loops inside the emulator. The app's main
#                    thread competes for the guest's CPUs; the host does
#                    not notice.
#   --vm-pause P     the emulator's qemu stopped for P% of every 200 ms.
#                    Everything in the guest stops together, clocks keep
#                    going: what a host too busy to schedule the VM does.
#
# The load lasts exactly as long as the command. Whatever ends the command,
# including an interrupt, the busy loops are killed and qemu is continued
# before this exits. Its exit code is the command's.
#
# It acts only on an emulator of this repository's own AVDs
# (sim-smix-android-*), and only on the one qemu process whose command line
# names that AVD and the serial's console port; anything else, or more
# than one candidate, and it refuses with exit 2 (could not judge).
set -euo pipefail

PERIOD_MS=200
HOG_PIDS=/data/local/tmp/smix-load-hogs.pids

say() { printf '[load-our-emulator] %s\n' "$*" >&2; }
cannot() { say "$*"; exit 2; }

[ "${1:-}" = run ] || cannot "usage: load-our-emulator.sh run <serial> (--guest-hog N | --vm-pause P) -- <command…>"
SERIAL="${2:-}"
MODE="${3:-}"
AMOUNT="${4:-}"
[ "${5:-}" = -- ] || cannot "usage: load-our-emulator.sh run <serial> (--guest-hog N | --vm-pause P) -- <command…>"
shift 5
[ "$#" -gt 0 ] || cannot "no command to run under load"
case "$AMOUNT" in '' | *[!0-9]*) cannot "the amount must be a whole number, got '$AMOUNT'" ;; esac

case "$SERIAL" in
  emulator-[0-9]*) PORT_ARG="${SERIAL#emulator-}" ;;
  *) cannot "'$SERIAL' is not an emulator serial — only an emulator of this repository is loaded" ;;
esac
AVD="$(adb -s "$SERIAL" emu avd name 2>/dev/null | head -1 | tr -d '\r')"
case "$AVD" in
  sim-smix-android-*) : ;;
  *) cannot "$SERIAL runs AVD '${AVD:-<none>}', which is not one of this repository's (sim-smix-android-*)" ;;
esac

qemu_pid() {
  # Every qemu on the machine, filtered to the one whose own arguments name
  # this AVD and this console port. Only that PID is ever signalled.
  ps -axo pid=,args= | python3 -c '
import shlex, sys
avd, port = sys.argv[1], sys.argv[2]
hits = []
for line in sys.stdin:
    pid, _, args = line.strip().partition(" ")
    try:
        argv = shlex.split(args)
    except ValueError:
        continue
    if not argv or "qemu-system" not in argv[0].rsplit("/", 1)[-1]:
        continue
    named = any(a == "-avd" and b == avd for a, b in zip(argv, argv[1:]))
    ported = any(a == "-port" and b == port for a, b in zip(argv, argv[1:]))
    if named and (ported or "-port" not in argv):
        hits.append(pid)
print(" ".join(hits))
' "$AVD" "$PORT_ARG"
}

PAUSER="" HOGGED=0 QEMU=""
unload() {
  if [ -n "$PAUSER" ]; then
    kill "$PAUSER" 2>/dev/null || true
    wait "$PAUSER" 2>/dev/null || true
  fi
  if [ -n "$QEMU" ]; then
    kill -CONT "$QEMU" 2>/dev/null || true
  fi
  if [ "$HOGGED" = 1 ]; then
    adb -s "$SERIAL" shell "if [ -f $HOG_PIDS ]; then kill \$(cat $HOG_PIDS) 2>/dev/null; rm -f $HOG_PIDS; fi" \
      >/dev/null 2>&1 || say "warning: could not stop the busy loops in $SERIAL"
  fi
}

case "$MODE" in
  --guest-hog)
    [ "$AMOUNT" -ge 1 ] || cannot "--guest-hog needs at least one loop"
    trap unload EXIT
    trap 'exit 130' INT TERM
    HOGGED=1
    adb -s "$SERIAL" shell "rm -f $HOG_PIDS; i=0; while [ \$i -lt $AMOUNT ]; do
        sh -c 'while :; do :; done' </dev/null >/dev/null 2>&1 &
        echo \$! >> $HOG_PIDS; i=\$((i + 1)); done" \
      || cannot "could not start the busy loops in $SERIAL"
    say "$AMOUNT busy loop(s) running in $SERIAL ($AVD)"
    ;;
  --vm-pause)
    [ "$AMOUNT" -ge 1 ] && [ "$AMOUNT" -le 90 ] || cannot "--vm-pause takes 1 to 90 (percent of each ${PERIOD_MS} ms)"
    QEMU="$(qemu_pid)"
    case "$QEMU" in
      '') cannot "no qemu process names AVD $AVD on port $PORT_ARG" ;;
      *' '*) cannot "more than one qemu names AVD $AVD on port $PORT_ARG ($QEMU) — not guessing which" ;;
    esac
    trap unload EXIT
    trap 'exit 130' INT TERM
    python3 -c '
import os, signal, sys, time
pid, share, period = int(sys.argv[1]), int(sys.argv[2]) / 100, int(sys.argv[3]) / 1000
def stop(*_):
    os.kill(pid, signal.SIGCONT)
    sys.exit(0)
signal.signal(signal.SIGTERM, stop)
signal.signal(signal.SIGINT, stop)
try:
    while True:
        os.kill(pid, signal.SIGSTOP)
        time.sleep(period * share)
        os.kill(pid, signal.SIGCONT)
        time.sleep(period * (1 - share))
finally:
    os.kill(pid, signal.SIGCONT)
' "$QEMU" "$AMOUNT" "$PERIOD_MS" &
    PAUSER=$!
    say "qemu $QEMU ($AVD) stopped for $AMOUNT% of every ${PERIOD_MS} ms"
    ;;
  *) cannot "unknown load '$MODE' — --guest-hog N or --vm-pause P" ;;
esac

rc=0
"$@" || rc=$?
exit "$rc"
