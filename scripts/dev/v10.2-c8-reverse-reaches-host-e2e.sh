#!/usr/bin/env bash
#
# v10.2-C8: a device reaches a service on this machine.
#
# A consumer's Android round (2026-09-22) starts a server on the host and
# drives the app against it. An emulator has 10.0.2.2 to remember; a
# phone on the cable has nothing at all, and raw `adb reverse` against a
# physical serial is refused by the plugin's adb guard — rightly, since
# nothing would record what it opened. So the round could not run on a
# phone.
#
# Three states, and the middle one is what `smix sim reverse` adds:
#
#   before=unreachable  the device cannot reach the stub
#   after=reachable     with the route open it reads the stub's own token
#   removed=unreachable --remove puts it back
#
# The device dials one port and the stub serves another, so the route's
# direction is part of what is judged: a pair built the wrong way round
# would point the device at a port nothing answers on.
#
# The two "unreachable" ends are what stops this from being a test that
# cannot fail: a probe that was broken — wrong port, dead stub, nc
# missing — would read unreachable in all three, and the middle
# assertion is the one that then goes red. The host reaches its own stub
# first, so "the device cannot reach it" can never be a stub that was
# never listening.
#
# Usage: v10.2-c8-reverse-reaches-host-e2e.sh [alias]
#
# The device comes from the registry, like every other device script
# here: an alias in, `smix sim resolve` out. It shuts the emulator down
# only if it was the one that booted it, and always closes the route it
# opened and the stub it started.
#
# Exit 0 pass · 1 a judgement failed, or the machine is not in a state
# where this can be judged honestly · 2 no device to judge against.

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/e2e-devices.sh"
ALIAS="${1:-${SMIX_C8_ANDROID:-$E2E_ANDROID}}"
SERIAL=""
WE_BOOTED=0
# Two different numbers, deliberately. The ordinary use of this verb
# has the same port on both sides, and with the same number a route
# built device-then-host and one built host-then-device are the same
# route — a test using it could not tell them apart, and the first
# version of this script proved that by passing with the two swapped.
# So the device dials one number and the host serves another, which is
# also the only device-level exercise `--to` gets.
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port _C8_HOST_PORT
gate_free_port _C8_DEVICE_PORT
HOST_PORT="${SMIX_E2E_STUB_PORT:-$_C8_HOST_PORT}"
DEVICE_PORT="${SMIX_E2E_DEVICE_PORT:-$_C8_DEVICE_PORT}"
TOKEN="smix-c8-$$-$(date +%s)"
STUB_PID=""

log() { echo "[c8-reverse] $*"; }
fail() { log "FAIL: $*"; exit 1; }

command -v adb >/dev/null 2>&1 || fail "no adb on PATH — this judges an Android route and cannot"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"

SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)"
if [ -z "$SERIAL" ]; then
  log "nothing is registered as $ALIAS"
  # 2, not 0: nothing was judged. A run that could not look and a run
  # that looked and liked what it saw must not end the same way.
  exit 2
fi

# Never a phone. Every adb below names $SERIAL, and an unpinned adb
# command on a developer machine reaches whatever is attached — often a
# phone; one has been wiped that way.
case "$SERIAL" in
  emulator-[0-9]*) ;;
  *)
    log "$ALIAS resolves to $SERIAL, which is not an emulator — refusing."
    log "a physical device is somebody's own, and this opens a route on it."
    exit 2
    ;;
esac

if ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  log "booting $ALIAS"
  "$SMIX" sim boot "$ALIAS" >/dev/null 2>&1 || fail "could not boot $ALIAS"
  WE_BOOTED=1
  adb -s "$SERIAL" wait-for-device
fi

# A port somebody else holds is not a reason to pass quietly. The
# suite's older scripts skip here and exit 0, which is how a red run
# came back green while a runner held the port (open-items I4).
if lsof -b -w -nP -iTCP:"$HOST_PORT" -sTCP:LISTEN >/dev/null 2>&1; then
  log "port $HOST_PORT is already bound:"
  lsof -b -w -nP -iTCP:"$HOST_PORT" -sTCP:LISTEN | sed 's/^/[c8-reverse]   /'
  fail "this needs $HOST_PORT to serve the stub — free it, or pass SMIX_E2E_STUB_PORT"
fi

cleanup() {
  # Direct adb, not smix: this has to close the route even when the code
  # under test is the thing that is broken.
  if [ -n "$SERIAL" ]; then
    adb -s "$SERIAL" reverse --remove "tcp:$DEVICE_PORT" >/dev/null 2>&1
    adb -s "$SERIAL" reverse --remove "tcp:$HOST_PORT" >/dev/null 2>&1
  fi
  [ -n "$STUB_PID" ] && kill "$STUB_PID" >/dev/null 2>&1
  # Only what we started. A script that switches off a device it was
  # lent is how one run fails the four after it.
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
  return 0
}
trap cleanup EXIT

python3 "$ROOT/scripts/dev/v10.2-c8-reverse-stub.py" "$HOST_PORT" "$TOKEN" >/tmp/smix-c8-stub.log 2>&1 &
STUB_PID=$!
for _ in 1 2 3 4 5 6 7 8 9 10; do
  grep -q listening /tmp/smix-c8-stub.log 2>/dev/null && break
  sleep 0.3
done
grep -q listening /tmp/smix-c8-stub.log 2>/dev/null \
  || fail "the stub never bound $HOST_PORT: $(cat /tmp/smix-c8-stub.log)"

# The stub answers this machine. Corroboration independent of the
# device: without it, "the device cannot reach it" and "there was
# nothing to reach" are the same reading.
host_says="$(printf 'GET / HTTP/1.0\r\n\r\n' | nc -w 2 127.0.0.1 "$HOST_PORT" 2>&1)"
case "$host_says" in
  *"$TOKEN"*) log "stub=serving             (this machine reads its own token on $HOST_PORT)" ;;
  *) fail "the stub is not serving on $HOST_PORT — nothing below could be judged: $host_says" ;;
esac

# `nc` on the device needs the request to be followed by a blank line
# and the connection held open a moment; measured on API 36, a bare
# `echo | nc` connects and prints nothing at all.
device_reads() {
  adb -s "$SERIAL" shell '(echo "GET / HTTP/1.0"; echo; sleep 1) | nc 127.0.0.1 '"$DEVICE_PORT" 2>&1
}

ledger_row() {
  python3 - "$SERIAL" "$DEVICE_PORT" "$HOST_PORT" <<'PY'
import json, os, sys
serial, device_port, host_port = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
home = os.path.expanduser("~")
path = os.path.join(
    os.environ.get("XDG_DATA_HOME", os.path.join(home, ".local", "share")),
    "smix", "leases", serial + ".json",
)
if not os.path.exists(path):
    print("none")
    sys.exit(0)
rows = json.load(open(path)).get("resources", [])
for r in rows:
    if r.get("kind") == "reversePort" and r.get("devicePort") == device_port:
        print("%d->%d" % (r["devicePort"], r["hostPort"]))
        sys.exit(0)
print("none")
PY
}

before="$(device_reads)"
case "$before" in
  *"$TOKEN"*) fail "the device already reaches $DEVICE_PORT before any route was opened — \
something else is routing it, and nothing below would mean anything" ;;
  *) log "before=unreachable       ($(echo "$before" | tr -d '\r' | head -1))" ;;
esac

"$SMIX" sim reverse "$SERIAL" "$DEVICE_PORT" --to "$HOST_PORT" >/tmp/smix-c8-open.log 2>&1 \
  || fail "smix sim reverse failed: $(tail -2 /tmp/smix-c8-open.log)"

row="$(ledger_row)"
[ "$row" = "$DEVICE_PORT->$HOST_PORT" ] \
  || fail "the ledger does not hold the open route (read '$row', wanted '$DEVICE_PORT->$HOST_PORT')"
log "ledger=recorded          (reversePort $row on $SERIAL)"

after="$(device_reads)"
case "$after" in
  *"$TOKEN"*) log "after=reachable          (the device read this run's token)" ;;
  *) fail "with the route open the device dialling $DEVICE_PORT does not reach the stub on $HOST_PORT: $(echo "$after" | tr -d '\r' | head -1)" ;;
esac

"$SMIX" sim reverse "$SERIAL" "$DEVICE_PORT" --remove >/tmp/smix-c8-close.log 2>&1 \
  || fail "smix sim reverse --remove failed: $(tail -2 /tmp/smix-c8-close.log)"

row="$(ledger_row)"
[ "$row" = "none" ] || fail "the ledger still holds a route that was closed: $row"
log "ledger=cleared           (nothing open on $SERIAL)"

removed="$(device_reads)"
case "$removed" in
  *"$TOKEN"*) fail "the device still reaches $DEVICE_PORT after --remove — the route did not close" ;;
  *) log "removed=unreachable      ($(echo "$removed" | tr -d '\r' | head -1))" ;;
esac

log "C8-REVERSE-E2E-PASS on $SERIAL (a route opened, carried this run's token, and closed)"
