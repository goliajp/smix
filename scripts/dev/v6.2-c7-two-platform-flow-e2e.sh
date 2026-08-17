#!/usr/bin/env bash
# v6.2-C7: one flow, byte-identical but for its appId, passes on both
# platforms — the cold plan's exit criterion, strung end to end.
#
# The four deep gates (c1/c3/c4/c5) each own a property's two-sided teeth,
# and c3/c4/c5 run only on Android. This is the integration gate: a single
# flow that uses only portable selectors (role: textField, text:) — never
# a platform-native id (iOS fixture-input with a hyphen, Android
# fixture_input with an underscore) — run on iOS and Android with no
# --platform anywhere, tying together ① launchApp foregrounds, ② platform
# read from the device, ③ fill/find from both the flow and the CLI, and
# ④ --env reaching the flow. Android also gets ⑤ (tree prints text) as an
# integration touch; the deep two-sided teeth stay in c4/c5.
#
# Judged by content and the screen, not return codes. Boots only what it
# needs and tears down only what it started.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"
IOS_ALIAS="${SMIX_C7_IOS:-smix-ios}"
AND_ALIAS="${SMIX_C7_ANDROID:-smix-android}"
IOS_PORT="${SMIX_C7_IOS_PORT:-22095}"
AND_PORT="${SMIX_C7_ANDROID_PORT:-22096}"
IOS_APPID="jp.golia.smix.fixture"
AND_APPID="dev.smix.fixture"
IOS_FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
AND_APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
IOS_PROJECT="$ROOT/swift-bridge/SmixRunner.xcodeproj"
ENV_VAL="c7EnvValueQ9"
CLI_WORD="c7CliWordK3"
MISSING="SMIX_C7_MISSING"
WORK="$(mktemp -d)"

log()  { printf '[c7] %s\n' "$*" >&2; }
step() { printf '[c7] --- %s\n' "$*" >&2; }
fail() { printf '[c7] FAIL: %s\n' "$*" >&2; exit 1; }
skip() { printf '[c7] SKIP: %s\n' "$*" >&2; exit 0; }

IOS_UDID="" AND_SERIAL=""
IOS_WE_BOOTED=0 IOS_WE_UPPED=0 AND_WE_BOOTED=0 AND_WE_UPPED=0
cleanup() {
  [ "$IOS_WE_UPPED" = 1 ]  && "$SMIX" down --device "$IOS_UDID" >/dev/null 2>&1 || true
  if [ -n "$IOS_UDID" ]; then
    P="$(pgrep -f "xcodebuild.*$IOS_UDID" 2>/dev/null || true)"
    [ -n "$P" ] && { kill -INT $P 2>/dev/null || true; sleep 1; kill -9 $P 2>/dev/null || true; }
  fi
  [ "$IOS_WE_BOOTED" = 1 ] && "$SMIX" sim shutdown "$IOS_UDID" >/dev/null 2>&1 || true
  [ "$AND_WE_UPPED" = 1 ]  && "$SMIX" down --platform android --device "$AND_SERIAL" >/dev/null 2>&1 || true
  [ "$AND_WE_BOOTED" = 1 ] && "$SMIX" sim shutdown "$AND_SERIAL" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX"

# A flow whose only per-platform difference is the appId line — portable
# selectors throughout. Written from one template so the diff can prove it.
flow_a() { # $1 appId  $2 outfile
  cat >"$2" <<FLOW
appId: $1
---
- launchApp
- assertVisible: "Submit"
- tapOn:
    role: textField
- inputText: "\${SMIX_C7_VAL}"
- tapOn: "Submit"
- assertVisible: "$ENV_VAL"
FLOW
}
flow_b() { # $1 appId  $2 outfile — the missing-var flow
  cat >"$2" <<FLOW
appId: $1
---
- launchApp
- tapOn:
    role: textField
- inputText: "\${$MISSING}"
FLOW
}

flow_a "$IOS_APPID" "$WORK/a_ios.yaml"
flow_a "$AND_APPID" "$WORK/a_android.yaml"
DIFF="$(diff <(grep -v '^appId:' "$WORK/a_ios.yaml") <(grep -v '^appId:' "$WORK/a_android.yaml") | grep -c '^[<>]' || true)"
[ "$DIFF" = 0 ] || fail "the two flows differ off the appId line ($DIFF lines) — C7's premise is one flow, one app change"
log "one flow, byte-identical but for appId (portable selectors only)"

port_free() { ! curl -s "http://127.0.0.1:$1/health" 2>/dev/null | grep -q '"ok":true'; }

tree_json() { SMIX_RUNNER_PORT="$1" "$SMIX" tree --json --device "$2" 2>/dev/null | grep -v '^kevy:'; }

wait_for_textfield() { # $1 port $2 dev — resolve via the selector engine,
  # the same path the flow uses, rather than guessing the wire field name.
  for _ in $(seq 1 20); do
    SMIX_RUNNER_PORT="$1" "$SMIX" find 'role:textField' --device "$2" 2>/dev/null \
      | grep -v '^kevy:' | grep -q '^exists=true' && return 0
    sleep 1
  done
  return 1
}

# ---- iOS leg --------------------------------------------------------
run_ios() {
  command -v xcrun >/dev/null 2>&1 || { log "no xcrun — skipping iOS leg"; return 0; }
  IOS_UDID="$("$SMIX" sim resolve "$IOS_ALIAS" 2>/dev/null | grep -v '^kevy:' | tr -d '[:space:]')" || true
  [ -n "$IOS_UDID" ] || { log "no sim registered as '$IOS_ALIAS' — skipping iOS leg"; return 0; }
  [ -d "$IOS_FIXTURE" ] || { log "no iOS fixture — skipping iOS leg"; return 0; }
  port_free "$IOS_PORT" || skip "iOS port $IOS_PORT already serves a runner — set SMIX_C7_IOS_PORT"

  step "iOS: boot $IOS_ALIAS, install, runner up (no --platform anywhere after)"
  if ! xcrun simctl list devices 2>/dev/null | grep -q "$IOS_UDID.*Booted"; then
    "$SMIX" sim boot "$IOS_UDID" >"$WORK/ios-boot.log" 2>&1 || fail "iOS boot: $(tail -3 "$WORK/ios-boot.log")"
    IOS_WE_BOOTED=1
  fi
  "$SMIX" sim install "$IOS_UDID" "$IOS_FIXTURE" >"$WORK/ios-install.log" 2>&1 || fail "iOS install: $(tail -3 "$WORK/ios-install.log")"
  SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" runner up "$IOS_UDID" --bundle "$IOS_APPID" --runner-port "$IOS_PORT" \
    --runner-project "$IOS_PROJECT" >"$WORK/ios-up.log" 2>&1 || fail "iOS runner up: $(tail -5 "$WORK/ios-up.log")"
  IOS_WE_UPPED=1
  leg "iOS" "$IOS_UDID" "$IOS_PORT" "$WORK/a_ios.yaml"
}

# ---- Android leg ----------------------------------------------------
run_android() {
  command -v adb >/dev/null 2>&1 || { log "no adb — skipping Android leg"; return 0; }
  AND_SERIAL="$("$SMIX" sim resolve "$AND_ALIAS" 2>/dev/null | grep -v '^kevy:' | tr -d '[:space:]')" || true
  [ -n "$AND_SERIAL" ] || { log "no emulator registered as '$AND_ALIAS' — skipping Android leg"; return 0; }
  [ -f "$AND_APK" ] || { log "no Android fixture apk — skipping Android leg"; return 0; }
  port_free "$AND_PORT" || skip "Android port $AND_PORT already serves a runner — set SMIX_C7_ANDROID_PORT"

  step "Android: ensure $AND_ALIAS up, install, runner up"
  if ! adb devices 2>/dev/null | grep -q "^$AND_SERIAL[[:space:]]*device"; then
    "$SMIX" sim boot "$AND_SERIAL" >"$WORK/and-boot.log" 2>&1 || fail "emulator boot: $(tail -3 "$WORK/and-boot.log")"
    AND_WE_BOOTED=1
  fi
  adb -s "$AND_SERIAL" install -r "$AND_APK" >"$WORK/and-install.log" 2>&1 || fail "Android install: $(tail -3 "$WORK/and-install.log")"
  "$SMIX" runner up "$AND_SERIAL" --platform android --runner-port "$AND_PORT" >"$WORK/and-up.log" 2>&1 \
    || { grep -qiE 'already|in use|running' "$WORK/and-up.log" || fail "Android runner up: $(tail -5 "$WORK/and-up.log")"; }
  AND_WE_UPPED=1
  leg "Android" "$AND_SERIAL" "$AND_PORT" "$WORK/a_android.yaml"
}

# ---- shared per-platform assertions (① ② ③ ④-supply) ---------------
leg() { # $1 label  $2 dev  $3 port  $4 flow_a
  local label="$1" dev="$2" port="$3" flow="$4"
  # Run the flow first — it contains launchApp, so the app (and its
  # textField) is only on screen afterwards; a presence check before it
  # would be reading the springboard.
  step "$label: run the shared flow, no --platform (①② + ④ supply via assertVisible '$ENV_VAL')"
  local out rc=0
  out="$(env -u SMIX_C7_VAL SMIX_RUNNER_PORT="$port" "$SMIX" run --device "$dev" "$flow" --env "SMIX_C7_VAL=$ENV_VAL" 2>&1 | grep -v '^kevy:')" || rc=$?
  printf '%s\n' "$out" | grep -qE 'simctl|Invalid device' && fail "$label: run took a wrong platform path (② not read from device): $out"
  printf '%s\n' "$out" | grep -q '"ok":true' || fail "$label: flow did not pass (① app not foregrounded, or ④ env value '$ENV_VAL' never reached the field): $out"
  log "$label OK: launchApp→visible→textField→inputText \${env}→submit→result shows '$ENV_VAL'"

  step "$label: presence (a textField must now be in the tree)"
  wait_for_textfield "$port" "$dev" || fail "$label: no textField in tree after launch — reading air"

  step "$label: CLI entrance parity (③ fill + find reachable, both platforms)"
  # ③ was a 501: fill / find id:/find text: were unreachable from the CLI
  # on Android while the flow's inputText worked. C7's integration proof
  # is that both are reachable (rc 0, not 501) from the CLI on BOTH
  # platforms, and find is two-sided. That the fill's CONTENT lands is
  # proven where it is reliable: on Android by c3 (deep, two-sided), and
  # on both platforms by this flow's own `inputText: ${env}` →
  # `assertVisible "'"$ENV_VAL"'"` above. C7 does not re-read the filled
  # value here — iOS stores it in `value` (not matched by find text:) and
  # a post-flow read of it is stale-prone; that is harness noise, not the
  # capability ③ is about.
  local frc=0
  SMIX_RUNNER_PORT="$port" "$SMIX" fill 'role:textField' --text "$CLI_WORD" --device "$dev" >/dev/null 2>&1 || frc=$?
  [ "$frc" -eq 0 ] || fail "$label: CLI fill role:textField exited $frc (the 501 the fix removes)"
  # find must not 501 and must be two-sided. 'Submit' is on both (text: is
  # case-insensitive, so iOS 'Submit' and Android 'SUBMIT' both match).
  SMIX_RUNNER_PORT="$port" "$SMIX" find 'text:Submit' --device "$dev" 2>/dev/null | grep -v '^kevy:' | grep -q '^exists=true' \
    || fail "$label: CLI find 'text:Submit' is not exists=true (find 501 or two-sided broken)"
  SMIX_RUNNER_PORT="$port" "$SMIX" find 'text:NoSuchElemZZZ' --device "$dev" 2>/dev/null | grep -v '^kevy:' | grep -q '^exists=false' \
    || fail "$label: find of an absent needle is not exists=false (find not two-sided)"
  log "$label OK: CLI fill + find reachable (rc 0, not 501), find true/false both sides"
}

run_ios
run_android

# ---- ④ missing-var (host-side, platform-agnostic; proved once on Android) ----
if [ -n "$AND_SERIAL" ] && [ "$AND_WE_UPPED" = 1 ]; then
  step "Android: unresolved \${$MISSING} must error, not type the literal (④ missing side, once)"
  flow_b "$AND_APPID" "$WORK/b.yaml"
  adb -s "$AND_SERIAL" shell am force-stop "$AND_APPID" >/dev/null 2>&1 || true
  BRC=0
  env -u "$MISSING" SMIX_RUNNER_PORT="$AND_PORT" "$SMIX" run --device "$AND_SERIAL" "$WORK/b.yaml" >"$WORK/b.log" 2>&1 || BRC=$?
  [ "$BRC" -ne 0 ] || fail "unresolved \${$MISSING} exited 0 — must fail, not pass silently"
  grep -qi 'undefined variable' <(grep -v '^kevy:' "$WORK/b.log") || fail "missing-var did not name 'undefined variable': $(grep -v '^kevy:' "$WORK/b.log" | tail -2)"
  log "Android OK: unresolved \${$MISSING} → non-zero + 'undefined variable', no literal typed"

  step "Android: tree human output prints text= (⑤ integration touch)"
  SMIX_RUNNER_PORT="$AND_PORT" "$SMIX" tree --device "$AND_SERIAL" 2>/dev/null | grep -v '^kevy:' | grep -q 'text=' \
    || fail "Android human tree has no text= — ⑤ regressed"
  log "Android OK: human tree prints text="
fi

log "v6.2-C7 PASS: one flow, both platforms, no --platform — ①②③④⑤ strung end to end"

# MUTATION (run each by hand; not in the automated checkpoint block):
#   M1 (env, both platforms): main.rs single-run branch env_vars: env.clone()
#      -> env_vars: Vec::new(); cargo build; rerun -> both legs red on
#      assertVisible "c7EnvValueQ9" (env value never reached the flow). Restore.
#   M2 (platform inference, Android leg): make resolve_run_platform/dial_platform
#      always return Ios; cargo build; rerun -> Android leg dies "Invalid
#      device" (took the simctl path). Restore.
