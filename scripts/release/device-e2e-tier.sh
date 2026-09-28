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
# `$1`, when given, names the leg ("on API 36"): it follows the verdict and
# each named script, so the two legs cannot be read as one.
e2e_verdict() {
  local leg="${1:-}" tag=""
  [ -n "$leg" ] && tag=" (${leg#on })"
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
    echo "device-e2e-tier: NOTHING DRIVEN${leg:+ $leg} — no scripts were run at all"
    return 1
  fi

  for f in "${fails[@]+"${fails[@]}"}"; do echo "  - FAIL $f$tag"; done
  for s in "${skips[@]+"${skips[@]}"}"; do echo "  - skip $s$tag"; done

  if [ "$drove" -eq 0 ]; then
    # The case this script exists for. Every script skipped, every one
    # exited 0, and the loop that ran them would have reported success
    # while driving nothing.
    echo "device-e2e-tier: NOTHING DRIVEN${leg:+ $leg} — $skipped skipped, $failed failed of $total"
    return 1
  fi
  if [ "$failed" -gt 0 ]; then
    echo "device-e2e-tier: FAILED${leg:+ $leg} — $drove drove, $skipped skipped, $failed failed of $total"
    return 1
  fi
  echo "device-e2e-tier: DROVE $drove/$total${leg:+ $leg} — $skipped skipped"
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

# The scripts run a second time on API 36, after the main leg.
#
# The main leg's Android device is API 33, and until 2026-09-27 no gate
# had run on anything newer. A consumer drives API 36 (google_apis with
# Gboard), where the system draws apps edge to edge and the keyboard is a
# window of its own, and the first run there found a tap that the status
# bar took. These are the scripts whose subject changes with the API
# level: keyboard, windows, where a tap lands, typing, a watch for a
# flash. Each has an Android leg and takes its device from E2E_ANDROID,
# except v2.14-c2, which takes the serial (SMIX_ANDROID_SERIAL).
API36_E2E="v11.0-c8-something-appeared-between-e2e
v11.0-c9e-an-empty-field-holds-nothing-e2e
v11.0-c9h-focus-after-enter-e2e
v11.0-c9i-keyboard-beside-the-probe-e2e
v11.0-c9m-a-field-that-submits-itself-e2e
v2.14-c2-android-clear-e2e"

# api36_list_check [list] — every name on the list is a script here that
# can be pointed at another Android device. A renamed script would
# otherwise drop out of the leg in silence, which is the departure a walk
# over the list cannot see.
api36_list_check() {
  local list="${1:-$API36_E2E}" name f bad=0 n=0
  while read -r name; do
    [ -z "$name" ] && continue
    n=$((n + 1))
    f="$ROOT/scripts/dev/$name.sh"
    if [ ! -f "$f" ]; then
      echo "API 36 list names $name, and there is no scripts/dev/$name.sh"
      bad=1
    elif ! grep -qE 'E2E_ANDROID|SMIX_ANDROID_SERIAL' "$f"; then
      echo "API 36 list names $name, which takes its Android device from neither E2E_ANDROID nor SMIX_ANDROID_SERIAL"
      bad=1
    fi
  done <<< "$list"
  [ "$n" -gt 0 ] || { echo "API 36 list is empty"; return 1; }
  [ "$bad" = 0 ] || return 1
  echo "API 36 list is whole — $n scripts"
}

# shellcheck source=../lib/e2e-devices.sh
. "$ROOT/scripts/lib/e2e-devices.sh"

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
  # The API 36 leg is counted on its own, under its own name, by the same
  # rules: a leg that skipped everything drove nothing on API 36, however
  # green the main leg was.
  check36() { # label expected-exit expected-substring input
    local out rc
    out="$(printf '%s\n' "$4" | e2e_verdict "on API 36")" && rc=0 || rc=$?
    if [ "$rc" -ne "$2" ]; then
      echo "device-e2e-tier selftest: $1 — exit $rc, wanted $2" >&2
      fails=$((fails + 1))
    fi
    case "$out" in
      *"$3"*) ;;
      *) echo "device-e2e-tier selftest: $1 — output lacks '$3': $out" >&2
         fails=$((fails + 1)) ;;
    esac
  }
  check36 "API 36: everything drove" 0 "DROVE 2/2 on API 36" "$(printf 'a drove\nb drove')"
  check36 "API 36: everything skipped" 1 "NOTHING DRIVEN on API 36" "$(printf 'a skip\nb skip')"
  check36 "API 36: one failed" 1 "FAILED on API 36" "$(printf 'a drove\nb fail')"
  check36 "API 36: the failure is named" 1 "FAIL b (API 36)" "$(printf 'a drove\nb fail')"
  list_check() { # label expected-exit expected-substring [list]
    local out rc
    out="$(api36_list_check "${4:-}")" && rc=0 || rc=$?
    if [ "$rc" -ne "$2" ]; then
      echo "device-e2e-tier selftest: $1 — exit $rc, wanted $2: $out" >&2
      fails=$((fails + 1))
    fi
    case "$out" in
      *"$3"*) ;;
      *) echo "device-e2e-tier selftest: $1 — output lacks '$3': $out" >&2
         fails=$((fails + 1)) ;;
    esac
  }
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
  list_check "the API 36 list as written" 0 "API 36 list is whole"
  list_check "a name with no script" 1 "no scripts/dev/v0-gone-e2e.sh" "v0-gone-e2e"
  list_check "a script that cannot be pointed at API 36" 1 "neither E2E_ANDROID" "v2.7-c1-tap-hit-e2e"

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
  echo "device-e2e-tier selftest: 25 cases pass — a count, every placed script reported, a verdict for each leg, the API 36 list, what each exit code means, and whether a screen can show an app"
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

# run_one <script> <leg-label> — run one script, log it, print `<name> <state>`.
run_one() {
  local e2e="$1" leg="$2" name out rc state log
  name="$(basename "$e2e" .sh)"
  log="/tmp/device-e2e-$name${leg:+-api36}.log"
  echo "device-e2e-tier: [$name]${leg:+ ($leg)} running..." >&2
  out="$(bash "$e2e" 2>&1)" && rc=0 || rc=$?
  printf '%s\n' "$out" > "$log"
  state="$(e2e_state "$rc")"
  echo "device-e2e-tier: [$name]${leg:+ ($leg)} $state" >&2
  echo "$name $state"
}

# run_lane <lane> <list-file> — run a lane's scripts in turn, printing one
# `<name> <state>` per script. A lane that touches the Android device stops
# at the first failure that turns out to be the device no longer able to
# show anything: every script after it would fail about its own step.
run_lane() {
  local lane="$1" list="$2" e2e line problem
  while read -r e2e <&3; do
    [ -z "$e2e" ] && continue
    line="$(run_one "$e2e" "" </dev/null)"
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
if [ "${SMIX_TIER_ONLY:-}" != api36 ]; then
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
fi

# ---- the API 36 leg ----------------------------------------------------
#
# After the main leg, never beside it: two emulators and a simulator at
# once is load the machine's other users feel. The AVD is booted through
# smix, so the ledger records who booted it, and shut down through smix
# only when this leg booted it — a 36 that was already running is used
# and left as it was found.
# SMIX_TIER_ONLY=api36 runs this leg alone.
api36_rc=0
API36_AVD="$E2E_ANDROID_36"   # scripts/lib/e2e-devices.sh
list_said="$(api36_list_check)" || { echo "device-e2e-tier: $list_said"; exit 1; }
api36_booted=0
api36_down() {
  if [ "$api36_booted" = 1 ]; then
    "$SMIX" sim shutdown "$API36_AVD" >/dev/null 2>&1 \
      || echo "device-e2e-tier: warning: could not shut down $API36_AVD" >&2
  fi
}
trap api36_down EXIT

api36_serial="$("$SMIX" sim resolve "$API36_AVD" 2>/dev/null | tail -1 | tr -d '[:space:]')" || api36_serial=""
if [ -z "$api36_serial" ] || ! adb -s "$api36_serial" shell getprop sys.boot_completed 2>/dev/null | grep -q 1; then
  echo "device-e2e-tier: booting $API36_AVD for the API 36 leg" >&2
  if ! "$SMIX" sim boot "$API36_AVD" >/tmp/device-e2e-api36-boot.log 2>&1; then
    echo "device-e2e-tier: FAILED on API 36 — could not boot $API36_AVD: $(tail -3 /tmp/device-e2e-api36-boot.log)"
    exit 1
  fi
  api36_booted=1
  api36_serial="$("$SMIX" sim resolve "$API36_AVD" 2>/dev/null | tail -1 | tr -d '[:space:]')"
fi
case "$api36_serial" in
  emulator-*) : ;;
  *) echo "device-e2e-tier: FAILED on API 36 — $API36_AVD resolves to '$api36_serial', not an emulator"; exit 1 ;;
esac
for _ in $(seq 1 90); do
  adb -s "$api36_serial" shell getprop sys.boot_completed 2>/dev/null | grep -q 1 && break
  sleep 2
done
problem36="$(e2e_android_screen_problem_of "$api36_serial")"
if [ -n "$problem36" ]; then
  echo "device-e2e-tier: FAILED on API 36 — $API36_AVD ($api36_serial): $problem36"
  exit 1
fi

# The list is read on fd 3 and each script gets /dev/null on stdin: a
# script's adb or ssh reads stdin, and on the first run of this leg the
# first script swallowed the rest of the list, so one ran of six.
# SMIX_C8_PLATFORMS=android: c8's iOS legs would repeat the main leg's.
results36=""
while read -r name <&3; do
  [ -z "$name" ] && continue
  line="$(SMIX_E2E_ANDROID="$API36_AVD" SMIX_ANDROID_SERIAL="$api36_serial" SMIX_C8_PLATFORMS=android \
    run_one "$ROOT/scripts/dev/$name.sh" "API 36" </dev/null)"
  results36="$results36$line"$'\n'
done 3<<< "$API36_E2E"
n_ran="$(printf '%s' "$results36" | grep -c .)"
n_listed="$(printf '%s\n' "$API36_E2E" | grep -c .)"
if [ "$n_ran" != "$n_listed" ]; then
  echo "device-e2e-tier: FAILED on API 36 — $n_listed scripts listed, $n_ran ran"
  api36_rc=1
fi
printf '%s' "$results36" | e2e_verdict "on API 36" || api36_rc=1

[ "$main_rc" = 0 ] && [ "$api36_rc" = 0 ]
