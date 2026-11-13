#!/usr/bin/env bash
#
# Exercises lifecycle-guard against the commands it exists to refuse, in
# both of its modes: judged by the smix binary (`smix guard lifecycle`),
# and the coarse copy it falls back to when no smix on PATH can judge.
#
# Set SMIX_BIN to the smix to judge with; default is this checkout's
# debug build. The UDID is a placeholder: nothing here reaches a device.

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
GUARD="$ROOT/plugin/scripts/lifecycle-guard.sh"
SMIX_BIN="${SMIX_BIN:-$ROOT/target/debug/smix}"
UDID="A1B2C3D4-1111-2222-3333-444455556666"
[ -x "$SMIX_BIN" ] || { echo "no smix at $SMIX_BIN — cargo build -p smix-cli first" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin" "$work/machine"
ln -s "$SMIX_BIN" "$work/bin/smix"
WITH_SMIX="$work/bin:/usr/bin:/bin"
WITHOUT_SMIX="/usr/bin:/bin"

fails=0
cases=0
feed() {
  local path="$1" want="$2" cmd="$3" json got
  json="$(python3 -c 'import json,sys; print(json.dumps({"tool_input":{"command":sys.argv[1]}}))' "$cmd")"
  printf '%s' "$json" | PATH="$path" SMIX_MACHINE_DIR="$work/machine" bash "$GUARD" >/dev/null 2>&1
  got=$?
  cases=$((cases + 1))
  if [ "$got" != "$want" ]; then
    echo "FAIL ($([ "$path" = "$WITH_SMIX" ] && echo smix || echo fallback)) want=$want got=$got: $cmd"
    fails=$((fails + 1))
  fi
}

BLOCK=2
ALLOW=0
refused=(
  "emulator -avd qip-test-01 -port 5560 -no-snapshot-save -no-audio -no-boot-anim"
  "\$ANDROID_HOME/emulator/emulator @sim-android-01"
  "cd /x && nohup emulator -avd a > /dev/null 2>&1 &"
  "/sdk/emulator/qemu/darwin-aarch64/qemu-system-aarch64 -avd phone"
  "adb -s emulator-5554 emu kill"
  "avdmanager create avd -n x -k img"
  "xcrun simctl boot $UDID"
  "xcrun simctl shutdown $UDID"
  "xcrun simctl erase $UDID"
  "xcrun simctl create t 'iPhone 17' rt"
  "xcrun simctl delete unavailable"
)
allowed=(
  "xcrun simctl list devices"
  "xcrun simctl bootstatus $UDID -b"
  "adb devices"
  "emulator -list-avds"
  "avdmanager list avd"
  "smix sim boot qip-test-01 --unlock-pin 1234"
  "smix sim exec $UDID boot"
  "ls -la"
)
for path in "$WITH_SMIX" "$WITHOUT_SMIX"; do
  for c in "${refused[@]}"; do feed "$path" $BLOCK "$c"; done
  for c in "${allowed[@]}"; do feed "$path" $ALLOW "$c"; done
  feed "$path" $ALLOW "SMIX_FENCE_EXCEPTION=\"wedged AVD\" emulator -avd a -wipe-data"
done
# only the binary reads text precisely enough to let these through
feed "$WITH_SMIX" $ALLOW "grep -rn 'simctl boot' crates/"
feed "$WITH_SMIX" $ALLOW "git commit -m 'refuse emulator -avd'"
feed "$WITH_SMIX" $BLOCK "SMIX_FENCE_EXCEPTION= emulator -avd a"
feed "$WITH_SMIX" $BLOCK "bash -c 'xcrun simctl boot $UDID'"

# the exception that passed with smix present was written down, once
logged="$(grep -c '"reason":"wedged AVD"' "$work/machine/fence-exceptions.jsonl" 2>/dev/null || true)"
cases=$((cases + 1))
if [ "$logged" != "1" ]; then
  echo "FAIL the exception was logged ${logged:-0} times, want 1"
  fails=$((fails + 1))
fi

echo "lifecycle-guard: $cases cases, $fails failed"
[ "$fails" -eq 0 ]
