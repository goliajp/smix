#!/usr/bin/env bash
# load-our-emulator.sh does what it says, measured inside the emulator.
#
# The measure is a fixed piece of arithmetic run in the guest's shell,
# timed by the host. Each load has to make it at least 1.5 times slower
# than with no load — a load that does not slow the guest reproduces
# nothing — and when the command is over nothing may be left: qemu not
# in the stopped state, no busy loop in the guest. A serial that is not
# an emulator, or an emulator that is not one of ours, is refused before
# anything is signalled.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
ALIAS="${SMIX_LOAD_ANDROID:-$E2E_ANDROID}"
LOAD="$ROOT/scripts/dev/load-our-emulator.sh"

log()  { printf '[load-test] %s\n' "$*" >&2; }
fail() { printf '[load-test] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[load-test] SKIP: %s\n' "$*" >&2; exit 2; }

SERIAL="" WE_BOOTED=0
cleanup() {
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || cannot_judge "no adb on PATH"
[ -x "$SMIX" ] || cannot_judge "no smix binary at $SMIX"

# Refusals first: nothing is booted or signalled for these.
rc=0; "$LOAD" run R5XXXXXXXXX --vm-pause 50 -- true 2>/dev/null || rc=$?
[ "$rc" = 2 ] || fail "a phone-shaped serial was not refused with exit 2 (got $rc)"
rc=0; "$LOAD" run emulator-5554 --guest-hog 1 -- 2>/dev/null || rc=$?
[ "$rc" = 2 ] || fail "a call with no command was not refused with exit 2 (got $rc)"
log "refusals: a phone serial and an empty command, both exit 2"

if ! SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" \
   || [ -z "$SERIAL" ] || ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  log "booting $ALIAS"
  with_deadline 300 "$SMIX" sim boot "$ALIAS" >/dev/null 2>&1 || fail "could not boot $ALIAS"
  WE_BOOTED=1
  SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" || fail "$ALIAS was booted and does not resolve"
fi
case "$SERIAL" in emulator-*) : ;; *) fail "$ALIAS resolves to '$SERIAL', not an emulator" ;; esac
for _ in $(seq 1 60); do
  adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done

# The fixed work: seconds, as a decimal, for one run of it in the guest.
work() {
  python3 - "$SERIAL" <<'PY'
import subprocess, sys, time
t = time.time()
subprocess.run(["adb", "-s", sys.argv[1], "shell",
                "i=0; while [ $i -lt 600000 ]; do i=$((i + 1)); done"],
               check=True, capture_output=True)
print(f"{time.time() - t:.3f}")
PY
}
export -f work
export SERIAL

# The quickest of three, so one hiccup does not decide a ratio.
best() { # $1.. optional load prefix
  local b="" t i
  for i in 1 2 3; do
    if [ "$#" -gt 0 ]; then t="$("$@" bash -c work)"; else t="$(work)"; fi
    if [ -z "$b" ] || python3 -c "import sys; sys.exit(not float('$t') < float('$b'))"; then b="$t"; fi
  done
  printf '%s' "$b"
}

base="$(best)"
hog="$(best "$LOAD" run "$SERIAL" --guest-hog 16 --)"
pause="$(best "$LOAD" run "$SERIAL" --vm-pause 60 --)"
log "fixed work in the guest: no load ${base}s, 16 busy loops ${hog}s, qemu paused 60% ${pause}s"
python3 - "$base" "$hog" "$pause" <<'PY' || exit 1
import sys
base, hog, pause = map(float, sys.argv[1:])
bad = []
if base <= 0:
    bad.append("the unloaded run took no time — the measure is not measuring")
if hog < 1.5 * base:
    bad.append(f"busy loops slowed the guest {hog / base:.2f}x, under 1.5x")
if pause < 1.5 * base:
    bad.append(f"pausing qemu slowed the guest {pause / base:.2f}x, under 1.5x")
for b in bad:
    print(f"[load-test] FAIL: {b}", file=sys.stderr)
sys.exit(1 if bad else 0)
PY

# Nothing left behind.
# A busy loop's own command line, matched whole: the shell asking the
# question carries the same words inside a longer line.
pidfile="$(adb -s "$SERIAL" shell 'ls /data/local/tmp/smix-load-hogs.pids 2>/dev/null; true' | tr -d '\r')"
[ -z "$pidfile" ] || fail "the load left its pid file behind in the guest"
loops="$(adb -s "$SERIAL" shell 'ps -A -o ARGS=' | tr -d '\r' | grep -cx 'sh -c while :; do :; done' || true)"
[ "${loops:-0}" = 0 ] || fail "the load left $loops spinning shell(s) behind in the guest"
avd="$(adb -s "$SERIAL" emu avd name 2>/dev/null | head -1 | tr -d '\r')"
state="$(ps -axo stat=,args= | awk -v a="$avd" 'index($0, "qemu-system") && index($0, "-avd " a) { print $1 }')"
[ -n "$state" ] || fail "could not find qemu for $avd to check its state"
case "$state" in T*) fail "the load left qemu for $avd stopped ($state)" ;; esac
log "after both: no busy loop in the guest, qemu running ($state)"

# The command's exit code is passed through, load or not.
rc=0; "$LOAD" run "$SERIAL" --vm-pause 10 -- sh -c 'exit 7' 2>/dev/null || rc=$?
[ "$rc" = 7 ] || fail "the command's exit code 7 came back as $rc"
log "LOAD-OUR-EMULATOR-PASS"
