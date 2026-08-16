#!/usr/bin/env bash
# v2.11-C4 Android propose e2e: close the loop "real failing flow -> real bundle
# -> real local claude propose -> apply/emit -> amended flow reruns fail->pass".
# Device work — pins the emulator serial (a physical phone must never be
# touched), sweeps on exit. Real `claude` in the loop is non-deterministic;
# PASS/FAIL is made stable by a determinism precondition (typo'd selector, the
# correct value carried in the bundle's `suggestions`) + a machine assertion +
# bounded retry. Stage markers keep effectiveness distinct from well-formed.
set -euo pipefail

# A serial from the caller, or the ledger's answer — never a port
# somebody else's emulator may be sitting on.
SERIAL="${1:-}"
if [ -z "$SERIAL" ]; then
  SERIAL="$(bash "$(cd "$(dirname "$0")/../.." && pwd)/scripts/dev/pick-dev-emulator.sh")" || exit 1
fi
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# A port of this gate's own, so a bystander runner cannot turn it red.
. "$ROOT/scripts/lib/gate-port.sh"
# The port the runner is actually on, which since gate-port.sh landed is
# not 28080 unless someone says so.
#
# `runner up` reads SMIX_RUNNER_PORT from the environment, and
# gate-port.sh exports a free one so a bystander's runner cannot turn
# this red. This line kept probing 28080 regardless, so the health loop
# knocked ninety times on a door nobody was behind and reported "runner
# never healthy" — a self-inflicted break that reads exactly like the
# product failing to start.
PORT="${SMIX_ANDROID_PORT:-$SMIX_RUNNER_PORT}"

SMIX="$ROOT/target/release/smix"
R="http://localhost:$PORT"

log()  { printf '[c4-propose] %s\n' "$*"; }
fail() { printf '[c4-propose] FAIL: %s\n' "$*" >&2; exit 1; }

# A precondition this script detects and cannot satisfy is a SKIP with
# what to do about it — not a FAIL. Yielding to somebody else's batch, or
# an unset target, says nothing about whether smix works, and FAIL says it
# does not to whoever reads the suite next.
skip() { printf '[c4-propose] %s\n' "$*" >&2; printf '%s\n' "C4-ANDROID-PROPOSE-SKIP"; exit 0; }


# --- guards: emulator-only, yield to a batch owner, tools present ---
case "$SERIAL" in emulator-*) ;; *) fail "serial must be an emulator (got $SERIAL); never a physical phone" ;; esac
export ANDROID_SERIAL="$SERIAL"
pgrep -f 'runner.ts|smix run|supervise' >/dev/null && skip "batch owner active — yielding, not seizing the runner"
command -v claude >/dev/null 2>&1 || fail "claude CLI not found on PATH"
[ -x "$SMIX" ] || fail "smix not built at $SMIX"

WORK="$(mktemp -d)"
cleanup() {
  log "teardown: runner down + rm work"
  "$SMIX" runner down --platform android --device "$SERIAL" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

# --- runner up (Android takes its target per-request, no --bundle) ---
log "runner up $SERIAL"
"$SMIX" runner up "$SERIAL" --platform android >"$WORK/up.log" 2>&1 &
for i in $(seq 1 90); do
  sleep 2
  curl -sf --max-time 2 "$R/health" >/dev/null 2>&1 && break
  [ "$i" = 90 ] && { cat "$WORK/up.log"; fail "runner never healthy"; }
done

# --- put Settings home on screen ---
log "launch Settings home"
adb -s "$SERIAL" shell am force-stop dev.smix.fixture >/dev/null 2>&1 || true
adb -s "$SERIAL" shell am start -n dev.smix.fixture/.MainActivity >/dev/null 2>&1
sleep 2

# The fixture, not the system Settings app.
#
# Those resource ids belong to whatever Settings version the emulator
# runs; `search_src_text` is absent on this one, so the tap landed
# nowhere and the failure surfaced somewhere else entirely. The fixture
# ships in this repository and is built by the gate that drives it.
SEL="fixture_input"

# --- (a) baseline flow must pass (exit 0) ---
cat >"$WORK/baseline.yaml" <<YAML
appId: dev.smix.fixture
---
- assertVisible:
    id: $SEL
YAML
log "baseline flow (assertVisible id:$SEL)"
if ! "$SMIX" run --device "$SERIAL" --platform android --runner-port "$PORT" --no-launch "$WORK/baseline.yaml"; then
  fail "C4-BASELINE-FAIL: baseline selector $SEL not on the fixture — not a loop problem"
fi

# --- (b) corrupt flow must fail (exit 3, Sdk), assembling a real bundle ---
cat >"$WORK/corrupt.yaml" <<YAML
appId: dev.smix.fixture
---
- assertVisible:
    id: ${SEL}X
YAML
mkdir -p "$WORK/bundle"
log "corrupt flow (assertVisible id:${SEL}X) — expect exit 3"
set +e
"$SMIX" run --device "$SERIAL" --platform android --runner-port "$PORT" --no-launch \
  --debug-output "$WORK/bundle" --format json "$WORK/corrupt.yaml" \
  >"$WORK/bundle/failure.json" 2>"$WORK/corrupt.err"
code=$?
set -e
[ "$code" = 3 ] || fail "C4-CORRUPT-DID-NOT-FAIL: expected exit 3 (Sdk), got $code — $(cat "$WORK/corrupt.err")"

# --- (c) determinism precondition: bundle names the correct selector ---
# Match the correct selector distinct from the typo: `search_action_bar`
# followed by a non-`X` char (a suggestion carries it as `..._bar"`; the
# typo is `..._barX`, so `[^X]` excludes the corrupt selector's own echo).
log "assert bundle carries a suggestion naming $SEL"
grep -qE "${SEL}[^X]" "$WORK/bundle/failure.json" \
  || fail "C4-DETERMINISM-UNMET: failure.json carries no suggestion naming $SEL — $(cat "$WORK/bundle/failure.json")"

# --- (d) real claude propose + amend (<=3 retry), well-formed gate ---
log "real claude propose_and_amend (<=3 retry)"
ok=0
for attempt in 1 2 3; do
  if cargo run -q -p smix-authoring-propose --example propose_amend -- \
       "$WORK/corrupt.yaml" "$WORK/bundle" "$WORK/amended.yaml" 2>"$WORK/propose.$attempt.err"; then
    if "$SMIX" run --check "$WORK/amended.yaml"; then
      ok=1
      break
    fi
  fi
  log "attempt $attempt: no well-formed amend yet"
done
[ "$ok" = 1 ] || fail "C4-PROPOSE-MALFORMED: no well-formed amended flow in 3 attempts — $(cat "$WORK"/propose.*.err 2>/dev/null)"
log "amended flow:"; sed 's/^/    /' "$WORK/amended.yaml"

# --- (e) amended flow reruns on device (effectiveness) ---
log "rerun amended flow on device"
if "$SMIX" run --device "$SERIAL" --platform android --runner-port "$PORT" --no-launch "$WORK/amended.yaml"; then
  log "C4-E2E-PASS"
else
  fail "C4-WELLFORMED-ONLY: amended flow well-formed + ran to verdict but did not flip fail->pass"
fi
