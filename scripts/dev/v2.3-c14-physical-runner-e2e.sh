#!/usr/bin/env bash
# v2.3-C14 physical runner e2e: one command brings a phone up.
#
# Everything below the device — team resolution, the destination fork,
# forwarding, ledger ordering — is proven by unit tests that run on any
# machine. What only a phone can answer is whether those pieces, put
# together, produce a runner that answers /health through the forwarder.
#
# No device → C14-PHYSICAL-RUNNER-SKIP, saying what was missing.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
ALIAS="${SMIX_PHYSICAL_ALIAS:-phone}"
BUNDLE="${SMIX_PHYSICAL_BUNDLE:-com.apple.Preferences}"
# A port of this gate's own, so a bystander runner cannot turn it red.
. "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
OUT="$(mktemp)"

log()  { printf '[c14-phys] %s\n' "$*"; }
step() { printf '[c14-phys] --- %s\n' "$*"; }
fail() { printf '[c14-phys] FAIL: %s\n' "$*" >&2; exit 1; }

cleanup() {
  # Not silenced. A teardown that fails leaves a runner on this device,
  # and an Android device has only one -- so the next thing to start one
  # there gets two instrumentations, or two xcodebuild sessions on one
  # sim, and every failure after that is about the wrong thing. What it
  # cost when it was silent, measured 2026-08-29: 23 of 26 corpus flows.
  if ! down_said="$("$SMIX" runner down 2>&1)"; then
    printf 'warning: the runner was not stopped:\n%s\n' "$(printf '%s' "$down_said" | tail -3)" >&2
  fi
  rm -f "$OUT"
}
trap cleanup EXIT
cd "$ROOT"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX"

step "0. the pieces, proven without a device"
cargo test -p smix-capsule signing > "$OUT" 2>&1 || { tail -10 "$OUT"; fail "signing tests failed"; }
log "$(grep -oE 'test result: ok\. [0-9]+ passed' "$OUT" | head -1) (team discovery)"
cargo test -p smix-capsule target_tests > "$OUT" 2>&1 || { tail -10 "$OUT"; fail "destination tests failed"; }
log "$(grep -oE 'test result: ok\. [0-9]+ passed' "$OUT" | head -1) (destination fork)"

step "1. is a physical device attached and registered?"
UDID="$(cargo run -q -p smix-usbmux --example first_device 2>/dev/null || true)"
if [ -z "$UDID" ]; then
  log "no iOS device on usbmux — the transport smix drives through sees nothing"
  log "attach one over USB, register it with --kind physical-ios, and re-run"
  echo "C14-PHYSICAL-RUNNER-SKIP"
  exit 0
fi
if ! "$SMIX" sim resolve "$ALIAS" >/dev/null 2>&1; then
  log "device $UDID is attached but no alias '$ALIAS' is registered"
  log "register it: smix sim register $ALIAS --udid $UDID --kind physical-ios"
  echo "C14-PHYSICAL-RUNNER-SKIP"
  exit 0
fi
log "device $UDID registered as $ALIAS"

step "2. one command: runner up on a phone"
# A machine with development identities for more than one team has to say
# which one signs — smix refuses to pick, so the harness must be able to
# answer. Unset on a single-team machine, where nothing needs saying.
TEAM_ARGS=()
[ -n "${SMIX_PHYSICAL_TEAM:-}" ] && TEAM_ARGS=(--team "$SMIX_PHYSICAL_TEAM")
if ! "$SMIX" runner up "$ALIAS" --bundle "$BUNDLE" "${TEAM_ARGS[@]+"${TEAM_ARGS[@]}"}" > "$OUT" 2>&1; then
  # A locked phone is an unmet precondition, not a defect. It reads
  # exactly like "no device attached" — smix is fine, the device is not
  # available — and calling it a FAIL would say smix is broken to
  # whoever reads the suite next.
  if grep -q "device is locked\|destination is not ready\|never became ready" "$OUT"; then
    log "the phone is locked, so xcodebuild cannot install the runner on it"
    log "unlock it (and keep it unlocked) and re-run to turn this into a PASS"
    echo "C14-PHYSICAL-RUNNER-SKIP"
    exit 0
  fi
  tail -25 "$OUT"
  fail "runner up failed on the physical device"
fi
grep -q "port forward" "$OUT" || { cat "$OUT"; fail "no forwarder was started"; }
log "$(grep -o 'port forward.*' "$OUT" | head -1)"

step "3. /health answers through the forwarder"
curl -s -m 10 "http://127.0.0.1:$PORT/health" > "$OUT" 2>&1 \
  || fail "/health did not answer through the forward"
grep -q '"ok"' "$OUT" || { cat "$OUT"; fail "/health returned something unexpected"; }
log "health: $(head -c 120 "$OUT")"

step "4. the ledger holds both rows"
LEDGER=".smix/leases/$UDID.json"
[ -f "$LEDGER" ] || fail "no ledger for $UDID"
python3 -c "
import json,sys
kinds=[r['kind'] for r in json.load(open('$LEDGER'))['resources']]
for want in ('runner','portForward'):
    if want not in kinds: sys.exit(f'no {want} row — kinds were {kinds}')
" || fail "the physical session is not fully recorded"
log "runner + portForward both recorded"

step "5. the tree comes back, and it is this device's screen"
curl -s -m 15 "http://127.0.0.1:$PORT/tree" > "$OUT" 2>&1 || fail "/tree did not answer"
python3 -c "
import json
n=json.load(open('$OUT'))
b=n.get('bounds',{})
assert b.get('w',0)>0 and b.get('h',0)>0, f'degenerate bounds {b}'
print(f\"tree ok: {b['w']}x{b['h']}\")
" || fail "/tree returned no usable tree"
log "$(python3 -c "
import json;n=json.load(open('$OUT'));b=n['bounds'];print(f\"screen {b['w']}x{b['h']}\")")"

step "6. runner down closes both, in order"
"$SMIX" runner down > "$OUT" 2>&1 || fail "runner down failed"
[ -f "$LEDGER" ] && fail "ledger survived a clean teardown"
curl -s -m 3 "http://127.0.0.1:$PORT/health" >/dev/null 2>&1 \
  && fail "the port still answers after teardown"
# The forwarder, by process — not by the port. A forwarder whose runner
# is gone answers nothing, so the /health check above is blind to it:
# one passed this step and lived five hours, still holding the port and
# still wired to the phone.
pgrep -f "runner forward $UDID" >/dev/null \
  && fail "the forwarder outlived the teardown (pgrep -fl 'runner forward $UDID')"
log "both closed, port free, forwarder gone"

echo "C14-PHYSICAL-RUNNER-PASS"
