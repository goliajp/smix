#!/usr/bin/env bash
# A tap at an element a page sheet covers is refused, and touches nothing.
#
# A page sheet is not an alert, dialog or sheet to XCUITest: it is a second
# container the window draws after the content it covers. 12.0.0 refused a
# covered tap only under those three, so a tap at a form row under a sheet
# was sent, ticked the sheet row drawn at the same place, and was reported
# as done — a consumer's flows picked the wrong camera that way, and this
# script against that runner counted "form 0 sheet 1", three runs of three.
#
# On the first smix simulator, with the fixture's sheet-over-form stage:
#   * the form row under the open sheet: the tap fails NOT_VISIBLE and
#     neither layer counts a tap
#   * a sheet row: tapped, the sheet counts one
#   * the form row with no sheet open: tapped, the form counts one
# The counts are the app's own, so where a touch went is not read from
# smix's answer.
#
# Exit: 0 judged and passed, 1 judged and failed, 2 could not judge.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
source "$ROOT/scripts/lib/e2e-devices.sh"
UDID="${SMIX_C2_IOS:-$E2E_IOS}"
UDID="$("$SMIX" sim resolve "$UDID" 2>/dev/null | tr -d '[:space:]')" || true
APPID="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
PROJECT="$ROOT/swift-bridge/SmixRunner.xcodeproj"
WORK="$(mktemp -d)"

log()  { printf '[c2-sheet] %s\n' "$*" >&2; }
fail() { printf '[c2-sheet] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[c2-sheet] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

UPPED=0 WE_BOOTED=0
cleanup() {
  if [ "$UPPED" = 1 ]; then
    said="$("$SMIX" runner down --device "$UDID" --runner-port "$PORT" 2>&1)" \
      || printf '[c2-sheet] warning: the runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -n "$UDID" ] || cannot_judge "no simulator registered for the first iOS slot"
[ -d "$FIXTURE" ] || cannot_judge "no iOS fixture — run: bash scripts/dev/build-fixture-app.sh"
if [ "$(simulator_state "$UDID")" != Booted ]; then
  "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || cannot_judge "could not boot $UDID"
  WE_BOOTED=1
fi
"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 \
  || cannot_judge "could not install the fixture: $(tail -3 "$WORK/install.log" | tr '\n' ' ')"
SMIX_RUNNER_PORT="$PORT" "$SMIX" runner up "$UDID" --bundle "$APPID" --runner-port "$PORT" \
  --runner-project "$PROJECT" >"$WORK/up.log" 2>&1 \
  || cannot_judge "the runner would not start: $(tail -5 "$WORK/up.log" | tr '\n' ' ')"
UPPED=1

counts() {
  curl -s -m 30 -H "App-Bundle-Id: $APPID" "http://127.0.0.1:$PORT/tree" \
    | grep -o '"label":"form [0-9]* sheet [0-9]*"' | head -1 | sed 's/"label"://; s/"//g'
}
flow() { # flow <name> <tap id> <open the sheet first: yes|no>
  {
    printf 'appId: %s\n---\n- launchApp:\n    arguments: ["-sheet-over-form"]\n' "$APPID"
    if [ "$3" = yes ]; then
      printf -- "- tapOn: { id: 'sheet_open' }\n"
      printf -- "- extendedWaitUntil: { visible: { id: 'sheet_row_3' }, timeout: 5000 }\n"
    fi
    printf -- "- tapOn: { id: '%s' }\n" "$2"
  } >"$WORK/$1.yaml"
}

flow covered form_row_3 yes
flow sheet sheet_row_3 yes
flow form form_row_3 no

for i in 1 2 3; do
  rc=0
  # raw run: this leg expects the tap to be refused and reads which way it failed
  SMIX_RUNNER_PORT="$PORT" "$SMIX" run "$WORK/covered.yaml" --device "$UDID" --platform ios \
    --runner-port "$PORT" >"$WORK/covered-$i.log" 2>&1 || rc=$?
  c="$(counts)"
  [ "$c" = "form 0 sheet 0" ] || fail "covered $i: the app counted '$c' — the tap reached something"
  [ "$rc" != 0 ] || fail "covered $i: nothing was touched and the flow passed"
  grep -q 'NOT_VISIBLE' "$WORK/covered-$i.log" \
    || { tail -6 "$WORK/covered-$i.log" >&2; fail "covered $i: failed (exit $rc), but not as covered"; }
  log "covered $i: refused NOT_VISIBLE, app counts '$c'"
done

SMIX_RUNNER_PORT="$PORT" "$SMIX_RUN" --device "$UDID" --platform ios "$WORK/sheet.yaml" >"$WORK/sheet.log" 2>&1 \
  || { tail -6 "$WORK/sheet.log" >&2; fail "a row on the sheet could not be tapped"; }
c="$(counts)"; [ "$c" = "form 0 sheet 1" ] || fail "sheet row: the app counted '$c'"
log "sheet row: tapped, app counts '$c'"

SMIX_RUNNER_PORT="$PORT" "$SMIX_RUN" --device "$UDID" --platform ios "$WORK/form.yaml" >"$WORK/form.log" 2>&1 \
  || { tail -6 "$WORK/form.log" >&2; fail "the form row with no sheet open could not be tapped"; }
c="$(counts)"; [ "$c" = "form 1 sheet 0" ] || fail "form row: the app counted '$c'"
log "form row, no sheet: tapped, app counts '$c'"

log "PASS"
