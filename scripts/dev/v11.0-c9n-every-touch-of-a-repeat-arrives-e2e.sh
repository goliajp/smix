#!/usr/bin/env bash
# v11.0-C9n: on iOS, every touch of a repeated tap arrives, at the
# interval asked for, and a double tap is still one gesture.
#
# A repeated tap was one synthesised event carrying a pointer path per
# touch. Paths in one event are separate fingers, and their start
# offsets were not kept: measured 2026-09-27 on sim-smix-02 (iOS 27),
# touches asked for 1000 ms and 2000 ms apart reached the app 0.06-0.12 s
# apart, and the fixture's Button counted one fewer than were sent —
# 2 gave 1, 5 gave 4, 10 gave 9. Android counted every one.
#
# Judged by the app's own count, and the spacing by the wall clock of
# the step alone: three touches 1000 ms apart cannot take under 2 s.
# A double tap is checked too, because it is the one case that wants
# the touches close together: it must still count as one double tap.
#
# Three codes, per the contract: 0 judged and right, 1 judged and wrong
# or the setup this run owns did not come up, 2 could not judge.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-devices.sh
source "$ROOT/scripts/lib/e2e-devices.sh"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
# shellcheck source=../lib/judged-run.sh
source "$ROOT/scripts/lib/judged-run.sh"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
UDID="${1:-${SMIX_E2E_UDID:-}}"
APP="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"

log()  { printf '[c9n-repeat] %s\n' "$*" >&2; }
fail() { printf '[c9n-repeat] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c9n-repeat] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

[ -n "$UDID" ] || cannot_judge "usage: $0 <simulator udid> (or SMIX_E2E_UDID)"
[ "$(simulator_state "$UDID")" = Booted ] \
  || cannot_judge "$UDID is not a booted simulator"
[ -d "$FIXTURE" ] || fail "no fixture app — run: bash scripts/dev/build-fixture-app.sh"

WORK="$(mktemp -d)"
WE_UPPED=0
cleanup() {
  if [ "$WE_UPPED" = 1 ]; then
    local said
    if ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$PORT" 2>&1)"; then
      printf '[c9n-repeat] warning: the runner on %s:%s was not stopped:\n%s\n' \
        "$UDID" "$PORT" "$(printf '%s' "$said" | tail -3)" >&2
    fi
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

xcrun simctl install "$UDID" "$FIXTURE" || fail "could not install the fixture"
"$SMIX" runner up "$UDID" --bundle "$APP" --runner-port "$PORT" --force >/dev/null 2>&1 \
  || fail "the runner would not start on $UDID:$PORT"
WE_UPPED=1

# The number in the label of the element with this identifier.
label_count() { # label_count <identifier>
  local tree
  tree="$("$SMIX" tree --device "$UDID" --port "$PORT" --json 2>/dev/null)" || return 1
  TREE_JSON="$tree" WANT="$1" python3 - <<'PY'
import json, os, re, sys
d = json.loads(os.environ["TREE_JSON"])
def walk(n):
    if n.get("identifier") == os.environ["WANT"]:
        m = re.search(r"(\d+)", n.get("label") or n.get("text") or "")
        if m:
            print(m.group(1))
            sys.exit(0)
    for c in n.get("children") or []:
        walk(c)
walk(d.get("root") or d)
sys.exit(1)
PY
}

# A fresh app, so every count starts from zero.
fresh() {
  xcrun simctl terminate "$UDID" "$APP" >/dev/null 2>&1 || true
  "$SMIX" sim launch "$UDID" "$APP" >/dev/null 2>&1 || fail "could not launch the fixture"
  local c
  c="$(label_count fixture-icon-count)" || cannot_judge "the fixture's press count is not on the screen"
  [ "$c" = 0 ] || fail "a freshly launched fixture counts $c presses, not 0"
}

# Runs one step alone and prints how long it took, in milliseconds.
step() { # step <yaml step line>
  printf 'appId: %s\n---\n%s\n' "$APP" "$1" > "$WORK/step.yaml"
  local t0 t1
  t0="$(python3 -c 'import time; print(int(time.time()*1000))')"
  "$SMIX_RUN" "$WORK/step.yaml" --device "$UDID" --runner-port "$PORT" >"$WORK/step.out" 2>&1 \
    || fail "step '$1' failed: $(tail -3 "$WORK/step.out" | tr '\n' ' ')"
  t1="$(python3 -c 'import time; print(int(time.time()*1000))')"
  echo $((t1 - t0))
}

for n in 1 2 5 10; do
  fresh
  step "- tapOn: { label: Pause, repeat: $n, delay: 100 }" >/dev/null
  got="$(label_count fixture-icon-count)" || fail "no press count after $n touches"
  [ "$got" = "$n" ] || fail "tapOn repeat: $n reached the app as $got presses"
  log "repeat $n: the app counted $got"
done

fresh
ms="$(step "- tapOn: { label: Pause, repeat: 3, delay: 1000 }")"
got="$(label_count fixture-icon-count)" || fail "no press count after the spaced touches"
[ "$got" = 3 ] || fail "tapOn repeat: 3, delay: 1000 reached the app as $got presses"
[ "$ms" -ge 2000 ] || fail "three touches 1000 ms apart took $ms ms — the interval was not kept"
log "repeat 3, delay 1000: the app counted $got, the step took $ms ms"

fresh
step "- doubleTapOn: { id: fixture-doubletap }" >/dev/null
dt="$(label_count fixture-doubletap)" || fail "no double-tap count"
[ "$dt" = 1 ] || fail "doubleTapOn counted $dt double taps, not 1 — its touches are no longer one gesture"
log "doubleTapOn: one double tap"

log "PASS"
