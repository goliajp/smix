#!/usr/bin/env bash
# v2.3-C6 lease + reconcile e2e: the hard kill, and the graceful path it
# gets at the next startup.
#
# Bring a runner up, then `kill -9` the xcodebuild that is the session —
# the shape of every teardown smix does not perform: Ctrl-C in a terminal,
# an IDE restart, a CI timeout, another agent's pkill. Nothing about that
# kill is cooperative, and before the ledger existed nothing anywhere
# remembered the session had existed at all.
#
# Then check that the next smix command finds it, says so, and closes it
# by the path the dying process never took.
#
# WHAT THIS CANNOT CLAIM
#
# The hard kill itself may produce a crash report; that already happened
# by the time reconcile runs and no ledger can undo it. What is asserted
# is narrower and is the part that is ours: **the settling produces no new
# crash report of its own**. A cleanup that hard-killed its way through
# would fail that, and hard-killing is exactly what the naive fix does.
#
# Device work pins an explicit UDID throughout — never `booted`, never a
# device name, never a global verb.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
ALIAS="${SMIX_E2E_DEVICE:-sim-smix-02}"
BUNDLE="com.apple.Preferences"
# The literal fallback here was 22087 -- the very default this gate
# exists to avoid. Ask the OS instead; SMIX_RUNNER_PORT reaches
# startup, every flow and teardown alike through clap's env.
# shellcheck source=/dev/null
. "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
REPORTS="$HOME/Library/Logs/DiagnosticReports"

log()  { printf '[c6-lease] %s\n' "$*"; }
step() { printf '[c6-lease] --- %s\n' "$*"; }
fail() { printf '[c6-lease] FAIL: %s\n' "$*" >&2; exit 1; }

ips_count() { ls -1 "$REPORTS" 2>/dev/null | grep -c '\.ips$' || true; }

cd "$ROOT"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli --bin smix)"

step "0. resolve the device, and refuse to run next to somebody's session"
UDID="$("$SMIX" sim list 2>/dev/null | awk -v a="$ALIAS" '$2 == a || $0 ~ a {print $1; exit}')"

# Only shut down what this script booted.
#
# `sim shutdown` in teardown reads as tidiness when the script is run
# alone. In a suite it is how one script fails the ones after it: this
# resolved the shared dev sim from an alias, drove it, and shut it down,
# and everything later needing that sim failed with "Unable to lookup in
# current state: Shutdown". Four of eight failures in the first full
# tier run passed when run on their own.
#
# `smix sim boot` on a booted device is a no-op, so booting it is not a
# way to acquire the right to shut it down — what matters is whether it
# was already up when we arrived.
WAS_BOOTED=no
xcrun simctl list devices 2>/dev/null | grep -q "$UDID.*Booted" && WAS_BOOTED=yes
[ -n "$UDID" ] || fail "alias $ALIAS is not registered — see \`smix sim list\`"
log "device $ALIAS = $UDID"
if pgrep -f "xcodebuild.*id=$UDID" >/dev/null 2>&1; then
  # SKIP, not FAIL. Somebody else driving this device is an unmet
  # precondition, exactly like "no device attached" — smix is fine, the
  # device is busy. Calling it a failure tells whoever reads the suite
  # next that the product is broken.
  log "a runner is already driving $UDID — this test would tear down someone's work"
  log "wait for it, or point this run elsewhere with SMIX_DEV_SIM_ALIAS"
  echo "C6-LEASE-RECONCILE-SKIP"
  exit 0
fi

# The `byUs` assertion below needs this script to be the one that booted
# the device, and it cannot become that by asking.
#
# It asserts that smix's ledger tells "we brought this up" apart from
# "somebody else did" — which is what a lease is for, so the assertion
# stays exactly as it is. What was missing is the precondition: run
# against a device someone else booted and the honest answer is "I
# cannot check this here", not "smix got it wrong". It failed that way
# for the first time tonight, and only because an earlier fix stopped
# other scripts shutting the device down between runs.
if [ "$WAS_BOOTED" = "yes" ]; then
  # Same reasoning as the guard above, and the same shape: an unmet
  # precondition is a SKIP.
  log "$UDID was already booted by someone else — \`byUs\` says whose boot it"
  log "was, and this script can only check that about a boot it performed"
  echo "C6-LEASE-RECONCILE-SKIP"
  exit 0
fi
# The default runner port is shared with every other smix session on this
# machine, so a busy port is not this test's failure to report.
if curl -s -m 2 "http://127.0.0.1:${SMIX_RUNNER_PORT}/health" >/dev/null 2>&1; then
  log "port ${SMIX_RUNNER_PORT} already answers /health — another session is using it"
  log "re-run with SMIX_RUNNER_PORT=<free port>"
  echo "C6-LEASE-RECONCILE-SKIP"
  exit 0
fi


cleanup() {
  "$SMIX" runner down >/dev/null 2>&1 || true
  # Through smix, not `xcrun simctl` — a shutdown that goes around
  # smix leaves the boot row behind, pointing at a device that is
  # already off. Our own scripts should not be the example of the
  # thing this whole mechanism is about.
  if [ "$WAS_BOOTED" != "yes" ]; then
    "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

step "1. boot the device and bring the runner up, both through smix"
# `smix sim boot`, not `xcrun simctl boot` — the point is that smix records
# having been the one to bring the device up, which is what later gives it
# the right to shut it down.
"$SMIX" sim boot "$UDID" >/dev/null 2>&1 || fail "sim boot failed"
"$SMIX" runner up "$UDID" --bundle "$BUNDLE" >/dev/null 2>&1 \
  || fail "runner up failed"
LEDGER=".smix/leases/$UDID.json"
[ -f "$LEDGER" ] || fail "no ledger at $LEDGER — runner up did not record the session"
log "ledger written: $LEDGER"

step "2. the ledger names the runner and the process behind it"
RUNNER_PID="$(python3 -c "
import json,sys
led = json.load(open('$LEDGER'))
rs = [r for r in led['resources'] if r['kind'] == 'runner']
if not rs: sys.exit('no runner row in the ledger')
r = rs[0]
assert r['proc']['startedAt'], 'runner row has no start time to verify against'
print(r['proc']['pid'])
")" || fail "ledger does not record a usable runner row"
log "runner pid $RUNNER_PID recorded with an identity to verify"
ps -p "$RUNNER_PID" -o command= | grep -q xcodebuild \
  || fail "recorded pid $RUNNER_PID is not the xcodebuild session"
python3 -c "
import json,sys
led = json.load(open('$LEDGER'))
b = [r for r in led['resources'] if r['kind'] == 'booted']
if not b: sys.exit('no boot row — smix does not know it brought this device up')
if not b[0]['byUs']: sys.exit('boot recorded as somebody else\'s')
" || fail "the boot was not recorded as ours"
log "boot recorded as ours — the shutdown right follows from it"

step "3. a live session is visible in status, and is not preempted"
# What this step pins is that nothing here tears the session down. The
# wording moved with C21: `runner up` exits by design, so its lease shows
# as adoptable — the runner serving, the next command taking over — not
# as "in use", which had made the quickstart's own pairing impossible.
# "free" would be the failure: it would mean the ledger lost the session.
STATUS="$("$SMIX" lease status "$UDID" 2>/dev/null | grep "^$UDID:")"
log "$STATUS"
case "$STATUS" in
  *"in use"*|*"held by"*|*"runner serving"*) ;;
  *) fail "a live runner reported as: $STATUS" ;;
esac
# Same pin, either wording: reconcile must leave the session alone. A
# denied session says "not touching it"; an adoptable one says the
# runner is serving and settles nothing. What neither may do is close it.
RECON="$("$SMIX" lease reconcile "$UDID" 2>/dev/null)"
echo "$RECON" | grep -qE "not touching it|runner serving" \
  || fail "reconcile did not leave the live session alone: $RECON"
curl -s -m 3 "http://127.0.0.1:${SMIX_RUNNER_PORT}/health" >/dev/null 2>&1 \
  || fail "reconcile closed a serving runner"
ps -p "$RUNNER_PID" >/dev/null 2>&1 \
  || fail "reconcile ended a live session — the one thing it must never do"
log "live session left alone"

step "4. hard kill — the shape of every teardown smix does not perform"
IPS_BEFORE_KILL="$(ips_count)"
kill -9 "$RUNNER_PID"
for _ in $(seq 1 40); do
  ps -p "$RUNNER_PID" >/dev/null 2>&1 || break
  sleep 0.25
done
ps -p "$RUNNER_PID" >/dev/null 2>&1 && fail "pid $RUNNER_PID survived SIGKILL"
sleep 3   # let the crash reporter finish whatever the kill provoked
IPS_AFTER_KILL="$(ips_count)"
log "crash reports: $IPS_BEFORE_KILL before the kill, $IPS_AFTER_KILL after it"

step "5. the next command finds the orphan and says why"
STATUS="$("$SMIX" lease status "$UDID" 2>/dev/null | grep "^$UDID:")"
log "$STATUS"
echo "$STATUS" | grep -q "abandoned" \
  || fail "orphaned session not recognised: $STATUS"

step "6. settle it, and account for every close"
RECON="$("$SMIX" lease reconcile "$UDID" 2>&1)" || fail "reconcile failed: $RECON"
echo "$RECON"
echo "$RECON" | grep -q "cleared the port" \
  || fail "reconcile did not clear the port the dead launcher left behind"
echo "$RECON" | grep -q "shut down" \
  || fail "reconcile did not shut down the device it booted"
echo "$RECON" | grep -q "settled, ledger cleared" \
  || fail "reconcile did not settle the ledger"

step "7. nothing is left, and the settling itself was graceful"
[ -f "$LEDGER" ] && fail "ledger survived a clean settle"
pgrep -f "xcodebuild.*id=$UDID" >/dev/null 2>&1 \
  && fail "xcodebuild still driving $UDID after settle"
curl -s -m 2 "http://127.0.0.1:$PORT/health" >/dev/null 2>&1 \
  && fail "port $PORT still answers after settle"
STATE="$(xcrun simctl list devices -j | python3 -c "
import json,sys
for rt, devs in json.load(sys.stdin)['devices'].items():
    for d in devs:
        if d['udid'] == '$UDID': print(d['state'])
")"
[ "$STATE" = "Shutdown" ] || fail "device is $STATE after settle, expected Shutdown"
log "device shut down — the boot we performed was closed too"
sleep 3
IPS_AFTER_SETTLE="$(ips_count)"
[ "$IPS_AFTER_SETTLE" -eq "$IPS_AFTER_KILL" ] \
  || fail "settling produced $((IPS_AFTER_SETTLE - IPS_AFTER_KILL)) new crash report(s) — the graceful path was not taken"
log "no new crash report from the settle itself"

step "8. a device with no ledger is free, not broken"
STATUS="$("$SMIX" lease status "$UDID" 2>/dev/null | grep "^$UDID:")"
echo "$STATUS" | grep -q "free" || fail "settled device not reported free: $STATUS"

echo "C6-LEASE-RECONCILE-PASS"
