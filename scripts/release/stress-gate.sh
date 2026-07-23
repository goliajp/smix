#!/usr/bin/env bash
#
# Graded stress+smoke gate. Runs the corpus at a tier and reports a
# per-flow pass/fail + timing as structured JSON, so CI judges by
# machine rather than by log-reading. One corpus, two tiers:
#   --tier smoke   a key subset, per PR
#   --tier all     the full corpus, nightly
# The tier -> flow-list decision lives in stress-select.py, never
# duplicated here.
#
# Modes:
#   stress-gate.sh --tier smoke|all [--parallel N] [--sim <ref>]
#   stress-gate.sh --tier smoke|all --dry-run     # select + parse, no device
#   stress-gate.sh --selftest                     # aggregation logic, no device
#
# Device selection: --sim <ref>, else $SMIX_CORPUS_SIM. Exit 0 = all
# green; 1 = a flow failed; 2 = usage/setup error.

set -uo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SELECT="$REPO_ROOT/scripts/release/stress-select.py"

# aggregate "flow|ok|ms" ... -> JSON array on stdout; returns 1 if any
# flow's ok is not 1. The pure core, exercised by --selftest.
aggregate() {
  local fails=0 json="[" first=1
  for r in "$@"; do
    IFS='|' read -r flow ok ms <<<"$r"
    [ "$ok" = "1" ] || fails=$((fails + 1))
    if [ "$first" = "1" ]; then first=0; else json+=","; fi
    local okj=false
    [ "$ok" = "1" ] && okj=true
    json+="{\"flow\":\"$flow\",\"ok\":$okj,\"ms\":$ms}"
  done
  json+="]"
  echo "$json"
  [ "$fails" -eq 0 ]
}

if [ "${1:-}" = "--selftest" ]; then
  out="$(aggregate 'a|1|100' 'b|1|200')" && rc=0 || rc=$?
  [ "$rc" = "0" ] || { echo "selftest FAIL: all-pass must exit 0"; exit 1; }
  echo "$out" | grep -q '"flow":"a"' || { echo "selftest FAIL: json missing flow"; exit 1; }
  out="$(aggregate 'a|1|100' 'b|0|200')" && rc=0 || rc=$?
  [ "$rc" != "0" ] || { echo "selftest FAIL: a failed flow must be non-zero"; exit 1; }
  echo "$out" | grep -q '"ok":false' || { echo "selftest FAIL: json missing the failure"; exit 1; }
  echo "stress-gate: selftest ok"
  exit 0
fi

TIER="smoke"; PARALLEL=""; DRY=0; SIM="${SMIX_CORPUS_SIM:-}"
while [ $# -gt 0 ]; do
  case "$1" in
    --tier) TIER="$2"; shift 2 ;;
    --parallel) PARALLEL="$2"; shift 2 ;;
    --sim) SIM="$2"; shift 2 ;;
    --dry-run) DRY=1; shift ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

# `mapfile` is bash 4+; macOS ships bash 3.2, so read into the array by
# hand to keep the gate runnable on the default shell.
FLOWS=()
while IFS= read -r _line; do
  [ -n "$_line" ] && FLOWS+=("$_line")
done < <(python3 "$SELECT" --tier "$TIER")
[ "${#FLOWS[@]}" -gt 0 ] || { echo "no flows for tier $TIER" >&2; exit 2; }
echo "stress-gate: tier=$TIER flows=${#FLOWS[@]}"

SMIX_BIN="${SMIX_BIN:-$(command -v smix)}"

if [ "$DRY" = "1" ]; then
  # CI-safe: prove the tier's flows all parse, no device.
  "$SMIX_BIN" run "${FLOWS[@]}" --device dry --dry-run
  exit $?
fi

[ -n "$SIM" ] || { echo "error: --sim <ref> or SMIX_CORPUS_SIM required for a device run" >&2; exit 2; }
STAMP="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="$REPO_ROOT/.tmp/stress-gate/$STAMP"; mkdir -p "$LOG_DIR"
cleanup() { "$SMIX_BIN" runner down --device "$SIM" >/dev/null 2>&1 || true; }
trap cleanup EXIT

"$SMIX_BIN" sim boot "$SIM" >"$LOG_DIR/boot.log" 2>&1 || true
"$SMIX_BIN" runner up "$SIM" >"$LOG_DIR/up.log" 2>&1 \
  || { echo "error: runner up failed"; tail -20 "$LOG_DIR/up.log" >&2; exit 3; }

# Run the flows. --parallel shards them across sims when the caller
# provides extra --sim refs via SMIX_CORPUS_ALSO_SIMS (space-separated);
# otherwise each runs on the one sim.
RESULTS=()
if [ -n "$PARALLEL" ] && [ -n "${SMIX_CORPUS_ALSO_SIMS:-}" ]; then
  also=(); for s in $SMIX_CORPUS_ALSO_SIMS; do also+=(--also-device "$s"); done
  start=$(python3 -c 'import time;print(int(time.time()*1000))')
  "$SMIX_BIN" run "${FLOWS[@]}" --device "$SIM" "${also[@]}" --parallel "$PARALLEL" \
    >"$LOG_DIR/parallel.log" 2>&1 && ok=1 || ok=0
  end=$(python3 -c 'import time;print(int(time.time()*1000))')
  RESULTS+=("parallel-batch|$ok|$((end - start))")
else
  for flow in "${FLOWS[@]}"; do
    name="$(basename "$flow" .yaml)"
    start=$(python3 -c 'import time;print(int(time.time()*1000))')
    if "$SMIX_BIN" run "$flow" --device "$SIM" >"$LOG_DIR/$name.log" 2>&1; then ok=1; else ok=0; fi
    end=$(python3 -c 'import time;print(int(time.time()*1000))')
    echo "stress-gate: [$name] $([ $ok = 1 ] && echo PASS || echo FAIL) $((end - start))ms"
    RESULTS+=("$name|$ok|$((end - start))")
  done
fi

JSON="$(aggregate "${RESULTS[@]}")" && RC=0 || RC=1
echo "$JSON" >"$LOG_DIR/results.json"
echo "$JSON"
[ "$RC" = "0" ] && echo "stress-gate: GREEN tier=$TIER" || echo "stress-gate: RED tier=$TIER (see $LOG_DIR)"
exit "$RC"
