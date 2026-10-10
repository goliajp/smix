#!/usr/bin/env bash
# v13.2-C1: a page an app paints over another page hides it.
#
# A consumer's Android app pushes every page as a full-screen dialog with
# an opaque background (2026-10-10). The page under it stays laid out, and
# the probe reported its text as on screen: `assertNotVisible` on a page
# below failed, a tap on a list row went to the row of the covered list,
# and a wait for the page below "passed" with the page above in front.
#
# The fixture stacks the same three windows: the activity, a full-screen
# page painted opaque, and a sheet in a translucent window over part of the
# page. The page hides the activity; the sheet hides nothing — the page
# shows through it, which is why the accessibility service, which drops a
# covered window by its bounds alone, cannot be the judge here.
#
# There is no SKIP path: a gate that cannot judge must be red.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
ALIAS="${SMIX_V132_C1_ANDROID:-$E2E_ANDROID}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
PORT="$SMIX_RUNNER_PORT"
APPID="dev.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"

log()  { printf '[v13.2-c1] %s\n' "$*" >&2; }
fail() { printf '[v13.2-c1] FAIL: %s\n' "$*" >&2; exit 1; }

SERIAL="" WE_BOOTED=0 WE_UPPED=0 WORK=""
cleanup() {
  [ -n "$WORK" ] && rm -rf "$WORK"
  if [ "$WE_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$PORT" \
      >/dev/null || printf '[v13.2-c1] warning: the runner was not stopped\n' >&2
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
has() {
  "$SMIX" tree --device "$SERIAL" --port "$PORT" --json --reader probe 2>/dev/null | python3 -c '
import json, sys
want = sys.argv[1]
def walk(n):
    if want in (n.get("identifier"), n.get("text"), n.get("label")):
        return True
    return any(walk(c) for c in n.get("children") or [])
doc = json.load(sys.stdin)
sys.exit(0 if walk(doc.get("root", doc)) else 1)
' "$1"
}
run_flow() {
  local name="$1" rc=0
  env -u SMIX_TAP_HIT_MISMATCH SMIX_RUNNER_PORT="$PORT" \
    "$SMIX_RUN" --device "$SERIAL" --platform android --no-launch "$WORK/$name.yaml" >"$WORK/$name.log" 2>&1 || rc=$?
  if [ "$rc" -ne 0 ]; then
    tail -8 "$WORK/$name.log" >&2
    fail "the $name flow did not pass (exit $rc)"
  fi
}

adb -s "$SERIAL" shell am start -W -S -n "$APPID/.StackActivity" >/dev/null 2>&1 \
  || fail "could not start the stack screen"
for _ in $(seq 1 40); do has "under the page" && break; sleep 0.5; done
# Present first: an absence below means something only if this found it.
has "under the page" || fail "the activity's own text is not reported before anything covers it"
has fixture_stack_open_page || fail "the activity's button is not reported before anything covers it"

log "--- a page painted over the whole screen"
cat >"$WORK/page.yaml" <<EOF
appId: $APPID
---
- tapOn:
    id: "fixture_stack_open_page"
- assertVisible: "the page"
- assertNotVisible: "under the page"
- assertNotVisible:
    id: "fixture_stack_open_page"
EOF
run_flow page
if has "under the page" || has fixture_stack_open_page; then
  fail "the probe still reports the activity under the page"
fi
log "  the activity under it is gone from the probe's tree, and assertNotVisible passed"

log "--- a sheet in a translucent window over part of the page"
cat >"$WORK/sheet.yaml" <<EOF
appId: $APPID
---
- tapOn:
    id: "fixture_stack_open_sheet"
- assertVisible: "Time Range"
- assertVisible:
    id: "fixture_stack_sheet_apply"
- assertVisible: "the page"
EOF
run_flow sheet
has "the page" || fail "the page that shows through the sheet is no longer reported"
log "  the sheet is reported, and the page still shows through it"

log "--- a list narrowed in place, the way a consumer's device search narrows it"
# Rows the filter drops leave the tree with the screen. The consumer's eight
# rows after a search were the covered page's, not rows left behind.
adb -s "$SERIAL" shell am start -W -S -n "$APPID/.FilterActivity" >/dev/null 2>&1 \
  || fail "could not start the filter screen"
cat >"$WORK/filter.yaml" <<EOF
appId: $APPID
---
- tapOn:
    id: "fixture_filter_input"
- inputText: "Lobby Entrance"
- assertVisible: "1 result found"
EOF
run_flow filter
rows="$("$SMIX" tree --device "$SERIAL" --port "$PORT" --json --reader probe 2>/dev/null | python3 -c '
import json, sys
def walk(n):
    here = 1 if n.get("identifier") == "fixture_filter_row" else 0
    return here + sum(walk(c) for c in n.get("children") or [])
doc = json.load(sys.stdin)
print(walk(doc.get("root", doc)))
')"
[ "$rows" = 1 ] || fail "after the filter the probe reports $rows rows, and one is on the screen"
log "  one row on the screen, one in the tree"

log "V13.2-C1-E2E-PASS on $SERIAL"
