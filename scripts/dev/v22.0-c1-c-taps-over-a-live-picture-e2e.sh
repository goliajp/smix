#!/usr/bin/env bash
# v22.0-C1: taps right after a sheet comes or goes land, over a picture that
# never stops moving.
#
# A consumer's screens sit over a live video view, and their flows put a
# fixed wait before every tap after a sheet: without it a tap was refused
# as covered, or — through `dispatch: xcui` — answered ok and did nothing.
# The fixture's live screen redraws its background every frame and counts
# what each control received:
#   1. the default tap: a sheet's Confirm, then at once the button the
#      sheet was covering, then the next sheet — every tap counted once
#   2. the same through `dispatch: xcui`: a tap on a sheet still sliding
#      in answered ok and missed; every tap is counted once now
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

log()  { printf '[v22.0-c1-c] %s\n' "$*" >&2; }
fail() { printf '[v22.0-c1-c] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v22.0-c1-c] cannot judge: %s\n' "$*" >&2; exit 2; }

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

judge() { # $1 name, $2 dispatch line prefix ("" or "dispatch: xcui, ")
  local d="$2"
  cat >"$WORK/$1.yaml" <<YAML
appId: $APP
---
- launchApp:
    stopApp: true
    arguments: ["-live"]
- extendedWaitUntil: { visible: { id: live-menu }, timeout: 15000 }
- tapOn: { ${d}id: live-present }
- tapOn: { ${d}id: live-modal-confirm }
- tapOn: { ${d}id: live-menu }
- tapOn: { ${d}id: live-present }
- tapOn: { ${d}id: live-modal-confirm }
- tapOn: { ${d}id: live-menu }
- assertVisible: { id: live-counts, text: "menu 2 modal 2 submit 0" }
YAML
  # raw run: a refusal, or a count that disagrees, is the verdict here
  if ! "$SMIX" run --device "$UDID" --runner-port "$PORT" "$WORK/$1.yaml" >"$WORK/$1.log" 2>&1; then
    fail "$1: $(grep -m1 -E 'FAIL' "$WORK/$1.log"); counts: $(grep -oE 'name="menu [0-9]+ modal [0-9]+ submit [0-9]+"' "$WORK/$1.log" | head -1)"
  fi
}

log "1. default taps after a sheet comes and goes"
judge default ""

log "2. the same through dispatch: xcui"
judge xcui "dispatch: xcui, "

log "V22.0-C1-C-TAPS-OVER-A-LIVE-PICTURE-PASS"
