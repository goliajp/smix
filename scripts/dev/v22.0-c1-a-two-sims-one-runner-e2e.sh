#!/usr/bin/env bash
# v22.0-C1: with two simulators running, a command that names one never
# reads or touches the other.
#
# A consumer ran two iOS simulators at once and found flows failing on a
# screen that looked right: what smix read, and where it tapped, belonged
# to the other simulator. `smix run` already refused a port that reaches
# another device than the one it was told; the one-shot verbs (`tree`,
# `find`, `tap`, `describe`) built their runner clients bare and read
# whichever device held the port.
#
#   1. a runner on simulator A; text typed and submitted on A
#   2. each one-shot verb naming simulator B on A's port is refused, and
#      the refusal names both devices — before anything is sent
#   3. the same verbs naming A read and act on A: the text is there
#   4. B was never touched: its screen still says "nothing submitted"
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
A="$E2E_IOS"
B="$E2E_IOS_SECOND"
APP="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
WORK="$(mktemp -d)"

log()  { printf '[v22.0-c1-a] %s\n' "$*" >&2; }
fail() { printf '[v22.0-c1-a] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v22.0-c1-a] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID_A="" UDID_B="" UP=0 WE_BOOTED_A=0 WE_BOOTED_B=0
cleanup() {
  local said
  if [ "$UP" = 1 ] && ! said="$("$SMIX" runner down --device "$UDID_A" --runner-port "$PORT" 2>&1)"; then
    log "warning: the runner on $PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  if [ "$WE_BOOTED_A" = 1 ]; then "$SMIX" sim shutdown "$UDID_A" >/dev/null 2>&1 || true; fi
  if [ "$WE_BOOTED_B" = 1 ]; then "$SMIX" sim shutdown "$UDID_B" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

resolve() { "$SMIX" sim resolve "$1" 2>/dev/null | tail -1 | tr -d '[:space:]'; }
UDID_A="$(resolve "$A")" || true
UDID_B="$(resolve "$B")" || true
[ -n "$UDID_A" ] && [ -n "$UDID_B" ] || cannot_judge "two simulators are needed: '$A' and '$B'"
[ "$UDID_A" != "$UDID_B" ] || cannot_judge "'$A' and '$B' are the same simulator"
for pair in "A $UDID_A" "B $UDID_B"; do
  set -- $pair
  case "$(simulator_name "$2")" in
    sim-smix-*) ;;
    *) cannot_judge "$1 ($2) is not one of smix's own simulators" ;;
  esac
done
if [ "$(simulator_state "$UDID_A")" != Booted ]; then
  with_deadline 900 "$SMIX" sim boot "$UDID_A" --wait 600 >"$WORK/boot-a.log" 2>&1 || fail "boot A: $(tail -3 "$WORK/boot-a.log")"
  WE_BOOTED_A=1
fi
if [ "$(simulator_state "$UDID_B")" != Booted ]; then
  with_deadline 900 "$SMIX" sim boot "$UDID_B" --wait 600 >"$WORK/boot-b.log" 2>&1 || fail "boot B: $(tail -3 "$WORK/boot-b.log")"
  WE_BOOTED_B=1
fi
for u in "$UDID_A" "$UDID_B"; do
  "$SMIX" sim install "$u" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install on $u: $(tail -3 "$WORK/install.log")"
done
xcrun simctl terminate "$UDID_B" "$APP" >/dev/null 2>&1 || true
xcrun simctl launch "$UDID_B" "$APP" >/dev/null 2>&1 || fail "could not launch the fixture on B"

log "1. a runner on A, text submitted on A"
"$SMIX" runner up "$UDID_A" --bundle "$APP" --runner-port "$PORT" >"$WORK/up.log" 2>&1 \
  || fail "runner up: $(tail -5 "$WORK/up.log")"
UP=1
printf 'appId: %s\n---\n- launchApp: { clearState: true }\n- tapOn: { id: fixture-input }\n- inputText: "only-on-A"\n- tapOn: { id: fixture-submit }\n- assertVisible: { id: fixture-result, text: "only-on-A" }\n' "$APP" >"$WORK/a.yaml"
"$SMIX_RUN" --device "$UDID_A" --runner-port "$PORT" "$WORK/a.yaml" >"$WORK/a.log" 2>&1 \
  || { tail -10 "$WORK/a.log" >&2; fail "1: the flow on A failed"; }

log "2. each one-shot verb naming B on A's port is refused"
n=0
# B is not registered here, so the verbs that pick a driver by platform are told it; `describe` is iOS-only and takes none
for verb in "tree --platform ios" "find id:fixture-result --platform ios" "tap id:fixture-submit --platform ios" "describe"; do
  # shellcheck disable=SC2086
  if "$SMIX" $verb --device "$UDID_B" --runner-port "$PORT" >"$WORK/v.log" 2>&1; then
    fail "2: \`smix $verb --device B\` on A's port answered: $(head -3 "$WORK/v.log")"
  fi
  grep -q "reaches $UDID_A" "$WORK/v.log" && grep -q "named $UDID_B" "$WORK/v.log" \
    || fail "2: \`smix $verb --device B\` failed without naming both devices: $(head -3 "$WORK/v.log")"
  n=$((n + 1))
done
[ "$n" = 4 ] || fail "2: $n of 4 verbs were judged"

log "3. the same verbs naming A read A"
"$SMIX" tree --device "$UDID_A" --runner-port "$PORT" >"$WORK/tree-a.log" 2>&1 || fail "3: tree on A: $(head -3 "$WORK/tree-a.log")"
grep -q 'label="only-on-A"' "$WORK/tree-a.log" || fail "3: A's tree does not carry the text submitted on A"
"$SMIX" find id:fixture-result --device "$UDID_A" --runner-port "$PORT" >"$WORK/find-a.log" 2>&1 \
  || fail "3: find on A: $(head -3 "$WORK/find-a.log")"

log "4. B was never touched"
xcrun simctl io "$UDID_B" screenshot "$WORK/b.png" >/dev/null 2>&1 || cannot_judge "no screenshot of B"
# B has no runner; its own screen is read with a runner of its own, on another port
gate_free_port PORT_B
"$SMIX" runner up "$UDID_B" --bundle "$APP" --runner-port "$PORT_B" >"$WORK/up-b.log" 2>&1 \
  || fail "4: runner up on B: $(tail -5 "$WORK/up-b.log")"
b_tree="$("$SMIX" tree --device "$UDID_B" --runner-port "$PORT_B" 2>&1 || true)"
if ! said_b="$("$SMIX" runner down --device "$UDID_B" --runner-port "$PORT_B" 2>&1)"; then
  log "warning: the runner on $PORT_B was not stopped:"
  printf '%s\n' "$said_b" | tail -3 >&2
fi
printf '%s\n' "$b_tree" | grep -q 'label="nothing submitted"' \
  || fail "4: B's result reads otherwise: $(printf '%s\n' "$b_tree" | grep -m1 'fixture-result')"

log "V22.0-C1-A-TWO-SIMS-ONE-RUNNER-PASS"
