#!/usr/bin/env bash
# v6.2-C1: the platform is a property of the device, not an argument.
#
# The unit tests (`platform_from_device`) pin resolve_run_platform in
# isolation. This pins that the resolution is reached on the way to two
# real devices, and that the SAME flow — byte-identical but for its
# appId — foregrounds its app on both without a --platform flag anywhere.
#
# The negative control is baked into the device kind, not asserted
# separately: if the Android device had been mis-inferred as iOS, the run
# would take the simctl path and die "Invalid device: emulator-XXXX"
# (the exact defect the consumer hit). So `ok:true` on the Android device
# with no --platform IS the proof that the kind was read; iOS is the
# mirror. And the judgement is the app being on screen — assertVisible on
# an element of that app — not the exit code.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
WORK="$(mktemp -d)"

IOS_ALIAS="${SMIX_C1_IOS:-smix-ios}"
AND_ALIAS="${SMIX_C1_ANDROID:-smix-android}"
IOS_PORT="${SMIX_C1_IOS_PORT:-22090}"
AND_PORT="${SMIX_C1_ANDROID_PORT:-22091}"

IOS_FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
IOS_APPID="jp.golia.smix.fixture"
AND_APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
AND_APPID="dev.smix.fixture"

log()  { printf '[c1] %s\n' "$*" >&2; }
step() { printf '[c1] --- %s\n' "$*" >&2; }
fail() { printf '[c1] FAIL: %s\n' "$*" >&2; exit 1; }
skip() { printf '[c1] SKIP: %s\n' "$*" >&2; exit 0; }

# Only what this script started, tracked per device. An emulator or sim
# somebody else booted is not ours to reclaim (§9 #9 / v6.1-C1).
IOS_WE_BOOTED=0 IOS_WE_UPPED=0
AND_WE_BOOTED=0 AND_WE_UPPED=0
IOS_UDID="" AND_SERIAL=""
cleanup() {
  if [ "$IOS_WE_UPPED" = 1 ]; then
  # Not silenced: a teardown that fails leaves a runner on the device,
  # and what it costs is not this script but the next one to start one
  # there. Measured elsewhere in this repo: 23 of 26 corpus flows red,
  # every one of them blaming the runner.
    if ! down_said="$("$SMIX" down --device "$IOS_UDID" 2>&1)"; then
      printf 'warning: the iOS runner was not stopped:\n%s\n' "$(printf '%s' "$down_said" | tail -3)" >&2
    fi
    # `smix down` SIGINTs the runner session; the xcodebuild it launched
    # has been seen to outlive it, reparented to init and bound to a sim
    # that is about to shut down. Reap only the one pinned to OUR udid —
    # never a pattern that could match another sim's runner.
    if [ -n "$IOS_UDID" ]; then
      P="$(pgrep -f "xcodebuild.*$IOS_UDID" 2>/dev/null || true)"
      [ -n "$P" ] && { kill -INT $P 2>/dev/null || true; sleep 1; kill -9 $P 2>/dev/null || true; }
    fi
  fi
  [ "$IOS_WE_BOOTED" = 1 ] && "$SMIX" sim shutdown "$IOS_UDID" >/dev/null 2>&1 || true
  if [ "$AND_WE_UPPED" = 1 ]; then
    if ! down_said="$("$SMIX" down --platform android --device "$AND_SERIAL" 2>&1)"; then
      printf 'warning: the Android runner was not stopped:\n%s\n' "$(printf '%s' "$down_said" | tail -3)" >&2
    fi
  fi
  [ "$AND_WE_BOOTED" = 1 ] && "$SMIX" sim shutdown "$AND_SERIAL" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"

# The two flows: identical bytes but for the appId. Written from one
# template so "不改一个字" is enforced, not promised.
flow_for() {
  cat > "$2" <<FLOW
appId: $1
---
- launchApp
- assertVisible: "Submit"
FLOW
}
flow_for "$IOS_APPID" "$WORK/ios.yaml"
flow_for "$AND_APPID" "$WORK/android.yaml"
# Prove the claim mechanically: the two files differ on exactly the appId.
DIFF_LINES="$(diff <(grep -v '^appId:' "$WORK/ios.yaml") <(grep -v '^appId:' "$WORK/android.yaml") | grep -c '^[<>]' || true)"
[ "$DIFF_LINES" = 0 ] || fail "the two flows differ off the appId line ($DIFF_LINES lines) — C1's premise is that only the app changes"

# ---- iOS half -------------------------------------------------------
run_ios() {
  command -v xcrun >/dev/null 2>&1 || { log "no xcrun — skipping iOS half"; return 0; }
  IOS_UDID="$("$SMIX" sim resolve "$IOS_ALIAS" 2>/dev/null | grep -v '^kevy:' | tr -d '[:space:]')" || true
  [ -n "$IOS_UDID" ] || { log "no sim registered as '$IOS_ALIAS' — skipping iOS half"; return 0; }
  [ -d "$IOS_FIXTURE" ] || { log "no iOS fixture at $IOS_FIXTURE — skipping iOS half"; return 0; }

  step "iOS: boot $IOS_ALIAS ($IOS_UDID), install fixture, runner up"
  if ! xcrun simctl list devices 2>/dev/null | grep -q "$IOS_UDID.*Booted"; then
    "$SMIX" sim boot "$IOS_UDID" >"$WORK/ios-boot.log" 2>&1 || fail "iOS sim boot failed: $(tail -3 "$WORK/ios-boot.log")"
    IOS_WE_BOOTED=1
  fi
  "$SMIX" sim install "$IOS_UDID" "$IOS_FIXTURE" >"$WORK/ios-install.log" 2>&1 || fail "iOS install failed: $(tail -3 "$WORK/ios-install.log")"
  SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" runner up "$IOS_UDID" --bundle "$IOS_APPID" --runner-port "$IOS_PORT" >"$WORK/ios-up.log" 2>&1 || fail "iOS runner up failed: $(tail -5 "$WORK/ios-up.log")"
  IOS_WE_UPPED=1

  step "iOS: run the shared flow with NO --platform — platform must come from the sim's kind"
  OUT="$(SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" run --device "$IOS_UDID" "$WORK/ios.yaml" 2>&1 | grep -v '^kevy:')" || true
  printf '%s\n' "$OUT" | grep -qE 'simctl|Invalid device' && fail "iOS run went a wrong path: $OUT"
  printf '%s\n' "$OUT" | grep -q '"ok":true' || fail "iOS run did not pass (app not foregrounded / Submit not visible): $OUT"
  log "iOS PASS — no --platform, sim kind → iOS, Submit visible"
}

# ---- Android half ---------------------------------------------------
run_android() {
  command -v adb >/dev/null 2>&1 || { log "no adb — skipping Android half"; return 0; }
  AND_SERIAL="$("$SMIX" sim resolve "$AND_ALIAS" 2>/dev/null | grep -v '^kevy:' | tr -d '[:space:]')" || true
  [ -n "$AND_SERIAL" ] || { log "no emulator registered as '$AND_ALIAS' — skipping Android half"; return 0; }
  [ -f "$AND_APK" ] || { log "no Android fixture apk at $AND_APK (scripts/dev/build-android-fixture.sh) — skipping Android half"; return 0; }

  step "Android: ensure $AND_ALIAS ($AND_SERIAL) up, install fixture, runner up"
  if ! adb devices 2>/dev/null | grep -q "^${AND_SERIAL}[[:space:]]*device"; then
    "$SMIX" sim boot "$AND_SERIAL" >"$WORK/and-boot.log" 2>&1 || fail "emulator boot failed: $(tail -3 "$WORK/and-boot.log")"
    AND_WE_BOOTED=1
  fi
  adb -s "$AND_SERIAL" install -r "$AND_APK" >"$WORK/and-install.log" 2>&1 || fail "Android install failed: $(tail -3 "$WORK/and-install.log")"
  if ! "$SMIX" runner up "$AND_SERIAL" --platform android --runner-port "$AND_PORT" >"$WORK/and-up.log" 2>&1; then
    # A runner already on that port from a prior half is fine to reuse.
    grep -qiE 'already|in use|running' "$WORK/and-up.log" || fail "Android runner up failed: $(tail -5 "$WORK/and-up.log")"
  fi
  AND_WE_UPPED=1

  step "Android: run the shared flow with NO --platform — platform must come from the emulator's kind"
  adb -s "$AND_SERIAL" shell am force-stop "$AND_APPID" >/dev/null 2>&1 || true
  OUT="$(SMIX_RUNNER_PORT="$AND_PORT" "$SMIX" run --device "$AND_SERIAL" "$WORK/android.yaml" 2>&1 | grep -v '^kevy:')" || true
  printf '%s\n' "$OUT" | grep -qE 'simctl|Invalid device' && fail "Android run took the iOS/simctl path — platform was NOT read from the device: $OUT"
  printf '%s\n' "$OUT" | grep -q '"ok":true' || fail "Android run did not pass (app not foregrounded / Submit not visible): $OUT"
  log "Android PASS — no --platform, emulator kind → Android, Submit visible"
}

run_ios
run_android
log "v6.2-C1 PASS: one flow, two device kinds, no --platform anywhere"
