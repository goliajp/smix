#!/usr/bin/env bash
# Every device e2e, in one run, with the count of what actually ran.
#
# `preflight.sh` already loops `scripts/dev/*-e2e.sh` under
# SMIX_DEVICE_E2E, so the list has never been the problem — it is
# derived from a glob and a new script is inside the gate the day it
# lands. What the loop could not tell you is how many of those scripts
# did anything.
#
# Fourteen of the twenty-nine skip when their device is not named, and
# they each want a differently-named variable for the same thing:
# SMIX_C3_SIM, SMIX_C4_SIM, SMIX_C8_SIM, SMIX_CROSSAPP_E2E_UDID,
# SMIX_E2E_DEVICE, and so on. Run the loop without setting all of them
# and most scripts skip, the loop exits 0, and a run that drove almost
# nothing reports success.
#
# That shape has cost this cycle repeatedly: a classifier answering
# NORECORD twenty-one times while the gate printed GREEN, a settle
# reading "could not see" as "arrived". A measurement that failed to
# happen must not look like a measurement that passed.
#
# So this counts. All-skipped is a failure, and the summary names every
# script that skipped and the variable it was waiting for.
#
# Usage:
#   SMIX_E2E_UDID=<UDID> SMIX_BIN=<path> bash scripts/release/device-e2e-tier.sh
#   bash scripts/release/device-e2e-tier.sh --selftest
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# What one script's exit code says it did.
#
# It used to be read out of the output — exit 0 with the word SKIP
# anywhere in it counted as a skip. Two things wrong with that: a script
# that drove everything and passed counts as skipped if its own log
# happens to print the word, and a script that could not judge could not
# say so in the only channel that is not prose. The scripts now answer
# 0 drove / 1 failed / 2 could not judge, and this reads that.
e2e_state() {
  case "$1" in
    0) echo drove ;;
    2) echo skip ;;
    *) echo fail ;;
  esac
}

# The verdict over a run: how many drove, skipped, failed.
#
# Separated from the running so it can be checked without a simulator.
# Reads lines on stdin, one script per line: `<name> <drove|skip|fail>`.
e2e_verdict() {
  local drove=0 skipped=0 failed=0
  local -a skips=() fails=()
  local name state
  while read -r name state; do
    [ -z "$name" ] && continue
    case "$state" in
      drove) drove=$((drove + 1)) ;;
      skip)  skipped=$((skipped + 1)); skips+=("$name") ;;
      fail)  failed=$((failed + 1)); fails+=("$name") ;;
    esac
  done

  local total=$((drove + skipped + failed))
  if [ "$total" -eq 0 ]; then
    echo "device-e2e-tier: NOTHING DRIVEN — no scripts were run at all"
    return 1
  fi

  for f in "${fails[@]+"${fails[@]}"}"; do echo "  - FAIL $f"; done
  for s in "${skips[@]+"${skips[@]}"}"; do echo "  - skip $s"; done

  if [ "$drove" -eq 0 ]; then
    # The case this script exists for. Every script skipped, every one
    # exited 0, and the loop that ran them would have reported success
    # while driving nothing.
    echo "device-e2e-tier: NOTHING DRIVEN — $skipped skipped, $failed failed of $total"
    return 1
  fi
  if [ "$failed" -gt 0 ]; then
    echo "device-e2e-tier: FAILED — $drove drove, $skipped skipped, $failed failed of $total"
    return 1
  fi
  echo "device-e2e-tier: DROVE $drove/$total — $skipped skipped"
  return 0
}

# e2e_count_check <listed> <results> — every script placed in a lane
# reported back. Lanes run in the background, and one that died would
# otherwise leave its scripts out of the verdict as if they had never
# been listed.
e2e_count_check() {
  local listed="$1" ran
  ran="$(printf '%s\n' "$2" | grep -c . || true)"
  if [ "$ran" != "$listed" ]; then
    echo "device-e2e-tier: FAILED — $listed scripts placed in lanes, $ran reported"
    return 1
  fi
}

# shellcheck source=../lib/e2e-devices.sh
. "$ROOT/scripts/lib/e2e-devices.sh"
# shellcheck source=../lib/android-runner-log.sh
. "$ROOT/scripts/lib/android-runner-log.sh"

if [ "${1:-}" = "--selftest" ]; then
  fails=0
  check() { # label expected-exit expected-substring input
    local label="$1" want_rc="$2" want_sub="$3" input="$4"
    local out rc
    out="$(printf '%s\n' "$input" | e2e_verdict)" && rc=0 || rc=$?
    if [ "$rc" -ne "$want_rc" ]; then
      echo "device-e2e-tier selftest: $label — exit $rc, wanted $want_rc" >&2
      fails=$((fails + 1))
    fi
    case "$out" in
      *"$want_sub"*) ;;
      *) echo "device-e2e-tier selftest: $label — output lacks '$want_sub': $out" >&2
         fails=$((fails + 1)) ;;
    esac
  }

  check "everything drove" 0 "DROVE 3/3" "$(printf 'a drove\nb drove\nc drove')"
  check "some skipped, rest drove" 0 "DROVE 2/3" "$(printf 'a drove\nb skip\nc drove')"
  # The one this exists for.
  check "everything skipped" 1 "NOTHING DRIVEN" "$(printf 'a skip\nb skip\nc skip')"
  check "a failure is not hidden by skips" 1 "FAIL b" "$(printf 'a skip\nb fail\nc skip')"
  check "a skipped script is named" 0 "skip b" "$(printf 'a drove\nb skip')"
  check "nothing at all" 1 "NOTHING DRIVEN" ""
  count_check() { # label expected-exit listed results
    local out rc
    out="$(e2e_count_check "$3" "$4")" && rc=0 || rc=$?
    if [ "$rc" -ne "$2" ]; then
      echo "device-e2e-tier selftest: $1 — exit $rc, wanted $2: $out" >&2
      fails=$((fails + 1))
    fi
  }
  count_check "every placed script reported" 0 3 "$(printf 'a drove\nb drove\nc skip')"
  # A lane that died in the background left its scripts out entirely.
  count_check "a lane that died took scripts with it" 1 3 "$(printf 'a drove\nb drove')"

  # The classification itself, which is where the reading used to be
  # wrong. Each of these was a real misreading: a passing script whose
  # log mentions SKIP counted as skipped, and a script that could not
  # judge had no way to say so.
  state_check() { # label rc expected
    local got
    got="$(e2e_state "$2")"
    if [ "$got" != "$3" ]; then
      echo "device-e2e-tier selftest: $1 — exit $2 read as $got, wanted $3" >&2
      fails=$((fails + 1))
    fi
  }
  state_check "a script that drove and passed" 0 drove
  state_check "a script that could not judge" 2 skip
  state_check "a script that failed" 1 fail
  state_check "a script killed by a signal" 143 fail

  # Whether the Android device can show an app at all. Each input is what
  # the device reported on 2026-09-26 or the healthy baseline.
  screen_check() { # label expected-substring(empty = healthy) wake keyguard systemui anr
    local got
    got="$(e2e_android_screen_problem "$3" "$4" "$5" "$6")"
    if [ -z "$2" ] && [ -n "$got" ]; then
      echo "device-e2e-tier selftest: $1 — a healthy screen was called '$got'" >&2
      fails=$((fails + 1))
    elif [ -n "$2" ]; then
      case "$got" in
        *"$2"*) ;;
        *) echo "device-e2e-tier selftest: $1 — said '$got', wanted '$2'" >&2
           fails=$((fails + 1)) ;;
      esac
    fi
  }
  screen_check "a healthy device" "" Awake false 734 ""
  screen_check "system UI gone" "system UI is not running" Awake true "" ""
  screen_check "a not-responding dialog" "com.android.systemui isn't responding" Awake false 734 com.android.systemui
  screen_check "display off" "display is off" Asleep false 734 ""
  screen_check "lock screen" "lock screen is showing" Awake true 734 ""
  screen_check "nothing could be read" "could not read" "" "" "" ""

  if [ "$fails" -ne 0 ]; then
    echo "device-e2e-tier selftest: FAIL ($fails)" >&2
    exit 1
  fi
  echo "device-e2e-tier selftest: 18 cases pass — a count, every placed script reported, the verdict, what each exit code means, and whether a screen can show an app"
  exit 0
fi

: "${SMIX_E2E_UDID:?set SMIX_E2E_UDID to the simulator the scripts should drive}"

# A tier never runs a physical leg, whatever is plugged in and whatever
# the caller's environment says: a phone is used when a person runs that
# one script and names it. On 2026-09-25 this loop reached the owner's
# Samsung and iPhone because being attached and registered was all the
# scripts asked. Guarded by `an-e2e-leaves-the-phones-alone`.
unset SMIX_E2E_PHYSICAL_ANDROID SMIX_E2E_PHYSICAL_IOS SMIX_E2E_PHYSICAL_IOS_PORT

# shellcheck source=../lib/e2e-binary.sh
. "$ROOT/scripts/lib/e2e-binary.sh"

# Whether the Android device the scripts share can show an app. A device
# that is not running is not checked: the scripts boot it themselves.
android_problem() {
  local serial
  serial="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tail -1)" || return 0
  adb devices 2>/dev/null | grep -qE "^${serial}[[:space:]]+device" || return 0
  e2e_android_screen_problem_of "$serial" | sed "s|^|$E2E_ANDROID ($serial): |"
}

problem="$(android_problem)"
if [ -n "$problem" ]; then
  echo "device-e2e-tier: STOPPED before the first script — $problem"
  exit 1
fi

# The shared emulator's serial while it is up, else nothing.
android_serial() {
  local serial
  serial="$("$SMIX" sim resolve "$E2E_ANDROID" 2>/dev/null | tail -1)" || return 0
  adb devices 2>/dev/null | grep -qE "^${serial}[[:space:]]+device" && echo "$serial"
  return 0
}

# run_one <script> [<android-serial>] — run one script, log it, print
# `<name> <state>`. With a serial, a script that failed also keeps what the
# Android runner logged while it ran, beside its own log.
run_one() {
  local e2e="$1" serial="${2:-}" name out rc state log since="" n
  name="$(basename "$e2e" .sh)"
  log="/tmp/device-e2e-$name.log"
  echo "device-e2e-tier: [$name] running..." >&2
  [ -n "$serial" ] && since="$(android_device_now "$serial" || true)"
  out="$(bash "$e2e" 2>&1)" && rc=0 || rc=$?
  printf '%s\n' "$out" > "$log"
  state="$(e2e_state "$rc")"
  echo "device-e2e-tier: [$name] $state" >&2
  if [ "$state" = fail ] && [ -n "$serial" ]; then
    n="$(collect_android_runner_log "$serial" "$since" "${log%.log}-runner.log" || true)"
    echo "device-e2e-tier: [$name] the Android runner's log ($n route line(s)): ${log%.log}-runner.log" >&2
  fi
  echo "$name $state"
}

# run_lane <lane> <list-file> — run a lane's scripts in turn, printing one
# `<name> <state>` per script. A lane that touches the Android device stops
# at the first failure that turns out to be the device no longer able to
# show anything: every script after it would fail about its own step.
run_lane() {
  local lane="$1" list="$2" e2e line problem serial
  while read -r e2e <&3; do
    [ -z "$e2e" ] && continue
    serial=""
    if [ "$lane" = android ] || [ "$lane" = serial ]; then
      serial="$(android_serial)"
    fi
    line="$(run_one "$e2e" "$serial" </dev/null)"
    echo "$line"
    if { [ "$lane" = android ] || [ "$lane" = serial ]; } && [ "${line##* }" = fail ]; then
      problem="$(android_problem)"
      if [ -n "$problem" ]; then
        echo "STOPPED ${line% *} $problem"
        return 1
      fi
    fi
  done 3< "$list"
}

main_rc=0
# Three lanes that share no device run side by side — the simulator's,
# the emulator's, and the scripts that drive no device at all; scripts
# that drive both platforms, or act on the whole machine, run after
# them, alone.
# Which lane a script is in is read from the script (e2e-lanes.py).
lanes_dir="$(mktemp -d)"
python3 "$ROOT/scripts/dev/e2e-lanes.py" > "$lanes_dir/all" \
  || { echo "device-e2e-tier: FAILED — could not place the scripts in lanes"; exit 1; }
for lane in ios android none serial; do
  awk -F'\t' -v l="$lane" '$1 == l { print $2 }' "$lanes_dir/all" > "$lanes_dir/$lane.list"
done
n_listed="$(grep -c . "$lanes_dir/all")"

# With job control on. A shell without it starts every background job
# with SIGINT and SIGQUIT ignored, and a script cannot undo an ignore
# it inherited — so a script that sends its own process group a Ctrl-C
# (c9k) watched it go nowhere, but only when it ran in a lane.
set -m
run_lane ios "$lanes_dir/ios.list" > "$lanes_dir/ios.res" & ios_pid=$!
run_lane android "$lanes_dir/android.list" > "$lanes_dir/android.res" & android_pid=$!
run_lane none "$lanes_dir/none.list" > "$lanes_dir/none.res" & none_pid=$!
set +m
wait "$ios_pid" || true
wait "$android_pid" || true
wait "$none_pid" || true
if ! grep -q '^STOPPED ' "$lanes_dir/android.res"; then
  run_lane serial "$lanes_dir/serial.list" > "$lanes_dir/serial.res" || true
else
  : > "$lanes_dir/serial.res"
fi

results="$(cat "$lanes_dir/ios.res" "$lanes_dir/android.res" "$lanes_dir/none.res" "$lanes_dir/serial.res" | grep -v '^STOPPED ' || true)"
stopped="$(cat "$lanes_dir/android.res" "$lanes_dir/serial.res" | grep '^STOPPED ' | head -1 || true)"
if [ -n "$stopped" ]; then
  printf '%s\n' "$results" | e2e_verdict || true
  set -- $stopped
  shift
  name="$1"; shift
  echo "device-e2e-tier: STOPPED after [$name] — $*"
  exit 1
fi
e2e_count_check "$n_listed" "$results" || main_rc=1
printf '%s\n' "$results" | e2e_verdict || main_rc=1

[ "$main_rc" = 0 ]
