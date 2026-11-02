#!/usr/bin/env bash
# v22.0-C2: a tap with `until` is done again only while its effect has not
# shown, over a picture that never stops moving.
#
# A consumer's flows tapped, waited a fixed time and tapped again "in case
# the first was swallowed", which taps twice when the first only took a
# moment. The fixture's live screen counts what each control received:
#   1. a tap whose effect shows is done once: the menu counts 1, not 2
#   2. a control that ignores its first touch is touched again, and once
#      more is not: "stubborn 1" (two touches, the first ignored)
#   3. a tap on something that changes nothing fails saying the tap landed
#      and the screen did not change, after the retries it was given
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/judged-run.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
source "$ROOT/scripts/lib/deadline.sh"
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port PORT
APP="jp.golia.smix.fixture"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
WORK="$(mktemp -d)"

log()  { printf '[v22.0-c2-a] %s\n' "$*" >&2; }
fail() { printf '[v22.0-c2-a] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v22.0-c2-a] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" UP=0 WE_BOOTED=0
cleanup() {
  local said
  if [ "$UP" = 1 ] && ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$PORT" 2>&1)"; then
    log "warning: the runner on $PORT was not stopped:"
    printf '%s\n' "$said" | tail -3 >&2
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

UDID="$("$SMIX" sim resolve "$E2E_IOS" 2>/dev/null | tail -1 | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$E2E_IOS'"
case "$(simulator_name "$UDID")" in
  sim-smix-*) ;;
  *) cannot_judge "'$E2E_IOS' resolves to $UDID, which is not one of smix's own simulators" ;;
esac
if [ "$(simulator_state "$UDID")" != Booted ]; then
  with_deadline 300 "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi
"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"
"$SMIX" runner up "$UDID" --bundle "$APP" --runner-port "$PORT" >"$WORK/up.log" 2>&1 \
  || fail "runner up: $(tail -5 "$WORK/up.log")"
UP=1

flow() { # $1 name, rest: steps after opening the live screen
  local name="$1"; shift
  {
    printf 'appId: %s\n---\n- launchApp:\n    stopApp: true\n    arguments: ["-live"]\n' "$APP"
    printf -- '- extendedWaitUntil: { visible: { id: live-menu }, timeout: 15000 }\n'
    printf '%s\n' "$@"
  } >"$WORK/$name.yaml"
}

log "1. a tap whose effect shows is done once"
flow once \
  '- tapOn: { id: live-menu, until: { visible: { id: live-counts, text: "menu 1 modal 0 submit 0" } }, retries: 2 }' \
  '- assertVisible: { id: live-counts, text: "menu 1 modal 0 submit 0" }'
"$SMIX_RUN" --device "$UDID" --runner-port "$PORT" "$WORK/once.yaml" >"$WORK/once.log" 2>&1 \
  || fail "1: $(grep -m1 -E 'FAIL' "$WORK/once.log")"

log "2. a control that ignores its first touch is touched again, once"
flow twice \
  '- tapOn: { id: live-stubborn, until: { visible: { id: live-stubborn-count, text: "stubborn 1" } }, within: 1500, retries: 3 }' \
  '- assertVisible: { id: live-stubborn-count, text: "stubborn 1" }'
"$SMIX_RUN" --device "$UDID" --runner-port "$PORT" "$WORK/twice.yaml" >"$WORK/twice.log" 2>&1 \
  || fail "2: $(grep -m1 -E 'FAIL' "$WORK/twice.log")"

log "3. a tap that changes nothing fails saying so"
flow never \
  '- tapOn: { id: live-counts, until: { visible: { id: no-such-thing } }, within: 500, retries: 1 }'
# raw run: this step's failure is what is judged
if "$SMIX" run --device "$UDID" --runner-port "$PORT" "$WORK/never.yaml" >"$WORK/never.log" 2>&1; then
  fail "3: a tap whose effect never showed passed"
fi
grep -q "was done 2 times" "$WORK/never.log" || fail "3: not done twice: $(grep -m1 FAIL "$WORK/never.log")"
grep -q "landed and the screen did not change" "$WORK/never.log" \
  || fail "3: the failure does not say the screen did not change: $(grep -m1 FAIL "$WORK/never.log")"

log "V22.0-C2-A-A-TAP-AND-ITS-EFFECT-PASS"
