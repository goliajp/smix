#!/usr/bin/env bash
# v5.1-C12: which repair actually lands a landscape touch?
#
# The point is computed against the app's frame and the synthesised
# event is stamped with an orientation that decides which space those
# numbers are read in. Two repairs are possible — move the stamp to the
# point, or move the point to the stamp — and the private API that
# consumes both has no header to read. So both are built, and a device
# is asked.
#
# One predicate, the same one the reproduction uses: does the counter go
# up, and do any pixels change. Three rows, one of which is today's
# behaviour and must fail, or the experiment has no control.
#
# Exactly one row may win. Zero means neither repair is right and the
# decomposition was incomplete. More than one means the predicate is too
# loose to choose with — both are failures of this script, not results,
# and both print the table.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
UDID="${SMIX_C12_E2E_UDID:-}"
. "$ROOT/scripts/lib/gate-port.sh"
PORT="${SMIX_C12_E2E_PORT:-$SMIX_RUNNER_PORT}"
BUNDLE="$(grep -m1 '^BUNDLE_ID=' "$ROOT/scripts/dev/build-fixture-app.sh" | cut -d'"' -f2)"
WORK="$(mktemp -d)"

log()  { printf '[c12] %s\n' "$*" >&2; }
step() { printf '[c12] --- %s\n' "$*" >&2; }
fail() { printf '[c12] FAIL: %s\n' "$*" >&2; exit 1; }
skip() { printf '[c12] SKIP: %s\n' "$*" >&2; exit 0; }

cleanup() {
  "$SMIX" runner down --runner-port "$PORT" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
command -v xcrun >/dev/null 2>&1 || skip "no xcrun — this needs a simulator"
python3 -c 'import PIL' 2>/dev/null || fail "Pillow is the pixel judge here"

if [ -z "$UDID" ]; then
  UDID="$(bash "$ROOT/scripts/dev/pick-dev-sim.sh" 2>"$WORK/pick.log")" || {
    cat "$WORK/pick.log" >&2
    skip "no dev sim this machine's ledger says smix booted"
  }
fi
log "device $UDID, runner port $PORT"

bash "$ROOT/scripts/dev/build-fixture-app.sh" > "$WORK/build.log" 2>&1 \
  || { tail -20 "$WORK/build.log"; fail "the fixture app did not build"; }
"$SMIX" sim boot "$UDID" >/dev/null 2>&1 || fail "could not boot $UDID"
"$SMIX" sim install "$UDID" "$ROOT/test-fixtures/demo-app/build/SmixFixture.app" >/dev/null 2>&1 \
  || fail "could not install the fixture"

read_counter() {
  "$SMIX" tree --port "$PORT" --json 2>/dev/null | grep -v '^kevy:' | python3 -c "
import json,sys
def walk(n):
    if (n.get('identifier') or '') == 'landscape-counter':
        print(n.get('label') or ''); raise SystemExit
    for c in n.get('children', []): walk(c)
t=json.load(sys.stdin); walk(t if isinstance(t,dict) else t[0])
"
}

WINNERS=""
TABLE="$WORK/table.txt"
: > "$TABLE"

for strategy in legacyAlwaysPortrait deriveFromAppFrame convertPointToDeviceSpace; do
  step "$strategy"
  "$SMIX" runner down --runner-port "$PORT" >/dev/null 2>&1 || true

  # The strategy is read by the runner process at start, so each row
  # needs its own runner. Restarting between rows also means no row
  # inherits the previous one's screen.
  TEST_RUNNER_SMIX_EVENT_STAMP="$strategy" "$SMIX" runner up "$UDID" --bundle "$BUNDLE" \
    --runner-port "$PORT" > "$WORK/up-$strategy.log" 2>&1 \
    || { tail -20 "$WORK/up-$strategy.log"; fail "runner did not come up for $strategy"; }

  # The witness. A strategy that never reached the runner and a strategy
  # that reached it and did not help produce the same row, and the first
  # run of this script printed three identical rows — which read as
  # "neither repair works" and was in fact "the environment variable
  # went nowhere". The runner now says which one it is using, and a row
  # measured under the wrong one is not a result.
  active="$(curl -fsS "http://127.0.0.1:$PORT/coordinate-space?nx=0.5&ny=0.5" 2>/dev/null \
    | python3 -c "import json,sys; print(json.load(sys.stdin).get('stampStrategy','<absent>'))")"
  [ "$active" = "$strategy" ] \
    || fail "asked for $strategy and the runner reports $active — the row would
have measured something other than what it names"

  "$SMIX" tap "id:landscape-enter" --port "$PORT" >/dev/null 2>&1 \
    || fail "could not reach the landscape screen under $strategy"
  "$SMIX" wait-for "id:landscape-counter" --port "$PORT" --timeout 10 >/dev/null 2>&1 \
    || fail "the landscape screen did not come up under $strategy"

  before="$(read_counter)"
  xcrun simctl io "$UDID" screenshot "$WORK/before-$strategy.png" >/dev/null 2>&1
  # Not `|| fail`: under the strategy that is wrong, C11's refusal is
  # the correct behaviour and a non-zero exit here is data, not an
  # error. The counter is what decides.
  "$SMIX" tap "id:landscape-increment" --port "$PORT" > "$WORK/tap-$strategy.log" 2>&1 || true
  sleep 1
  xcrun simctl io "$UDID" screenshot "$WORK/after-$strategy.png" >/dev/null 2>&1
  after="$(read_counter)"

  bbox="$(python3 -c "
from PIL import Image, ImageChops
a=Image.open('$WORK/before-$strategy.png').convert('RGB')
b=Image.open('$WORK/after-$strategy.png').convert('RGB')
print(ImageChops.difference(a,b).getbbox() or 'None')
")"

  verdict="no"
  if [ "${before:-x}" != "${after:-y}" ] && [ "$bbox" != "None" ]; then
    verdict="LANDED"
    WINNERS="$WINNERS $strategy"
  fi
  printf '  %-26s counter %-6s → %-6s  pixels %-28s %s\n' \
    "$strategy" "${before:-<none>}" "${after:-<none>}" "$bbox" "$verdict" >> "$TABLE"
done

step "results"
cat "$TABLE" >&2

count=$(printf '%s\n' $WINNERS | grep -c . || true)
if [ "$count" = 0 ]; then
  fail "no strategy landed a touch — neither repair is right, and the
decomposition is not finished. Do not pick one from the table above."
fi

# Both repairs can be right at once, and expecting exactly one was a
# mistake in the first draft of this script: they are two consistent
# ways of naming the same physical point — move the stamp to the point,
# or move the point to the stamp. If both land, that is a result, and
# the choice is made on grounds the table cannot show.
if [ "$count" -gt 1 ]; then
  log "more than one repair lands:$WINNERS"
  log "choosing deriveFromAppFrame: it keeps our coordinates in the space"
  log "the tree describes, so /coordinate-space's resolvedPoint stays a"
  log "number a reader can check, and the rotation is the system's"
  log "arithmetic rather than ours to get wrong."
  case "$WINNERS" in
    *deriveFromAppFrame*) ;;
    *) fail "deriveFromAppFrame is not among the winners, so the tie-break
above does not apply — decide deliberately rather than by this script." ;;
  esac
else
  log "the repair is:$WINNERS"
fi

# The control row is what makes the winning row mean anything.
grep -q "legacyAlwaysPortrait.*no$" "$TABLE" \
  || fail "today's behaviour landed the touch — then the defect is not
reproducing and this run proves nothing"
log "the control row still fails, as it must"
