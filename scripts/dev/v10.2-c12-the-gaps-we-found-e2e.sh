#!/usr/bin/env bash
#
# C12 — the gaps this cycle found in itself, on devices.
#
# Four things, none of them reported by a consumer. They were found
# while closing the consumer's round, written down, and would have
# stayed written down:
#
#   I1  `smix tree` read the accessibility projection while a flow on
#       the same screen read the semantics tree. Two e2e scripts in this
#       cycle had to bypass the CLI to see what the flow was seeing.
#   I2  `launchApp: { clearState: true }` failed on Xcode 27 — it
#       started `/bin/rm` inside the simulator, and the runtime root has
#       no `rm`. It never needed to: the container is a host directory.
#   L1  `launchApp.permissions` was typed as the iOS permission enum the
#       whole way down, so `storage` — implemented and tested in the
#       Android backend — could not be named from a flow.
#   F1  a simulated location outlived the flow that set it and smix had
#       no way to put it back.
#
# Each verdict reads the device's own account, never smix's echo of it:
# the app's container on disk for the wipe, `dumpsys package` for the
# grant, and the tree's `source` field for which reader answered. The
# two that cannot be read back say so in their own line rather than
# borrowing a neighbour's evidence — `devicectl` and `simctl` both
# answer "cleared" with nothing simulating, so what is asserted there is
# that the verb exists and the device accepted it.
#
# There is no SKIP path. A missing simulator, a missing emulator, a
# missing fixture are all reasons this cannot judge anything, and a gate
# that cannot judge must be red rather than quiet (open-items I4).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/e2e-binary.sh
source "$ROOT/scripts/lib/e2e-binary.sh"
IOS_UDID="${1:-${SMIX_E2E_UDID:-}}"
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/e2e-devices.sh"
AND_ALIAS="${SMIX_C12_ANDROID:-$E2E_ANDROID}"
IOS_APPID="jp.golia.smix.fixture"
AND_APPID="dev.smix.fixture"
IOS_FIXTURE="$ROOT/test-fixtures/demo-app/build/SmixFixture.app"
AND_APK="$ROOT/test-fixtures/android-app/app/build/outputs/apk/debug/app-debug.apk"
# shellcheck source=../lib/gate-port.sh
source "$ROOT/scripts/lib/gate-port.sh"
AND_PORT="$SMIX_RUNNER_PORT"
gate_free_port IOS_PORT
IOS_PROJECT="$ROOT/swift-bridge/SmixRunner.xcodeproj"
WORK="$(mktemp -d)"

log()  { printf '[c12-gaps] %s\n' "$*" >&2; }
step() { printf '[c12-gaps] --- %s\n' "$*" >&2; }
fail() { printf '[c12-gaps] FAIL: %s\n' "$*" >&2; exit 1; }

AND_SERIAL="" AND_WE_BOOTED=0 AND_WE_UPPED=0 IOS_WE_BOOTED=0 IOS_WE_UPPED=0
cleanup() {
  # The location goes back first, before anything that could end the
  # script early. It is the one thing here that outlives the run.
  if [ -n "$IOS_UDID" ]; then
    xcrun simctl location "$IOS_UDID" clear >/dev/null 2>&1 || true
  fi
  if [ "$IOS_WE_UPPED" = 1 ]; then
    "$SMIX" runner down --device "$IOS_UDID" --runner-port "$IOS_PORT" \
      >/dev/null 2>&1 || log "warning: the iOS runner was not stopped"
  fi
  if [ "$AND_WE_UPPED" = 1 ]; then
    "$SMIX" runner down --platform android --device "$AND_SERIAL" --runner-port "$AND_PORT" \
      >/dev/null 2>&1 || log "warning: the Android runner was not stopped"
  fi
  if [ "$AND_WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$AND_SERIAL" >/dev/null 2>&1 || true; fi
  if [ "$IOS_WE_BOOTED" = 1 ]; then "$SMIX" sim shutdown "$IOS_UDID" >/dev/null 2>&1 || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

[ -x "$SMIX" ] || fail "no smix binary at $SMIX (cargo build -p smix-cli)"
[ -n "$IOS_UDID" ] || fail "no iOS simulator UDID given (argument 1, or SMIX_E2E_UDID)"

# ---- iOS: the sandbox wipe, and the way back from a location --------
run_ios() {
  step "iOS: $IOS_UDID"
  [ -d "$IOS_FIXTURE" ] || fail "no iOS fixture at $IOS_FIXTURE (build it first)"
  if ! xcrun simctl list devices | grep -q "$IOS_UDID (Booted)"; then
    "$SMIX" sim boot "$IOS_UDID" >/dev/null 2>&1 || fail "could not boot $IOS_UDID"
    IOS_WE_BOOTED=1
  fi
  "$SMIX" sim install "$IOS_UDID" "$IOS_FIXTURE" >/dev/null 2>&1 \
    || fail "could not install the iOS fixture"

  # A file the app itself would have written. `clearState` promises the
  # sandbox is empty afterwards, and an empty sandbox that was already
  # empty proves nothing — so something has to be in it first.
  local container
  container="$(xcrun simctl get_app_container "$IOS_UDID" "$IOS_APPID" data 2>/dev/null)" \
    || fail "could not locate the fixture's data container"
  mkdir -p "$container/Documents"
  printf 'written before clearState\n' > "$container/Documents/c12-witness.txt"
  [ -f "$container/Documents/c12-witness.txt" ] || fail "could not seed the sandbox"

  SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" runner up "$IOS_UDID" --bundle "$IOS_APPID" \
    --runner-port "$IOS_PORT" --runner-project "$IOS_PROJECT" >"$WORK/ios-up.log" 2>&1 \
    || fail "the iOS runner would not start: $(tail -5 "$WORK/ios-up.log")"
  IOS_WE_UPPED=1

  cat >"$WORK/ios.yaml" <<FLOW
appId: $IOS_APPID
---
- launchApp:
    appId: $IOS_APPID
    clearState: true
- setLocation:
    latitude: 35.6812
    longitude: 139.7671
- clearLocation
FLOW

  # The location is set in the same flow, so `clearLocation` has
  # something to undo and both verbs are driven the way a consumer
  # drives them. A setup done outside smix would leave the question of
  # whether the verb exists unasked.
  local out status=0
  out="$(SMIX_RUNNER_PORT="$IOS_PORT" "$SMIX" run --device "$IOS_UDID" "$WORK/ios.yaml" 2>&1)" || status=$?
  out="$(printf '%s\n' "$out" || true)"
  if [ "$status" -ne 0 ]; then
    printf '%s\n' "$out" | tail -20 | sed 's/^/[c12-gaps]   /' >&2
    fail "the iOS flow did not run (exit $status)"
  fi

  if [ -f "$container/Documents/c12-witness.txt" ]; then
    fail "clear-state=kept — the file written before clearState is still there ($container)"
  fi
  log "  clear-state=cleared    (the seeded file is gone from $(basename "$container"))"
  log "  location-clear=accepted (no host-side read-back exists; the verb ran and the device took it)"
}

# ---- Android: a permission only Android has, and one tree -----------
run_android() {
  AND_SERIAL="$("$SMIX" sim resolve "$AND_ALIAS" 2>/dev/null | tail -1)"
  [ -n "$AND_SERIAL" ] || fail "nothing is registered as $AND_ALIAS"
  # Never a phone: this grants and revokes a permission and installs an
  # app, none of which belongs on somebody's own handset.
  case "$AND_SERIAL" in
    emulator-[0-9]*) ;;
    *) fail "$AND_ALIAS resolves to $AND_SERIAL, which is not an emulator — refusing" ;;
  esac
  step "Android: $AND_ALIAS ($AND_SERIAL)"
  [ -f "$AND_APK" ] || fail "no fixture apk at $AND_APK (assembleDebug in test-fixtures/android-app)"
  # And the one THESE sources build: the path existing says a build
  # happened, not which sources it happened over (open-items O1).
  python3 "$ROOT/scripts/dev/fixture-apk-stamp.py" --check >&2 \
    || fail "the fixture apk on disk is not the one this tree builds"

  if ! adb -s "$AND_SERIAL" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
    "$SMIX" sim boot "$AND_ALIAS" >/dev/null 2>&1 || fail "could not boot $AND_ALIAS"
    AND_WE_BOOTED=1
    adb -s "$AND_SERIAL" wait-for-device
  fi
  "$SMIX" sim install "$AND_ALIAS" "$AND_APK" >/dev/null 2>&1 \
    || fail "could not install the fixture apk"

  # Revoked first. A permission that was already granted would let a
  # flow that asked for nothing pass this.
  adb -s "$AND_SERIAL" shell pm revoke "$AND_APPID" android.permission.WRITE_EXTERNAL_STORAGE \
    >/dev/null 2>&1 || true
  local before
  before="$(adb -s "$AND_SERIAL" shell dumpsys package "$AND_APPID" \
    | grep "WRITE_EXTERNAL_STORAGE: granted=" | head -1 || true)"
  case "$before" in
    *"granted=true"*) fail "storage-permission=already-granted — the revoke did not take, so a grant proves nothing" ;;
  esac

  # The runner first: every flow below goes through it.
  "$SMIX" runner up "$AND_ALIAS" --platform android --runner-port "$AND_PORT" >"$WORK/and-up.log" 2>&1 \
    || fail "the Android runner would not start on $AND_SERIAL:$AND_PORT: $(tail -5 "$WORK/and-up.log")"
  AND_WE_UPPED=1

  cat >"$WORK/and.yaml" <<FLOW
appId: $AND_APPID
---
- launchApp:
    appId: $AND_APPID
    permissions:
      storage: allow
FLOW
  local out status=0
  out="$(SMIX_RUNNER_PORT="$AND_PORT" "$SMIX" run --device "$AND_SERIAL" "$WORK/and.yaml" 2>&1)" || status=$?
  out="$(printf '%s\n' "$out" || true)"
  if [ "$status" -ne 0 ]; then
    printf '%s\n' "$out" | tail -20 | sed 's/^/[c12-gaps]   /' >&2
    fail "the Android flow did not run (exit $status)"
  fi

  local after
  after="$(adb -s "$AND_SERIAL" shell dumpsys package "$AND_APPID" \
    | grep "WRITE_EXTERNAL_STORAGE: granted=" | head -1 || true)"
  case "$after" in
    *"granted=true"*) log "  storage-permission=granted (device says: $(printf '%s' "$after" | tr -s ' '))" ;;
    "") fail "storage-permission=unknown — the package manager says nothing about WRITE_EXTERNAL_STORAGE" ;;
    *) fail "storage-permission=denied — the flow asked and the device says: $(printf '%s' "$after" | tr -s ' ')" ;;
  esac

  # A screen with Compose on it. The probe answers `present: true` for
  # any screen of an app that carries it, but a screen with no Compose
  # root has no semantics tree to hand back — and falling through to
  # the accessibility reader there is right, not a defect. `.MainActivity`
  # is View-based (measured: `roots: 0`), so it would fail this for a
  # reason that has nothing to do with what is being judged.
  adb -s "$AND_SERIAL" shell am start -n "$AND_APPID/.InteropActivity" >/dev/null 2>&1 \
    || fail "could not bring the Compose screen to the front"

  # Polled: the activity is resumed before Compose has composed, and a
  # tree read in that gap is of a screen that has not arrived.
  local source=""
  for _ in $(seq 1 30); do
    "$SMIX" tree --json --device "$AND_SERIAL" --port "$AND_PORT" 2>/dev/null \
 > "$WORK/tree.json" || true
    if head -c 1 "$WORK/tree.json" | grep -q '{'; then
      source="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["source"])' "$WORK/tree.json" 2>/dev/null || true)"
      [ "$source" = semantics ] && break
    fi
    sleep 0.5
  done
  if [ "$source" != semantics ]; then
    fail "tree-via-cli=$source — the CLI named no app and was given the accessibility tree; a flow on this screen reads the semantics one"
  fi
  log "  tree-via-cli=semantics  (no bundle named; the runner answered about the window in front)"
}

run_ios
run_android

log "C12-GAPS-E2E-PASS on $IOS_UDID and $AND_SERIAL"
