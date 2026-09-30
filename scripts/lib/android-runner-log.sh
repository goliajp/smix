#!/usr/bin/env bash
# What the Android runner recorded while a device gate drove it.
#
# The runner writes one line per request to the device log (tag
# `smix-route`): the route, its answer, how long it took and, for a route
# that keeps a time limit, how long each stage took and where it stopped.
# A gate that failed kept only the host's side — "the runner did not answer
# within 26 s" — and the device log that could say where the time went
# was gone by the time anyone looked. So a failing gate keeps it.
#
# Source this and call
#
#     since="$(android_device_now <serial>)"      # before driving
#     collect_android_runner_log <serial> "$since" <outfile>
#
# <outfile> gets a header naming the device and the window, then the
# runner's route lines, and the lines of any crash or test-runner failure
# on the device since then. It prints how many route lines it kept. When
# the log could not be read the file says so and the call returns 2:
# "the runner logged nothing" and "could not look" are different answers.
#
#     bash scripts/lib/android-runner-log.sh --selftest

# Both reads carry a deadline. A device that left mid-gate held
# `logcat -d` open for as long as anyone would wait — 106 minutes of a
# release run's tier, on an emulator that was no longer listed.
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/deadline.sh"
ANDROID_LOG_READ_SECS="${ANDROID_LOG_READ_SECS:-30}"

# The device's own clock, in epoch seconds with milliseconds: logcat's
# window is on the device's clock, which need not be the host's.
android_device_now() {
  with_deadline "$ANDROID_LOG_READ_SECS" adb -s "$1" shell 'echo $EPOCHREALTIME' 2>/dev/null \
    | tr -d '\r' | awk 'NF { print; exit }'
}

collect_android_runner_log() {
  local serial="$1" since="$2" out="$3" lines rc=0
  mkdir -p "$(dirname "$out")"
  printf 'device: %s\nsince: %s (device epoch)\n\n' "$serial" "${since:-unknown}" >"$out"
  if [ -z "$since" ]; then
    echo "could not read the device's clock when the gate began, so there is no window to read" >>"$out"
    echo 0
    return 2
  fi
  if ! lines="$(with_deadline "$ANDROID_LOG_READ_SECS" adb -s "$serial" logcat -d -v threadtime \
      -T "$since" -s smix-route:I AndroidRuntime:E TestRunner:* 2>&1)"; then
    printf 'could not read the device log (no answer within %s s, or adb refused):\n%s\n' \
      "$ANDROID_LOG_READ_SECS" "$lines" >>"$out"
    echo 0
    return 2
  fi
  printf '%s\n' "$lines" >>"$out"
  printf '%s\n' "$lines" | grep -c ' smix-route: ' || rc=$?
  # grep -c exits 1 on a count of zero, which is an answer, not a failure
  [ "$rc" -le 1 ] || return 2
  return 0
}

if [ "${BASH_SOURCE[0]}" = "$0" ] && [ "${1:-}" = "--selftest" ]; then
  set -u
  fails=0
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT
  mkdir -p "$work/bin"
  # a stand-in adb: records its arguments, answers from $FAKE_ADB
  cat >"$work/bin/adb" <<'FAKE'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$FAKE_ADB_ARGS"
case "$FAKE_ADB" in
  lines) printf '09-29 10:00:01.000  1  2 I smix-route: route=/clear-text status=200 tookMs=40\n09-29 10:00:02.000  1  2 E AndroidRuntime: FATAL EXCEPTION\n' ;;
  none) : ;;
  broken) echo "error: device 'emulator-5554' not found" >&2; exit 1 ;;
  hang) exec sleep 30 ;;
  clock) printf '1790000000.123456\r\n' ;;
esac
FAKE
  chmod +x "$work/bin/adb"
  check() {
    local name="$1" want_rc="$2" want_n="$3" must="$4" mode="$5" since="$6" n rc
    : >"$work/args"
    n="$(PATH="$work/bin:$PATH" FAKE_ADB="$mode" FAKE_ADB_ARGS="$work/args" \
      collect_android_runner_log emulator-5554 "$since" "$work/out.txt")" && rc=0 || rc=$?
    if [ "$rc" != "$want_rc" ] || [ "$n" != "$want_n" ] || ! grep -qF -- "$must" "$work/out.txt"; then
      echo "  - $name: rc=$rc (want $want_rc), n=$n (want $want_n), out:"
      sed 's/^/      /' "$work/out.txt"
      fails=$((fails + 1))
    fi
  }
  check "route lines are kept and counted" 0 1 "route=/clear-text status=200" lines 1790000000.123
  check "a crash on the device is kept" 0 1 "FATAL EXCEPTION" lines 1790000000.123
  check "a quiet runner is zero lines, not a failure" 0 0 "since: 1790000000.123" none 1790000000.123
  check "an unreadable log says so" 2 0 "could not read the device log" broken 1790000000.123
  check "no window is not a reading" 2 0 "no window to read" lines ""
  ANDROID_LOG_READ_SECS=1 check "a device that never answers ends the read" 2 0 "no answer within 1 s" hang 1790000000.123
  : >"$work/args"
  PATH="$work/bin:$PATH" FAKE_ADB=lines FAKE_ADB_ARGS="$work/args" \
    collect_android_runner_log emulator-5554 1790000000.123 "$work/out.txt" >/dev/null
  for want in "-s emulator-5554 logcat" "-d" "-T 1790000000.123" "smix-route:I" "AndroidRuntime:E"; do
    grep -qF -- "$want" "$work/args" || { echo "  - adb was not asked for '$want': $(cat "$work/args")"; fails=$((fails + 1)); }
  done
  got="$(PATH="$work/bin:$PATH" FAKE_ADB=clock FAKE_ADB_ARGS="$work/args" android_device_now emulator-5554)"
  [ "$got" = "1790000000.123456" ] || { echo "  - the device clock read as '$got'"; fails=$((fails + 1)); }
  if [ "$fails" -gt 0 ]; then
    echo "android-runner-log selftest: FAIL ($fails)" >&2
    exit 1
  fi
  echo "android-runner-log selftest: 12 checks"
fi
