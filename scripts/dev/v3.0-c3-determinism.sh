#!/usr/bin/env bash
# v3.0-C3: the same corpus, ten times, the same answer.
#
# One green run says the corpus can pass. Ten consecutive green runs with
# no retries anywhere is what "deterministic" has to mean before a
# release gate is worth anything — and that bar only became measurable at
# C2, which taught the gate to say FLAKE instead of folding a retry into
# a pass.
#
# What this does NOT do is decide what to fix. The cold plan named two
# mechanisms to stabilise; both were written before there was any
# instrument to see them, and neither reproduced on the two runs after
# C2 landed. Attacking a surface nobody has observed is what
# `debug/decomposition-before-attack` forbids. This measures; whatever it
# names is what gets attacked next.
#
# Usage:
#   SMIX_CORPUS_SIM=<UDID> SMIX_BIN=<path> bash scripts/dev/v3.0-c3-determinism.sh [N]
#   bash scripts/dev/v3.0-c3-determinism.sh --selftest
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT_DIR="$ROOT/.tmp/c3-determinism"

log() { printf '[c3] %s\n' "$*" >&2; }

# The verdict over N runs, given each run's exit code and flake count.
#
# Separated from the running so it can be driven without two hours of
# simulator time. Every rule that matters lives here: a missing count is
# an instrument failure, not a zero.
#
# Reads pairs on stdin, one run per line: `<exit-code> <flake-count>`,
# where a flake count of `-` means the run left no count file.
c3_verdict() {
  local runs=0 clean=0 problems=()
  local rc flakes
  while read -r rc flakes; do
    [ -z "$rc" ] && continue
    runs=$((runs + 1))
    if [ "$flakes" = "-" ]; then
      # The gate writes this file unconditionally. Its absence means the
      # gate did not reach its own summary, so this run measured
      # nothing — and a run that measured nothing must never be counted
      # as a clean one. Reading silence as a pass is how a classifier
      # that answered NORECORD twenty-one times still produced GREEN.
      problems+=("run $runs: no flake count — the gate left no measurement")
      continue
    fi
    if [ "$rc" != "0" ]; then
      problems+=("run $runs: corpus exited $rc, $flakes flaky")
      continue
    fi
    if [ "$flakes" != "0" ]; then
      problems+=("run $runs: green but $flakes flaky — a retry was needed")
      continue
    fi
    clean=$((clean + 1))
  done

  if [ "$runs" -eq 0 ]; then
    echo "c3: NO RUNS — nothing was measured"
    return 1
  fi
  if [ ${#problems[@]} -ne 0 ]; then
    printf 'c3: NOT DETERMINISTIC — %d/%d clean\n' "$clean" "$runs"
    printf '  - %s\n' "${problems[@]}"
    return 1
  fi
  printf 'c3: DETERMINISTIC — %d/%d clean\n' "$clean" "$runs"
  return 0
}

if [ "${1:-}" = "--selftest" ]; then
  fails=0
  check() { # label expected-exit expected-substring <<< pairs
    local label="$1" want_rc="$2" want_sub="$3" pairs="$4"
    local out rc
    out="$(printf '%s\n' "$pairs" | c3_verdict)" && rc=0 || rc=$?
    if [ "$rc" -ne "$want_rc" ]; then
      echo "c3 selftest: $label — exit $rc, wanted $want_rc" >&2
      fails=$((fails + 1))
    fi
    case "$out" in
      *"$want_sub"*) ;;
      *) echo "c3 selftest: $label — output lacks '$want_sub': $out" >&2
         fails=$((fails + 1)) ;;
    esac
  }

  ten_clean=$(for _ in $(seq 10); do echo "0 0"; done)
  check "ten clean runs" 0 "DETERMINISTIC — 10/10" "$ten_clean"

  # Green with a flake is the case C2 exists for: the gate already fails
  # it, and this must not quietly re-admit it.
  check "green but flaky" 1 "a retry was needed" "$(printf '0 0\n0 1\n0 0')"

  check "a red run" 1 "corpus exited 1" "$(printf '0 0\n1 2\n0 0')"

  # The one that would otherwise look like success.
  check "a run with no count file" 1 "no flake count" "$(printf '0 0\n0 -\n0 0')"

  check "nothing measured at all" 1 "NO RUNS" ""

  # A partial count must not read as the full bar.
  check "three clean runs are not ten" 0 "DETERMINISTIC — 3/3" "$(printf '0 0\n0 0\n0 0')"

  if [ "$fails" -ne 0 ]; then
    echo "c3 selftest: FAIL ($fails)" >&2
    exit 1
  fi
  echo "c3 selftest: 6 cases pass"
  exit 0
fi

N="${1:-10}"
: "${SMIX_CORPUS_SIM:?set SMIX_CORPUS_SIM to the simulator to drive}"
SMIX_BIN="${SMIX_BIN:-$(command -v smix)}"
[ -n "$SMIX_BIN" ] || { echo "error: no smix binary (set SMIX_BIN)" >&2; exit 2; }

mkdir -p "$OUT_DIR"
rm -f "$OUT_DIR"/run-*.txt

log "$N runs against $SMIX_CORPUS_SIM — results in $OUT_DIR"

pairs=""
for i in $(seq 1 "$N"); do
  run_log="$OUT_DIR/corpus-$i.log"
  # No `set -e` abort here, and no early exit on a red run: the question
  # is the distribution over N runs, not where the first failure sits.
  # One red is enough to fail the checkpoint; the remaining runs are
  # still evidence about how often and which flows.
  SMIX_CORPUS_SIM="$SMIX_CORPUS_SIM" SMIX_BIN="$SMIX_BIN" \
    bash "$ROOT/scripts/release/corpus-gate.sh" >"$run_log" 2>&1 && rc=0 || rc=$?

  # The gate writes its count into a timestamped directory; take the
  # newest, which is the run that just finished.
  count_file="$(ls -t "$ROOT"/.tmp/release-gate/*/flake-count.txt 2>/dev/null | head -1)"
  if [ -n "$count_file" ] && [ -r "$count_file" ]; then
    flakes="$(cat "$count_file")"
  else
    flakes="-"
  fi

  printf '%s %s\n' "$rc" "$flakes" > "$OUT_DIR/run-$i.txt"
  pairs="$pairs$rc $flakes"$'\n'
  log "run $i/$N: exit=$rc flaky=$flakes ($run_log)"
done

printf '%s' "$pairs" | c3_verdict
