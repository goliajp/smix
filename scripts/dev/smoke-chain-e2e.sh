#!/usr/bin/env bash
# The most ordinary path, on both platforms: boot by alias, bring a runner
# up, run a one-step flow, bring it down, shut down what this booted.
#
# Why this exists: every checkpoint drives its own subject, and none of
# them drove this. One change gave the device ledger a new kind of row, a
# `matches!` with a default arm filed it as "not a service", and on
# Android `sim boot` → `runner up` → `run` stopped working — `run` was
# refused, naming a recording that was not there. Two later checkpoints
# were committed on top before a third happened to walk the path. A check
# of the path itself is the one that would have said so the same day.
#
# Deliberately through smix only (`sim boot`, `runner up`, `run`,
# `runner down`, `sim shutdown`): the thing under test is that chain, so
# no step of it is done by hand.
#
# Exit: 0 judged and passed, 1 judged and failed, 2 could not judge.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/scripts/lib/e2e-binary.sh"
source "$ROOT/scripts/lib/gate-port.sh"
source "$ROOT/scripts/lib/deadline.sh"
AND_PORT="$SMIX_RUNNER_PORT"
gate_free_port IOS_PORT
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/e2e-devices.sh"
AND_ALIAS="${SMIX_SMOKE_ANDROID:-$E2E_ANDROID}"
IOS_ALIAS="${SMIX_SMOKE_IOS:-$E2E_IOS}"
AND_APPID="dev.smix.fixture"
IOS_APPID="jp.golia.smix.fixture"
APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
APP="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
WORK="$(mktemp -d)"

log()  { printf '[smoke-chain] %s\n' "$*" >&2; }
fail() { printf '[smoke-chain] FAIL: %s\n' "$*" >&2; exit 1; }
cannot_judge() { printf '[smoke-chain] CANNOT JUDGE: %s\n' "$*" >&2; exit 2; }

SERIAL="" UDID="" AND_UPPED=0 IOS_UPPED=0 WE_BOOTED=0 IOS_WE_BOOTED=0
cleanup() {
  if [ "$AND_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$AND_PORT" >/dev/null 2>&1 || true
  fi
  if [ "$IOS_UPPED" = 1 ]; then
    "$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" >/dev/null 2>&1 || true
  fi
  if [ "$WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$AND_ALIAS" >/dev/null 2>&1 || true; fi
  if [ "$IOS_WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -f "$APK" ] || cannot_judge "no fixture apk — run: bash scripts/dev/build-android-fixture.sh"
[ -d "$APP" ] || cannot_judge "no iOS fixture — run: bash scripts/dev/build-fixture-app.sh"

# $1 label, $2 what failed, $3 its output. Each link is judged by its own
# exit code, and a failure names the link rather than the chain.
link_failed() { printf '[smoke-chain] FAIL: %s: %s — %s\n' "$1" "$2" "$(printf '%s' "$3" | tail -3 | tr '\n' ' ')" >&2; exit 1; }

# ---- Android ----------------------------------------------------------
out=""
if ! SERIAL="$("$SMIX" sim resolve "$AND_ALIAS" 2>/dev/null | tail -1)" || [ -z "$SERIAL" ]; then
  out="$(with_deadline 300 "$SMIX" sim boot "$AND_ALIAS" 2>&1)" || link_failed android "sim boot $AND_ALIAS" "$out"
  WE_BOOTED=1
  SERIAL="$("$SMIX" sim resolve "$AND_ALIAS" 2>/dev/null | tail -1)" \
    || link_failed android "sim resolve $AND_ALIAS after boot" ""
fi
case "$SERIAL" in
  emulator-*) : ;;
  *) cannot_judge "$AND_ALIAS resolves to '$SERIAL', which is not an emulator — refusing" ;;
esac
out="$(with_deadline 180 "$SMIX" sim install "$AND_ALIAS" "$APK" 2>&1)" || link_failed android "sim install" "$out"
out="$(with_deadline 300 "$SMIX" runner up "$AND_ALIAS" --platform android --runner-port "$AND_PORT" 2>&1)" \
  || link_failed android "runner up" "$out"
AND_UPPED=1
printf 'appId: %s\n---\n- launchApp\n- assertVisible:\n    id: fixture_submit\n' "$AND_APPID" > "$WORK/a.yaml"
out="$(SMIX_RUNNER_PORT="$AND_PORT" with_deadline 180 "$SMIX_RUN" --device "$SERIAL" "$WORK/a.yaml" 2>&1)" \
  || link_failed android "run" "$out"
out="$("$SMIX" runner down --platform android --device "$SERIAL" --runner-port "$AND_PORT" 2>&1)" \
  || link_failed android "runner down" "$out"
AND_UPPED=0
log "android: sim boot → runner up → run → runner down, each answered"

# ---- iOS --------------------------------------------------------------
UDID="$("$SMIX" sim resolve "$IOS_ALIAS" 2>/dev/null | tail -1)" \
  || cannot_judge "no iOS simulator registered as $IOS_ALIAS"
if [ "$(simulator_state "$UDID")" != Booted ]; then
  out="$(with_deadline 300 "$SMIX" sim boot "$IOS_ALIAS" 2>&1)" || link_failed ios "sim boot $IOS_ALIAS" "$out"
  IOS_WE_BOOTED=1
fi
out="$(with_deadline 180 "$SMIX" sim install "$UDID" "$APP" 2>&1)" || link_failed ios "sim install" "$out"
out="$(with_deadline 600 "$SMIX" runner up "$UDID" --bundle "$IOS_APPID" --runner-port "$IOS_PORT" 2>&1)" \
  || link_failed ios "runner up" "$out"
IOS_UPPED=1
printf 'appId: %s\n---\n- launchApp\n- assertVisible:\n    id: fixture-submit\n' "$IOS_APPID" > "$WORK/i.yaml"
out="$(SMIX_RUNNER_PORT="$IOS_PORT" with_deadline 180 "$SMIX_RUN" --device "$UDID" "$WORK/i.yaml" 2>&1)" \
  || link_failed ios "run" "$out"
out="$("$SMIX" runner down --device "$UDID" --runner-port "$IOS_PORT" 2>&1)" \
  || link_failed ios "runner down" "$out"
IOS_UPPED=0
log "ios: sim boot → runner up → run → runner down, each answered"

log "SMOKE-CHAIN-PASS (the ordinary path answered on both platforms)"
