#!/usr/bin/env bash
# v2.13-C3 standalone loop e2e: everything smix does for someone on their
# own, with no Claude Code anywhere in it.
#
# Install an app → register a device → bring the runner up → drive → turn
# that session into a flow → run the flow → tear down. Each step is judged
# by an exit code or a parsed field, and the working directory is a fresh
# temp dir, not this repo, so nothing already in `.smix` can carry a step.
#
# The app under test is the repo's own fixture, built by
# build-fixture-app.sh. Pointing this at Apple's Settings instead would
# skip `sim install` entirely, and installing an app is the first thing
# anyone does.
#
# Honest boundary: on iOS the authoring step is `smix authoring record`,
# which samples the tree and writes a flow of assertions. Recording the
# actions themselves (`authoring tap-record`) is Android today — the iOS
# live-capture-to-generate leg is deferred, and this script does not
# pretend otherwise.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/release/smix}"
APP="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
BUNDLE="jp.golia.smix.fixture"
ALIAS="dev"

log()  { printf '[c3-standalone] %s\n' "$*"; }
step() { printf '[c3-standalone] --- %s\n' "$*"; }
fail() { printf '[c3-standalone] FAIL: %s\n' "$*" >&2; exit 1; }

# A precondition this script detects and cannot satisfy is a SKIP with
# what to do about it — not a FAIL. Yielding to somebody else's batch, or
# an unset target, says nothing about whether smix works, and FAIL says it
# does not to whoever reads the suite next.
skip() { printf '[c3-standalone] %s\n' "$*" >&2; printf '%s\n' "C3-STANDALONE-LOOP-SKIP"; exit 0; }


[ -x "$SMIX" ] || fail "smix binary missing: $SMIX (cargo build -p smix-cli --release)"

log "guard: no batch owner on this machine (yield, never seize)"
pgrep -f 'runner.ts|smix run|supervise' >/dev/null && skip "batch owner active — yielding"

UDID="${SMIX_C3_SIM:-}"
if [ -z "$UDID" ]; then
  UDID="$(bash "$ROOT/scripts/dev/pick-dev-sim.sh")" || skip "set SMIX_C3_SIM to a UDID"
fi
log "device: $UDID"

WORK="$(mktemp -d)"
cleanup() {
  step "teardown"
  ( cd "$WORK" && "$SMIX" down >/dev/null 2>&1 ) || true
  "$SMIX" runner down >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

# --- 0. the app to drive ------------------------------------------------

step "build the fixture app"
bash "$ROOT/scripts/dev/build-fixture-app.sh" >"$WORK/fixture.log" 2>&1 \
  || { tail -5 "$WORK/fixture.log" >&2; fail "fixture build failed"; }
[ -d "$APP" ] || fail "fixture app not produced at $APP"

# Everything from here runs in an empty directory: a new user's project,
# not this repo.
cd "$WORK"

# --- 1. register a device -----------------------------------------------

# One command for what someone arrives with: a device and an app. Running
# them as two steps is what surfaced the gap this closes — a freshly
# registered device is shut down, and `simctl install` refuses that with a
# CoreSimulator error code and no mention of booting.
step "smix init --app"
"$SMIX" init --device "$UDID" --alias "$ALIAS" --app "$APP" >"$WORK/init.log" 2>&1 \
  || { cat "$WORK/init.log" >&2; fail "init failed"; }
[ -d "$WORK/.smix" ] || fail "init did not create .smix in the working directory"
grep -q "installed .*$BUNDLE" "$WORK/init.log" \
  || { cat "$WORK/init.log" >&2; fail "init did not install the app"; }
log "registry created, app installed"

# init read the bundle id out of the .app, so the next command it prints
# is runnable as-is rather than carrying a placeholder.
grep -q -- "--bundle $BUNDLE" "$WORK/init.log" \
  || { cat "$WORK/init.log" >&2; fail "init's next command still has a placeholder bundle id"; }

# --- 2. where doctor says to go next ------------------------------------

# doctor is the thread between steps: after init it should be pointing at
# capsule up, which is what a person reads to know where they are.
NEXT="$("$SMIX" doctor --json 2>/dev/null | python3 -c 'import json,sys
d = json.load(sys.stdin)
print((d.get("next") or {}).get("command", ""))')"
case "$NEXT" in
  "smix capsule up $ALIAS"*) log "doctor points at: $NEXT" ;;
  *) fail "doctor should point at capsule up after init; said: ${NEXT:-<ready>}" ;;
esac

# --- 3. bring the runner up, by running what doctor said ----------------

# Not a command written here that happens to resemble it. doctor's promise
# is that its `next` is the thing to run, and the only way to hold it to
# that is to run it — a hand-written equivalent would keep passing on a
# machine where the suggestion had gone wrong.
step "run doctor's next command verbatim"
RUN_NEXT="${NEXT/<your.bundle.id>/$BUNDLE}"
RUN_NEXT="${RUN_NEXT/#smix/$SMIX}"
log "\$ $RUN_NEXT"
eval "$RUN_NEXT --soft" >"$WORK/capsule.log" 2>&1 \
  || { tail -15 "$WORK/capsule.log" >&2; fail "doctor's next command failed: $RUN_NEXT"; }

READY="$("$SMIX" doctor --json 2>/dev/null | python3 -c 'import json,sys
print(json.load(sys.stdin)["ready"])')"
[ "$READY" = "True" ] || fail "doctor still not ready after capsule up"
log "doctor: ready"

# --- 4. drive it --------------------------------------------------------

step "drive the app"
for id in fixture-input fixture-submit fixture-result; do
  OUT="$("$SMIX" find "id:$id" 2>/dev/null | tail -1)"
  [ "$OUT" = "exists=true" ] || fail "selector id:$id not found (got: $OUT)"
done

"$SMIX" fill "id:fixture-input" --text "hello smix" >/dev/null 2>&1 \
  || fail "fill failed"
"$SMIX" tap "id:fixture-submit" >/dev/null 2>&1 \
  || fail "tap failed"

# The result label only carries the typed text once Submit was pressed, so
# this distinguishes a tap that landed from one that merely happened.
OUT="$("$SMIX" find "text:hello smix" 2>/dev/null | tail -1)"
[ "$OUT" = "exists=true" ] || fail "the app did not react to the tap (got: $OUT)"
log "fill + tap confirmed by the app's own state"

# --- 5. turn the session into a flow ------------------------------------

# Put the screen in the state worth recording. Typing raised the
# keyboard, and a recording taken with it up marks the keyboard's own
# identifiers as stable — they are, for as long as it is showing, and
# they are gone by the time the flow is replayed from a fresh launch.
"$SMIX" hide-keyboard >/dev/null 2>&1 || fail "hide-keyboard failed"

step "smix authoring record"
"$SMIX" authoring record "$WORK/recorded.yaml" --duration-secs 3 --interval-ms 500 \
  --app-id "$BUNDLE" \
  >"$WORK/record.log" 2>&1 \
  || { tail -10 "$WORK/record.log" >&2; fail "authoring record failed"; }
[ -s "$WORK/recorded.yaml" ] || fail "recording produced an empty flow"
grep -q "fixture-" "$WORK/recorded.yaml" \
  || fail "the recorded flow names none of the app's identifiers"
log "recorded $(grep -c 'assertVisible' "$WORK/recorded.yaml") assertions"

# --- 6. run what was recorded -------------------------------------------

step "smix run the recorded flow"
"$SMIX" run "$WORK/recorded.yaml" --device "$ALIAS" >"$WORK/run.log" 2>&1 \
  || { tail -15 "$WORK/run.log" >&2; fail "the recorded flow did not run"; }
log "recorded flow ran green"

# --- 7. tear down -------------------------------------------------------

step "smix down"
"$SMIX" down >"$WORK/down.log" 2>&1 || { tail -10 "$WORK/down.log" >&2; fail "down failed"; }
pgrep -f "xcodebuild.*SmixRunner" >/dev/null && fail "a runner survived teardown"

log "C3-STANDALONE-PASS"
