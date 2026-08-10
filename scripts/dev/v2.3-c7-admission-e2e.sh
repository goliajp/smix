#!/usr/bin/env bash
# v2.3-C7 admission e2e: a destructive command, refused while somebody
# else is using the device.
#
# C6 gave a killed session a graceful path at the next startup. This is
# the other half: stopping the collision before it happens. A live runner
# on a device now makes `smix sim uninstall` and `smix sim keychain-reset`
# refuse — by name, with the holder's pid — instead of taking the app's
# data out from under a running test.
#
# The two commands under test are the ones classed Destructive that a
# person can trigger straight from a shell. `uninstall` is run against a
# bundle id that is not installed, so what is judged is the gate, not the
# uninstall: a refusal must happen before the device is touched at all.
#
# Device work pins an explicit UDID throughout.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
ALIAS="${SMIX_E2E_DEVICE:-sim-smix-02}"
BUNDLE="com.apple.Preferences"
ABSENT_BUNDLE="jp.golia.smix.not-installed"

log()  { printf '[c7-admission] %s\n' "$*"; }
step() { printf '[c7-admission] --- %s\n' "$*"; }
fail() { printf '[c7-admission] FAIL: %s\n' "$*" >&2; exit 1; }

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
  echo "C7-ADMISSION-SKIP"
  exit 0
fi
# The default runner port is shared with every other smix session on this
# machine, so a busy port is not this test's failure to report.
if curl -s -m 2 "http://127.0.0.1:${SMIX_RUNNER_PORT:-22087}/health" >/dev/null 2>&1; then
  log "port ${SMIX_RUNNER_PORT:-22087} already answers /health — another session is using it"
  log "re-run with SMIX_RUNNER_PORT=<free port>"
  echo "C7-ADMISSION-SKIP"
  exit 0
fi


cleanup() {
  "$SMIX" runner down >/dev/null 2>&1 || true
  "$SMIX" lease reconcile "$UDID" >/dev/null 2>&1 || true
  # Through smix, not `xcrun simctl` — a shutdown that goes around
  # smix leaves the boot row behind, pointing at a device that is
  # already off. Our own scripts should not be the example of the
  # thing this whole mechanism is about.
  if [ "$WAS_BOOTED" != "yes" ]; then
    "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

step "1. a free device lets a destructive command through"
"$SMIX" sim boot "$UDID" >/dev/null 2>&1 || fail "sim boot failed"
OUT="$("$SMIX" sim keychain-reset "$UDID" 2>&1)" \
  || fail "keychain-reset refused on a free device: $OUT"
echo "$OUT" | grep -q "keychain reset" || fail "unexpected output: $OUT"
log "free device: allowed"

step "2. the lease is given back, not held onto"
# A command that kept the lease would make every later command fail, and
# the failure would look like contention rather than like a leak.
OUT="$("$SMIX" sim keychain-reset "$UDID" 2>&1)" \
  || fail "second keychain-reset refused — the first one never released: $OUT"
log "lease released after the command"

step "3. bring a session up, so the device is genuinely in use"
"$SMIX" runner up "$UDID" --bundle "$BUNDLE" >/dev/null 2>&1 || fail "runner up failed"
LEDGER=".smix/leases/$UDID.json"
[ -f "$LEDGER" ] || fail "no ledger — runner up did not record the session"

step "4. the runner up's own lease is adopted, not a wall"
# `runner up` exits by design; the runner it leaves is a service the
# next command drives through. This step asserted the opposite until
# C21 — refusal — which barred the quickstart's own `runner up` → `run`
# pairing on every device kind, and was found the first time a full
# flow ran against a physical phone. The old worry (do not tear down a
# working runner) is answered by step 5, not by locking everyone out.
OUT="$("$SMIX" sim keychain-reset "$UDID" 2>&1)" \
  || fail "keychain-reset was refused after runner up — the lease was not adopted: $OUT"
log "adopted: a later command proceeds through the runner's lease"

step "4b. a LIVE holder still refuses everything, and names itself"
# The real concurrency case. The e2e itself plays the live holder: its
# own shell pid and true start time go into the ledger, so the identity
# probe (pid + lstart) passes against a genuinely running process. This
# is constructing the fact under test, not bypassing the product path —
# nothing but a second live process can make this fact true.
LEDGER_BAK="$(cat "$LEDGER")"
STARTED="$(ps -o lstart= -p $$ | sed 's/^ *//;s/ *$//')"
python3 - "$LEDGER" "$$" "$STARTED" <<'PYEOF'
import json, sys
path, pid, started = sys.argv[1], int(sys.argv[2]), sys.argv[3]
lease = json.load(open(path))
lease["holder"] = {"pid": pid, "startedAt": started, "cmd": "c7-admission-e2e (the live holder)"}
json.dump(lease, open(path, "w"))
PYEOF

for attempt in "sim keychain-reset $UDID" "sim uninstall $UDID $ABSENT_BUNDLE"; do
  set +e
  # shellcheck disable=SC2086
  OUT="$("$SMIX" $attempt 2>&1)"
  RC=$?
  set -e
  [ "$RC" -ne 0 ] || fail "'smix $attempt' proceeded past a live holder"
  echo "$OUT" | grep -q "in use by pid $$" \
    || fail "refusal does not name the live holder: $OUT"
done
# A run is the longest thing the CLI does to a device, and the flow path
# here does not exist: if the refusal names the missing file instead of
# the holder, the gate is running too late to be a gate.
set +e
OUT="$("$SMIX" run /nonexistent/flow.yaml --device "$UDID" 2>&1)"
RC=$?
set -e
[ "$RC" -ne 0 ] || fail "smix run proceeded past a live holder"
echo "$OUT" | grep -q "in use by pid $$" \
  || fail "run refusal does not name the live holder: $OUT"
echo "$OUT" | grep -qi "no such file\|not found" \
  && fail "the flow was read before the gate ran"
printf '%s' "$LEDGER_BAK" > "$LEDGER"
log "live holder refused all three, by pid; ledger restored"

step "5. the live session is untouched by any of it"
curl -s -m 3 "http://127.0.0.1:${SMIX_RUNNER_PORT:-22087}/health" >/dev/null 2>&1 \
  || fail "the runner stopped answering — a refused command still hit the device"
log "session still healthy"

step "6. once the session ends, the device is free again"
"$SMIX" runner down >/dev/null 2>&1 || fail "runner down failed"
OUT="$("$SMIX" sim keychain-reset "$UDID" 2>&1)" \
  || fail "still refused after the session ended: $OUT"
log "allowed again"

step "7. a run that takes the device gives it back, even when it fails"
# The lease is released on scope exit, so a run that dies on a missing
# flow must not leave the device marked as held. If it did, the failure
# mode would be a device nobody can use until someone runs `lease
# reconcile` — a worse outcome than the error the run already reported.
set +e
"$SMIX" run /nonexistent/flow.yaml --device "$UDID" >/dev/null 2>&1
set -e
OUT="$("$SMIX" lease status "$UDID" 2>/dev/null | grep "^$UDID:")"
echo "$OUT" | grep -q "free" || fail "a failed run left the device held: $OUT"
log "failed run released the device"

echo "C7-ADMISSION-PASS"
