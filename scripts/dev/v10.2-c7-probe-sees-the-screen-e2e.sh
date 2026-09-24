#!/usr/bin/env bash
# v10.2-C7: the probe's tree is the screen, not the layout.
#
# Three defects, one root, from a consumer's Android round
# (2026-09-22). The probe walked
# `SemanticsNode.children` and reported `positionOnScreen + size`, so:
#
#   1. a View inside an `AndroidView` was not in its tree at all — and
#      since a flow reads the probe's tree when there is one, adding the
#      probe to an app made the player chrome in their app unaddressable,
#      while `smix find` (which reads the accessibility path) still saw it.
#   2. a node the toolkit had measured but never placed was reported at a
#      position it had never had; their `scrollUntilVisible` stopped on one
#      and the tap after it landed on the filter chips.
#   3. a row clipped by its viewport was reported at full height.
#
# Measured here against a binary built before the fix (the same fixture
# screen, the probe stashed): `fixture_interop_button` absent,
# `interop_unplaced` present at [0,323,213,368], and all eight rows of a
# 220dp-tall scrolling column reported as if they were on screen —
# [0,263,1080,538] through [0,2188,1080,2463].
#
# There is no SKIP path. A missing emulator, a missing apk, a busy port or
# a runner that will not start are reasons this cannot judge anything, and
# a gate that cannot judge must be red rather than quiet (open-items I4).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/e2e-devices.sh"
ALIAS="${SMIX_C7_ANDROID:-$E2E_ANDROID}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"

log()  { printf '[c7-probe] %s\n' "$*" >&2; }
fail() { printf '[c7-probe] FAIL: %s\n' "$*" >&2; exit 1; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0
cleanup() {
  if [ "$WE_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
      >/dev/null 2>&1 || printf '[c7-probe] warning: the runner was not stopped\n' >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || fail "no adb on PATH — this judges an Android runner and cannot"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build --release -p smix-cli)"
[ -f "$APK" ] || fail "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
# And the one THESE sources build: the path existing says a build
# happened, not which sources it happened over (open-items O1).
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"

SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)"
[ -n "$SERIAL" ] || fail "no device registered as $ALIAS"
case "$SERIAL" in
  emulator-*) : ;;
  # This drives a fixture app and installs it. A registered phone belongs
  # to somebody; §9 #1 keeps destructive work off one, and an e2e that
  # would install onto whatever `$ALIAS` happens to name is how that gets
  # forgotten.
  *) fail "$ALIAS resolves to $SERIAL, which is not an emulator — refusing" ;;
esac

if ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  log "booting $SERIAL"
  "$SMIX" sim boot "$ALIAS" >/dev/null 2>&1 || fail "could not boot $ALIAS"
  WE_BOOTED=1
  adb -s "$SERIAL" wait-for-device
fi

adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"

if ! curl -s -m 5 "http://localhost:$PORT/health" >/dev/null 2>&1; then
  "$SMIX" runner up "$ALIAS" --platform android --runner-port "$PORT" >/dev/null 2>&1 \
    || fail "the runner would not start on $SERIAL:$PORT"
  WE_UPPED=1
fi

adb -s "$SERIAL" shell am force-stop "$APPID" >/dev/null 2>&1 || true
adb -s "$SERIAL" shell am start -n "$APPID/.InteropActivity" >/dev/null 2>&1 \
  || fail "could not start the interop screen"

# Each reader asked by name, through the CLI a consumer has.
#
# Both go through `smix tree` since I1 was closed: the probe is no
# longer reachable only by curl, and — the half that matters here — the
# accessibility tree is no longer what a plain `smix tree` returns on a
# probe-carrying app. Without `--reader` this would compare the
# semantics tree with itself and agree about everything.
probe_tree() { "$SMIX" tree --device "$SERIAL" --port "$PORT" --json --reader probe 2>/dev/null; }
a11y_tree()  { "$SMIX" tree --device "$SERIAL" --port "$PORT" --json --reader a11y 2>/dev/null; }

# Wait for the screen, do not guess at it: the activity is resumed before
# Compose has composed, and a tree read in that gap is of a screen that has
# not arrived.
for _ in $(seq 1 40); do
  if probe_tree | grep -q interop_title; then break; fi
  sleep 0.5
done

# `|| status=$?` on both: under `set -e` a failing command
# substitution ends the script where it stands, and the verdict below
# never prints — this exited 1 with no output at all when `smix tree`
# was handed a flag the binary on PATH did not have.
probe_status=0 a11y_status=0
PROBE="$(probe_tree)" || probe_status=$?
A11Y="$(a11y_tree)" || a11y_status=$?
[ "$probe_status" = 0 ] || fail "asking the probe reader failed (exit $probe_status) — is this smix built from this tree?"
[ "$a11y_status" = 0 ] || fail "asking the accessibility reader failed (exit $a11y_status) — is this smix built from this tree?"
printf '%s' "$PROBE" | grep -q interop_title || fail "the probe never reported the interop screen"

VERDICTS="$(PROBE_JSON="$PROBE" A11Y_JSON="$A11Y" python3 "$ROOT/scripts/dev/v10.2-c7-probe-verdicts.py")" || { printf '%s\n' "$VERDICTS" | sed 's/^/[c7-probe]   /' >&2; fail "the probe does not agree with the screen"; }

printf '%s\n' "$VERDICTS" | sed 's/^/[c7-probe]   /' >&2

log "--- the reconciliation gate, on this screen"
python3 "$ROOT/scripts/dev/two-paths-agree.py" --device "$SERIAL" --port "$PORT" \
  --binary "$SMIX" \
  --activity .InteropActivity --min-both 3 --min-bounds-compared 3 \
  --prove-differences-exhibited >&2 \
  || fail "two-paths-agree is red on the interop screen"

log "--- and on the screen it has always driven"
python3 "$ROOT/scripts/dev/two-paths-agree.py" --device "$SERIAL" --port "$PORT" \
  --binary "$SMIX" \
  --min-both 16 --min-bounds-compared 16 >&2 \
  || fail "two-paths-agree is red on the Compose screen"

log "C7-PROBE-E2E-PASS on $SERIAL"
