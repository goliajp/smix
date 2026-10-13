#!/usr/bin/env bash
# v15.1-C14: on a simulator, XCUITest's query log stays out of the app's log.
#
# XCUITest logs every query it answers from inside the app whose tree it
# reads, under the subsystem com.apple.dt.xctest; a consumer's app log was
# half that. `smix runner up` turns the subsystem off on the simulator. The
# script puts the subsystem back to its default, shows that the count can
# see those lines (tree reads with it on log some), then brings the runner
# up and judges:
#   - the same reads log none of them, and every read answered
#   - the simulator reports the subsystem off
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/e2e-devices.sh"
IOS_ALIAS="${SMIX_C14_IOS:-$E2E_IOS}"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
gate_free_port IOS_PORT
APPID="jp.golia.smix.fixture"
PROC="SmixFixture"
SUBSYSTEM="com.apple.dt.xctest"
FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
PROJECT="${SMIX_C14_RUNNER_PROJECT:-$ROOT/swift-bridge/SmixRunner.xcodeproj}"
READS=10
WORK="$(mktemp -d)"

log()  { printf '[v15.1-c14] %s\n' "$*" >&2; }
fail() { printf '[v15.1-c14] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[v15.1-c14] cannot judge: %s\n' "$*" >&2; exit 2; }

UDID="" WE_BOOTED=0 WE_UPPED=0
cleanup() {
  if [ "$WE_UPPED" = 1 ]; then
    if ! said="$("$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" 2>&1)"; then
      printf '[v15.1-c14] warning: the iOS runner was not stopped:\n%s\n' "$(printf '%s' "$said" | tail -3)" >&2
    fi
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

up() {
  SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" runner up "$UDID" --bundle "$APPID" --runner-port "$IOS_PORT" \
    --runner-project "$PROJECT" >"$WORK/up-$1.log" 2>&1 || fail "runner up: $(tail -5 "$WORK/up-$1.log")"
  WE_UPPED=1
}

now_ms() { python3 -c 'import datetime; print(datetime.datetime.now().isoformat(" ", "milliseconds"))'; }

# reads <name> — $READS tree reads; prints "<answered> <xctest lines> <app
# lines>", counting only lines stamped after the first read began: `log
# show` takes whole seconds, and the second before holds runner up's own
# queries, made before it turned the log off.
reads() {
  local start since end n=0 i
  since="$(now_ms)"
  start="${since%.*}"
  for i in $(seq "$READS"); do
    if SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" tree --device "$UDID" --runner-port "$IOS_PORT" >/dev/null 2>>"$WORK/$1.err"; then
      n=$((n + 1))
    fi
  done
  end="$(date -v+1S '+%Y-%m-%d %H:%M:%S')"
  xcrun simctl spawn "$UDID" log show --start "$start" --end "$end" \
    --predicate "process == \"$PROC\"" --style compact >"$WORK/$1.all" 2>/dev/null \
    || fail "log show on the simulator failed"
  awk -v since="$since" '($1 " " $2) >= since' "$WORK/$1.all" >"$WORK/$1.log"
  printf '%s %s %s\n' "$n" "$(grep -c "\[$SUBSYSTEM" "$WORK/$1.log" || true)" "$(grep -c "$PROC\[" "$WORK/$1.log" || true)"
}

command -v xcrun >/dev/null 2>&1 || cannot_judge "no xcrun"
UDID="$("$SMIX" sim resolve "$IOS_ALIAS" 2>/dev/null | tr -d '[:space:]')" || true
[ -n "$UDID" ] || cannot_judge "no simulator resolves '$IOS_ALIAS'"
[ -d "$FIXTURE" ] || fail "no iOS fixture at $FIXTURE (bash scripts/dev/build-fixture-app.sh)"
if [ "$(simulator_state "$UDID")" != Booted ]; then
  "$SMIX" sim boot "$UDID" >"$WORK/boot.log" 2>&1 || fail "boot: $(tail -3 "$WORK/boot.log")"
  WE_BOOTED=1
fi
"$SMIX" sim install "$UDID" "$FIXTURE" >"$WORK/install.log" 2>&1 || fail "install: $(tail -3 "$WORK/install.log")"

# the count sees these lines: with the subsystem at its default, reads log some
up first
xcrun simctl spawn "$UDID" log config --reset --subsystem "$SUBSYSTEM" \
  || cannot_judge "could not put $SUBSYSTEM back to its default"
read -r n xc app <<<"$(reads on)"
[ "$n" = "$READS" ] || fail "with the log on, $n of $READS reads answered: $(tail -3 "$WORK/on.err")"
[ "$xc" -gt 0 ] || cannot_judge "with $SUBSYSTEM at its default, $READS reads logged none of its lines in $PROC ($app lines in all) — the count cannot see what it judges"
log "subsystem at its default: $xc of $app lines in $PROC are $SUBSYSTEM"

# runner up turns it off; the runner is already up, and up says so and still quiets
up second
read -r n xc app <<<"$(reads off)"
[ "$n" = "$READS" ] || fail "with the log off, $n of $READS reads answered: $(tail -3 "$WORK/off.err")"
[ "$xc" = 0 ] || { grep "\[$SUBSYSTEM" "$WORK/off.log" | head -3 >&2; fail "after runner up, $READS reads still logged $xc lines of $SUBSYSTEM in $PROC"; }
status="$(xcrun simctl spawn "$UDID" log config --status --subsystem "$SUBSYSTEM")"
case "$status" in
  *"$SUBSYSTEM'  OFF"*|*"$SUBSYSTEM' OFF"*) ;;
  *) fail "the simulator does not report $SUBSYSTEM off: $status" ;;
esac
log "after runner up: 0 lines of $SUBSYSTEM ($app lines in all), $READS of $READS reads answered, the simulator reports it off"
log "V15.1-C14-E2E-PASS"
