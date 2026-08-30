#!/usr/bin/env bash
# v5.1-C11: a tap into a space the touch is not read in must refuse.
#
# The unit tests pin the decision; this pins that the decision is
# actually consulted on the way to a real device, and that it does not
# fire where nothing is wrong. Both halves matter equally — a check that
# refuses correct work is worse than the silence it replaced, and this
# one sits in front of every tap.
#
# Since the delivery was repaired, landscape taps land, so the refusal
# has to be provoked to be checked at all: the runner is brought up once
# with `SMIX_EVENT_STAMP=legacyAlwaysPortrait`, which reproduces the
# uncompensated delivery, and once with the default. A guard nothing can
# trigger is a guard nobody can trust, and this is the one reachable way
# to trigger it.
#
# Under the legacy delivery: exit non-zero, and the message carries both
# frames so the reader can see which two spaces disagree rather than
# being told that they do.
# Under the default: exit zero, in both orientations.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
UDID="${SMIX_C11_E2E_UDID:-}"
. "$ROOT/scripts/lib/gate-port.sh"
PORT="${SMIX_C11_E2E_PORT:-$SMIX_RUNNER_PORT}"
BUNDLE="$(grep -m1 '^BUNDLE_ID=' "$ROOT/scripts/dev/build-fixture-app.sh" | cut -d'"' -f2)"
WORK="$(mktemp -d)"

log()  { printf '[c11] %s\n' "$*" >&2; }
step() { printf '[c11] --- %s\n' "$*" >&2; }
fail() { printf '[c11] FAIL: %s\n' "$*" >&2; exit 1; }
skip() { printf '[c11] SKIP: %s\n' "$*" >&2; exit 0; }

started_runner=0
cleanup() {
  if [ "$started_runner" = 1 ]; then
    if ! "$SMIX" runner down --runner-port "$PORT" >"$WORK/down.log" 2>&1; then
      printf '[c11] the runner on %s was not stopped:\n' "$PORT" >&2
      tail -3 "$WORK/down.log" >&2
    fi
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
command -v xcrun >/dev/null 2>&1 || skip "no xcrun — this needs a simulator"

if [ -z "$UDID" ]; then
  UDID="$(bash "$ROOT/scripts/dev/pick-dev-sim.sh" 2>"$WORK/pick.log")" || {
    cat "$WORK/pick.log" >&2
    skip "no dev sim this machine's ledger says smix booted"
  }
fi
log "device $UDID, runner port $PORT"

step "1. build and install the fixture"
bash "$ROOT/scripts/dev/build-fixture-app.sh" > "$WORK/build.log" 2>&1 \
  || { tail -20 "$WORK/build.log"; fail "the fixture app did not build"; }
"$SMIX" sim boot "$UDID" >/dev/null 2>&1 || fail "could not boot $UDID"
"$SMIX" sim install "$UDID" "$ROOT/test-fixtures/demo-app/build/SmixFixture.app" >/dev/null 2>&1 \
  || fail "could not install the fixture"

step "2. runner up"
"$SMIX" runner up "$UDID" --bundle "$BUNDLE" --runner-port "$PORT" > "$WORK/up.log" 2>&1 \
  || { tail -20 "$WORK/up.log"; fail "runner did not come up"; }
started_runner=1

# The route this whole checkpoint reads. A runner too old to serve it
# makes every tap below proceed, and the landscape half would then fail
# for a reason that has nothing to do with the decision under test.
if ! curl -fsS -o "$WORK/space.json" \
     "http://127.0.0.1:$PORT/coordinate-space?nx=0.5&ny=0.5" 2>/dev/null; then
  skip "this runner does not serve /coordinate-space — rebuild the sources tarball and the CLI"
fi

step "3. portrait: the check must not fire"
"$SMIX" tap "id:portrait-enter" --port "$PORT" >/dev/null 2>&1 \
  || fail "could not reach the portrait counter screen"
"$SMIX" wait-for "id:portrait-counter" --port "$PORT" --timeout 10 >/dev/null 2>&1 \
  || fail "the portrait counter screen did not come up"
if ! "$SMIX" tap "id:portrait-increment" --port "$PORT" > "$WORK/portrait.log" 2>&1; then
  cat "$WORK/portrait.log" >&2
  fail "a portrait tap was refused — the check fires where nothing is wrong"
fi
grep -q "aimed inside" "$WORK/portrait.log" \
  || fail "the portrait tap did not report what it verified: $(cat "$WORK/portrait.log")"
log "portrait taps proceed"
"$SMIX" tap "id:portrait-exit" --port "$PORT" >/dev/null 2>&1 || true

step "4. landscape under the repaired delivery: taps land"
"$SMIX" tap "id:landscape-enter" --port "$PORT" >/dev/null 2>&1 \
  || fail "could not reach the landscape screen"
"$SMIX" wait-for "id:landscape-counter" --port "$PORT" --timeout 10 >/dev/null 2>&1 \
  || fail "the landscape screen did not come up"
"$SMIX" tap "id:landscape-increment" --port "$PORT" > "$WORK/landed.log" 2>&1 \
  || { cat "$WORK/landed.log" >&2; fail "a landscape tap was refused under the repaired delivery"; }
log "landscape taps proceed"

step "5. the guard still fires when the delivery is not compensated"
# Mid-test, before bringing the runner back up under a different stamp
# strategy. Silenced, this hid a `down` that had not worked and the
# `up` below would then be talking to the old one.
if ! down_said="$("$SMIX" runner down --runner-port "$PORT" 2>&1)"; then
  printf '[c11] warning: the runner on %s was not stopped:\n%s\n' "$PORT" "$(printf '%s' "$down_said" | tail -3)" >&2
fi
TEST_RUNNER_SMIX_EVENT_STAMP=legacyAlwaysPortrait "$SMIX" runner up "$UDID" \
  --bundle "$BUNDLE" --runner-port "$PORT" > "$WORK/up-legacy.log" 2>&1 \
  || { tail -20 "$WORK/up-legacy.log"; fail "runner did not come up under the legacy delivery"; }

active="$(curl -fsS "http://127.0.0.1:$PORT/coordinate-space?nx=0.5&ny=0.5" 2>/dev/null \
  | python3 -c "import json,sys; print(json.load(sys.stdin).get('stampStrategy','<absent>'))")"
[ "$active" = "legacyAlwaysPortrait" ] \
  || fail "asked the runner for the legacy delivery and it reports $active — this
step would pass or fail for a reason other than the one it names"

"$SMIX" tap "id:landscape-enter" --port "$PORT" >/dev/null 2>&1 \
  || fail "could not reach the landscape screen under the legacy delivery"
"$SMIX" wait-for "id:landscape-counter" --port "$PORT" --timeout 10 >/dev/null 2>&1 \
  || fail "the landscape screen did not come up under the legacy delivery"

if "$SMIX" tap "id:landscape-increment" --port "$PORT" > "$WORK/landscape.log" 2>&1; then
  cat "$WORK/landscape.log" >&2
  fail "an uncompensated landscape tap reported success — the guard did not fire"
fi

for needle in "874" "402" "COORDINATE_SPACE_MISMATCH"; do
  grep -q "$needle" "$WORK/landscape.log" \
    || fail "the refusal does not carry $needle — without it the reader cannot see which spaces disagree:
$(cat "$WORK/landscape.log")"
done
grep -qi "selector" "$WORK/landscape.log" \
  || fail "the refusal does not tell the reader their selector is not the problem"

log "landscape taps refuse, naming both spaces"
log "both directions hold"
