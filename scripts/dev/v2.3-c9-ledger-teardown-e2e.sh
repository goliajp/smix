#!/usr/bin/env bash
# v2.3-C9 ledger teardown e2e: `smix down` closes what the ledger says
# is open, and says what it closed.
#
# Teardown used to be "find processes that look like mine" — a pattern
# match that fails in both directions: too broad and it kills another
# project's runner (which happened, and `runner.rs:551` carries the fix
# for that half), too narrow and it misses the same resource under a
# different command line.
#
# What is judged here is that the ledger pass runs first and accounts for
# each row by name, and that the pattern passes survive as a backstop for
# what no ledger covers rather than as the mechanism.
#
# The supervisor is the row that matters most: it exists to restart a
# runner it finds dead, so a teardown that stops the runner first watches
# it come back. The ledger orders the supervisor ahead of the runner, and
# this checks that the order survives into a real teardown.
#
# Device work pins an explicit UDID throughout.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
ALIAS="${SMIX_E2E_DEVICE:-sim-smix-02}"
BUNDLE="com.apple.Preferences"
REPORTS="$HOME/Library/Logs/DiagnosticReports"

log()  { printf '[c9-teardown] %s\n' "$*"; }
step() { printf '[c9-teardown] --- %s\n' "$*"; }
fail() { printf '[c9-teardown] FAIL: %s\n' "$*" >&2; exit 1; }

ips_count() { ls -1 "$REPORTS" 2>/dev/null | grep -c '\.ips$' || true; }

cd "$ROOT"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX"

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
[ -n "$UDID" ] || fail "alias $ALIAS is not registered"
log "device $ALIAS = $UDID"
if pgrep -f "xcodebuild.*id=$UDID" >/dev/null 2>&1; then
  # SKIP, not FAIL. Somebody else driving this device is an unmet
  # precondition, exactly like "no device attached" — smix is fine, the
  # device is busy. Calling it a failure tells whoever reads the suite
  # next that the product is broken.
  log "a runner is already driving $UDID — this test would tear down someone's work"
  log "wait for it, or point this run elsewhere with SMIX_DEV_SIM_ALIAS"
  echo "C9-LEDGER-TEARDOWN-SKIP"
  exit 0
fi
# The default runner port is shared with every other smix session on this
# machine, so a busy port is not this test's failure to report.
if curl -s -m 2 "http://127.0.0.1:${SMIX_RUNNER_PORT:-22087}/health" >/dev/null 2>&1; then
  log "port ${SMIX_RUNNER_PORT:-22087} already answers /health — another session is using it"
  log "re-run with SMIX_RUNNER_PORT=<free port>"
  echo "C9-LEDGER-TEARDOWN-SKIP"
  exit 0
fi


cleanup() {
  "$SMIX" runner down >/dev/null 2>&1 || true
  "$SMIX" lease reconcile "$UDID" >/dev/null 2>&1 || true
  if [ "$WAS_BOOTED" != "yes" ]; then
    "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

step "1. bring a supervised runner up"
"$SMIX" sim boot "$UDID" >/dev/null 2>&1 || fail "sim boot failed"
"$SMIX" runner up "$UDID" --bundle "$BUNDLE" --supervise >/dev/null 2>&1 \
  || fail "runner up --supervise failed"
LEDGER=".smix/leases/$UDID.json"
[ -f "$LEDGER" ] || fail "no ledger after runner up"

step "2. the ledger holds a runner row AND a supervisor row"
python3 -c "
import json,sys
kinds = [r['kind'] for r in json.load(open('$LEDGER'))['resources']]
for want in ('runner', 'supervisor'):
    if want not in kinds: sys.exit(f'no {want} row — kinds were {kinds}')
" || fail "the supervised session is not fully recorded"
log "runner + supervisor both recorded"

step "3. smix down settles by ledger, and says what it closed — in order"
# The order is the point. A supervisor exists to restart a runner it
# finds dead, so a teardown that stops the runner first watches it come
# back. The plan's ordering is unit-tested; what is judged here is that
# the ordering survives into a real teardown, read off the output.
IPS_BEFORE="$(ips_count)"
OUT="$("$SMIX" down 2>&1)" || true
echo "$OUT" | grep -q "device ledgers" || fail "the ledger pass did not run first: $OUT"

SUP_LINE="$(echo "$OUT" | grep -n "supervisor pid" | head -1 | cut -d: -f1)"
RUN_LINE="$(echo "$OUT" | grep -nE "runner on port [0-9]+" | head -1 | cut -d: -f1)"
[ -n "$SUP_LINE" ] || fail "teardown did not account for the supervisor by name:\n$OUT"
[ -n "$RUN_LINE" ] || fail "teardown did not account for the runner by name:\n$OUT"
[ "$SUP_LINE" -lt "$RUN_LINE" ] \
  || fail "the runner was stopped before its supervisor ($RUN_LINE vs $SUP_LINE):\n$OUT"
log "supervisor closed first, then the runner — each named"

step "4. nothing left, and the settling itself was graceful"
[ -f "$LEDGER" ] && fail "ledger survived a clean teardown"
pgrep -f "xcodebuild.*id=$UDID" >/dev/null 2>&1 \
  && fail "xcodebuild still driving $UDID"
sleep 3
IPS_AFTER="$(ips_count)"
[ "$IPS_AFTER" -eq "$IPS_BEFORE" ] \
  || fail "teardown produced $((IPS_AFTER - IPS_BEFORE)) new crash report(s)"
log "no new crash report from the teardown"

step "5. residue outside any ledger is still reported, not silently missed"
# The pattern passes are a backstop, not the mechanism — but a backstop
# that stopped reporting would make "clean" a claim nobody checked.
touch /tmp/smix-c9-fake-vite
( exec -a "smix/web/node_modules/.bin/vite --fake-for-c9" sleep 30 ) &
FAKE=$!
sleep 1
set +e
OUT="$("$SMIX" down 2>&1)"
set -e
kill "$FAKE" 2>/dev/null || true
rm -f /tmp/smix-c9-fake-vite
echo "$OUT" | grep -q "STILL RUNNING\|vite" \
  || fail "a process outside the ledger went unreported: $OUT"
log "ledger-external residue still reported"

echo "C9-LEDGER-TEARDOWN-PASS"
