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

  if [ "$fails" -ne 0 ]; then
    echo "device-e2e-tier selftest: FAIL ($fails)" >&2
    exit 1
  fi
  echo "device-e2e-tier selftest: 6 cases pass"
  exit 0
fi

: "${SMIX_E2E_UDID:?set SMIX_E2E_UDID to the simulator the scripts should drive}"

results=""
for e2e in "$ROOT"/scripts/dev/*-e2e.sh; do
  name="$(basename "$e2e" .sh)"
  echo "device-e2e-tier: [$name] running..." >&2
  out="$(bash "$e2e" 2>&1)" && rc=0 || rc=$?
  printf '%s\n' "$out" > "/tmp/device-e2e-$name.log"

  # Exit 0 with SKIP in the output is a skip; exit 0 without it is a
  # script that ran. Both conventions in this repo print SKIP somewhere
  # — some to stderr as `SKIP:`, some to stdout as a named marker — and
  # combined output catches either.
  if [ "$rc" -ne 0 ]; then
    state=fail
  elif printf '%s' "$out" | grep -q "SKIP"; then
    state=skip
  else
    state=drove
  fi
  echo "device-e2e-tier: [$name] $state" >&2
  results="$results$name $state"$'\n'
done

printf '%s' "$results" | e2e_verdict
