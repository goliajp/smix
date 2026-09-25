#!/usr/bin/env bash
# v10's exit acceptance, as one command that prints each verdict in its own
# words.
#
# The cold plan lists four conditions. They are run here rather than
# remembered, because "I checked them all" is exactly the claim this project
# has been caught making. Any one red is red.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
# This tree's binary unless SMIX_BIN names another, for every check below
# (the Python ones ask the same resolver); the flow is judged by the code
# smix reports. It ran `./target/release/smix` while the checks beside it
# chose their own.
# shellcheck source=../lib/e2e-binary.sh
. "$ROOT/scripts/lib/e2e-binary.sh"
# Not a slot. emulator-5554 is whichever AVD booted into it first, and on
# 2026-09-24 that was a consumer's. The picker answers with an emulator
# this machine's ledger says smix booted, or refuses and says what to run.
if [[ -n "${SMIX_EXIT_ANDROID:-}" ]]; then
  ANDROID="$SMIX_EXIT_ANDROID"
else
  ANDROID="$(bash "$ROOT/scripts/dev/pick-dev-emulator.sh" 2>&1)" || {
    printf 'v10-exit: no emulator this check may drive:\n%s\n' "$ANDROID" >&2
    exit 1
  }
fi
APORT="${SMIX_EXIT_ANDROID_PORT:-22095}"
IOS="${SMIX_EXIT_IOS:-}"
# No literal: the iOS gate asks the OS for a port of its own, and an
# empty argument leaves it that choice. A number here would be one
# more thing a bystander can hold.
IPORT="${SMIX_EXIT_IOS_PORT:-}"
FAILED=0

step() {
  local name="$1"; shift
  local out
  out="$("$@" 2>&1)"
  local rc=$?
  if [ $rc -eq 0 ]; then
    printf 'v10-exit: PASS  %s\n' "$name"
    printf '%s\n' "$out" | tail -1 | sed 's/^/          /'
  else
    printf 'v10-exit: FAIL  %s\n' "$name"
    printf '%s\n' "$out" | tail -3 | sed 's/^/          /'
    FAILED=1
  fi
}

# 1 — the two perception paths reconcile, and the reconciliation is not
#     vacuous (an exact count, not "more than none").
step "two paths agree on the fixture" \
  python3 scripts/dev/two-paths-agree.py --device "$ANDROID" --port "$APORT" \
    --min-both 16 --min-bounds-compared 16 --focus compose_input

# 1b — and on the screen where Compose hosts a View, which is where the
#      claim above could fail and never did, for want of such a screen.
step "two paths agree where Compose hosts a View" \
  python3 scripts/dev/two-paths-agree.py --device "$ANDROID" --port "$APORT" \
    --activity .InteropActivity --min-both 8 --min-bounds-compared 8 \
    --prove-differences-exhibited --focus fixture_interop_input

# 2 — the three root causes of 6.4.0 each have something that goes red.
step "the three that went red" \
  python3 scripts/dev/the-three-that-went-red.py --device "$ANDROID" --port "$APORT"

# 3 — waiting does not end while the screen is still moving.
step "a wait that does not end early" \
  python3 scripts/dev/a-wait-that-does-not-end-early.py --device "$ANDROID" --port "$APORT"

# 4 — the probe stages the screen; the touch stays real.
step "a semantics action is not a touch" \
  python3 scripts/dev/a-semantics-action-is-not-a-touch.py --device "$ANDROID" --port "$APORT"

# 5 — the headline: a control inside a Compose dialog, addressed by id.
step "dialog-confirm flow" \
  "$SMIX_RUN" --device "$ANDROID" --platform android \
    --runner-port "$APORT" scripts/release/android-behaviour/dialog-confirm.yaml

# 6 — three readers of one report say the same thing.
step "three readers agree" python3 scripts/dev/three-readers-agree.py

# 7 — iOS: a tap that cannot land says so. Skipped ALOUD when no sim was
#     named: a silent skip and a pass look the same from the outside, and
#     this release spent a checkpoint on exactly that shape.
if [ -n "$IOS" ]; then
  step "a tap that cannot land says so" \
    bash scripts/dev/a-tap-that-cannot-land-says-so.sh "$IOS" "$IPORT"
  # 8 — the same shape one layer up: a request for an app that is not on
  #     the device. It refuses, and the runner is still there afterwards.
  step "a foreground that cannot happen says so" \
    bash scripts/dev/a-foreground-that-cannot-happen-says-so.sh "$IOS"
else
  printf 'v10-exit: NOT RUN  a tap that cannot land says so\n'
  printf '          set SMIX_EXIT_IOS=<udid> to include it. Until then this\n'
  printf '          run has verified Android only, and says so.\n'
  FAILED=1
fi

if [ "$FAILED" -ne 0 ]; then
  echo "v10-exit: NOT COMPLETE — see above"
  exit 1
fi
echo "v10-exit: all conditions hold on $ANDROID and $IOS"
