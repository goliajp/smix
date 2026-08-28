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
ANDROID="${SMIX_EXIT_ANDROID:-emulator-5554}"
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
  python3 scripts/dev/two-paths-agree.py --device "$ANDROID" --port "$APORT" --min-both 16

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
  ./target/release/smix run --device "$ANDROID" --platform android \
    --runner-port "$APORT" scripts/release/android-behaviour/dialog-confirm.yaml

# 6 — three readers of one report say the same thing.
step "three readers agree" python3 scripts/dev/three-readers-agree.py

# 7 — iOS: a tap that cannot land says so. Skipped ALOUD when no sim was
#     named: a silent skip and a pass look the same from the outside, and
#     this release spent a checkpoint on exactly that shape.
if [ -n "$IOS" ]; then
  step "a tap that cannot land says so" \
    bash scripts/dev/a-tap-that-cannot-land-says-so.sh "$IOS" "$IPORT"
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
