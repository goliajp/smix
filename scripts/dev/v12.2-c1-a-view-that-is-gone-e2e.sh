#!/usr/bin/env bash
# v12.2-C1: a view that has been hidden is not on the screen.
#
# A consumer's Android app (2026-10-08) set a status line GONE and smix
# went on reporting it, with the text it had while it showed, for as
# long as anyone looked: `assertNotVisible` waited out its five seconds
# and failed. The probe walked Views without reading their visibility
# and took the size a View last had as the size it has, and a
# `LinearLayout` never lays out a GONE child again, so that size stays.
#
# The same walk explains their second report: a GONE empty-state layer,
# which its parent lays out over the whole list on every pass, was taken
# for something drawn over the list's rows, and a tap that reached a row
# was judged to have landed on the layer.
#
# Measured on this screen with the probe as 12.1.0 shipped it: both
# flows below fail, and `fixture_hidden_waiting` is in the probe's tree
# after it was hidden.
#
# There is no SKIP path: a gate that cannot judge must be red.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
ALIAS="${SMIX_V122_C1_ANDROID:-$E2E_ANDROID}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"

log()  { printf '[v12.2-c1] %s\n' "$*" >&2; }
fail() { printf '[v12.2-c1] FAIL: %s\n' "$*" >&2; exit 1; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0 WORK=""
cleanup() {
  [ -n "$WORK" ] && rm -rf "$WORK"
  if [ "$WE_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
      >/dev/null 2>&1 || printf '[v12.2-c1] warning: the runner was not stopped\n' >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$SERIAL" >/dev/null 2>&1 || true; fi
}
trap cleanup EXIT

command -v adb >/dev/null 2>&1 || fail "no adb on PATH — this judges an Android runner and cannot"
[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
[ -f "$APK" ] || fail "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
  || fail "the fixture apk on disk is not the one this tree builds"

if ! SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" \
   || [ -z "$SERIAL" ] || ! adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  log "booting $ALIAS"
  with_deadline 300 "$SMIX" sim boot "$ALIAS" >/dev/null 2>&1 || fail "could not boot $ALIAS"
  WE_BOOTED=1
  SERIAL="$("$SMIX" sim resolve "$ALIAS" 2>/dev/null | tail -1)" || fail "$ALIAS was booted and does not resolve"
fi
case "$SERIAL" in
  emulator-*) : ;;
  # This installs a fixture app; a registered phone belongs to somebody.
  *) fail "$ALIAS resolves to '$SERIAL', which is not an emulator — refusing" ;;
esac
with_deadline 120 adb -s "$SERIAL" wait-for-device || fail "$SERIAL did not come up"
for _ in $(seq 1 60); do
  adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done

adb -s "$SERIAL" install -r -g "$APK" >/dev/null 2>&1 || fail "could not install the fixture"

if ! curl -s -m 5 "http://localhost:$PORT/health" >/dev/null 2>&1; then
  "$SMIX" runner up "$ALIAS" --platform android --runner-port "$PORT" >/dev/null 2>&1 \
    || fail "the runner would not start on $SERIAL:$PORT"
  WE_UPPED=1
fi

WORK="$(mktemp -d)"
start_screen() {
  adb -s "$SERIAL" shell am start -W -S -n "$APPID/.HiddenActivity" >/dev/null 2>&1 \
    || fail "could not start the hidden screen"
}
tree_of() { "$SMIX" tree --device "$SERIAL" --port "$PORT" --json --reader "$1" 2>/dev/null; }
# Exit 0 when the `smix tree --json` document on stdin has a node whose identifier or text
# is $1 — the same two fields the selectors below match.
has_node() {
  python3 -c '
import json, sys
want = sys.argv[1]
def walk(n):
    if n.get("identifier") == want or n.get("text") == want or n.get("label") == want:
        return True
    return any(walk(c) for c in n.get("children") or [])
doc = json.load(sys.stdin)
sys.exit(0 if walk(doc.get("root", doc)) else 1)
' "$1"
}
run_flow() {
  local name="$1" rc=0
  # The tap judgement is what is under test, so nobody's relaxed setting
  # may reach it.
  env -u SMIX_TAP_HIT_MISMATCH SMIX_RUNNER_PORT="$PORT" \
    "$SMIX_RUN" --device "$SERIAL" "$WORK/$name.yaml" >"$WORK/$name.log" 2>&1 || rc=$?
  if [ "$rc" -ne 0 ]; then
    tail -8 "$WORK/$name.log" >&2
    fail "the $name flow did not pass (exit $rc)"
  fi
}

start_screen
for _ in $(seq 1 40); do
  if tree_of probe | has_node "hidden screen"; then break; fi
  sleep 0.5
done

# Present first, from both readers: an absence later means something
# only if the same question found it here.
for reader in probe a11y; do
  tree_of "$reader" | has_node fixture_hidden_waiting \
    || fail "the $reader reader does not report the status line while it is showing"
done
log "the status line is reported by both readers while it shows"

log "--- a status line set GONE as rows are added below it"
cat >"$WORK/hide.yaml" <<EOF
appId: $APPID
---
- tapOn:
    id: "fixture_hidden_hide"
- assertVisible: "row added 2"
- assertNotVisible:
    id: "fixture_hidden_waiting"
EOF
run_flow hide
for reader in probe a11y; do
  tree=""
  tree="$(tree_of "$reader")" || fail "asking the $reader reader failed"
  printf '%s' "$tree" | has_node "row added 2" \
    || fail "the $reader reader does not show the rows added in the same frame"
  if printf '%s' "$tree" | has_node fixture_hidden_waiting; then
    fail "the $reader reader still reports the status line after it was hidden"
  fi
  if printf '%s' "$tree" | has_node "nothing here"; then
    fail "the $reader reader reports the hidden empty-state layer"
  fi
done
log "  gone from both readers, and assertNotVisible passed"

log "--- a tap on a row under a hidden layer that is laid out over it"
cat >"$WORK/row.yaml" <<EOF
appId: $APPID
---
- tapOn:
    id: "fixture_hidden_row"
- assertVisible: "row taps 1"
EOF
start_screen
run_flow row
log "  the tap was judged to land on the row, and the app counted it"

log "V12.2-C1-E2E-PASS on $SERIAL"
