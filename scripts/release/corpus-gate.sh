#!/usr/bin/env bash
#
# v1.0.10 §D7 — release-gate that runs every `.yaml` under a corpus
# directory against a booted sim, refusing to succeed on any failure.
# Intended to be run BEFORE `scripts/release/ship.sh` — no publish
# proceeds if any yaml in the corpus fails.
#
# Corpus directory selection (in priority order):
#   1. --corpus-dir=<path> CLI flag
#   2. $SMIX_CORPUS_DIR env var
#   3. crates/smix-cli/tests/fixtures/bootstrap-corpus/
#
# The corpus is expected to be a flat directory of maestro-format
# `.yaml` files. Each file runs as its own `smix run --script <path>`
# invocation. The gate:
#   - Boots the target sim (via `smix sim boot <ref>`) if not already up
#   - Brings `smix runner up` (auto-syncs sources on version drift; §D2)
#   - Runs each yaml; a non-zero exit fails the gate immediately
#   - After completion, dumps `smix diagnostic dump --json` to a
#     timestamped file in .tmp/release-gate/ for post-mortem
#   - Never leaves the sim in a modified state on refusal — teardown
#     runs regardless via trap
#
# Environment:
#   SMIX_CORPUS_SIM       — device ref (name / UDID / registry alias)
#                            required
#   SMIX_CORPUS_DIR       — corpus root (see priority above)
#   SMIX_CORPUS_TIMEOUT_S — per-yaml timeout in seconds (default 120)
#
# Exit codes:
#   0 — corpus green
#   1 — one or more yamls failed
#   2 — misconfiguration (missing sim, missing corpus, etc.)
#   3 — infrastructure failure (runner boot, sim boot)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DEFAULT_CORPUS="$REPO_ROOT/crates/smix-cli/tests/fixtures/insight-bootstrap-corpus"

CORPUS_DIR=""
for arg in "$@"; do
  case "$arg" in
    --corpus-dir=*) CORPUS_DIR="${arg#*=}" ;;
    --help|-h)
      sed -n '2,30p' "${BASH_SOURCE[0]}"
      exit 0
      ;;
  esac
done
: "${CORPUS_DIR:=${SMIX_CORPUS_DIR:-$DEFAULT_CORPUS}}"
: "${SMIX_CORPUS_TIMEOUT_S:=120}"

# --- guards --------------------------------------------------------------

if [[ -z "${SMIX_CORPUS_SIM:-}" ]]; then
  echo "error: SMIX_CORPUS_SIM env var required (sim name / UDID / registry alias)" >&2
  exit 2
fi

if [[ ! -d "$CORPUS_DIR" ]]; then
  echo "error: corpus dir not found: $CORPUS_DIR" >&2
  echo "hint: pass --corpus-dir=<path>, or set SMIX_CORPUS_DIR, or place the insight-bootstrap-corpus PR at the default." >&2
  exit 2
fi

mapfile -t YAMLS < <(find "$CORPUS_DIR" -type f -name '*.yaml' | sort)
if [[ ${#YAMLS[@]} -eq 0 ]]; then
  echo "error: no .yaml files under $CORPUS_DIR" >&2
  exit 2
fi

SMIX_BIN="${SMIX_BIN:-$(command -v smix)}"
if [[ -z "$SMIX_BIN" ]]; then
  echo "error: smix binary not on PATH (set SMIX_BIN)" >&2
  exit 2
fi

STAMP="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="$REPO_ROOT/.tmp/release-gate/$STAMP"
mkdir -p "$LOG_DIR"

echo "corpus gate: sim=$SMIX_CORPUS_SIM corpus=$CORPUS_DIR yamls=${#YAMLS[@]} log=$LOG_DIR"

# --- teardown ------------------------------------------------------------

cleanup() {
  echo "corpus gate: tearing down runner"
  "$SMIX_BIN" runner down >/dev/null 2>&1 || true
  # Diagnostic dump AFTER runner down — captures the last observed state.
  "$SMIX_BIN" diagnostic dump --json \
    > "$LOG_DIR/diagnostic-dump.json" 2>/dev/null || true
}
trap cleanup EXIT

# --- setup ---------------------------------------------------------------

echo "corpus gate: booting sim $SMIX_CORPUS_SIM"
"$SMIX_BIN" sim boot "$SMIX_CORPUS_SIM" >"$LOG_DIR/sim-boot.log" 2>&1 \
  || { echo "error: sim boot failed"; cat "$LOG_DIR/sim-boot.log" >&2; exit 3; }

echo "corpus gate: bringing runner up (auto-syncs sources on version drift)"
"$SMIX_BIN" runner up "$SMIX_CORPUS_SIM" >"$LOG_DIR/runner-up.log" 2>&1 \
  || { echo "error: runner up failed"; tail -25 "$LOG_DIR/runner-up.log" >&2; exit 3; }

# --- run corpus ----------------------------------------------------------

FAILURES=()
for yaml in "${YAMLS[@]}"; do
  name="$(basename "$yaml" .yaml)"
  echo "corpus gate: [$name] running..."
  yaml_log="$LOG_DIR/${name}.log"
  # Hard limit per yaml so one hang does not stall the gate. 124 on
  # timeout, as GNU timeout does — treated as fail either way.
  #
  # NOT `timeout` itself: that is GNU coreutils and macOS does not ship
  # it, so on a stock Mac this line was "command not found" for every
  # yaml, every one was recorded FAIL, and the gate could never be
  # anything but RED. A missing tool has to read as a missing tool, not
  # as a product failing all of its tests.
  if python3 "$REPO_ROOT/scripts/dev/run-with-timeout.py" "$SMIX_CORPUS_TIMEOUT_S" \
       "$SMIX_BIN" run --script "$yaml" \
       >"$yaml_log" 2>&1; then
    echo "corpus gate: [$name] PASS"
  else
    rc=$?
    echo "corpus gate: [$name] FAIL (exit $rc)"
    FAILURES+=("$name")
  fi
done

if [[ ${#FAILURES[@]} -eq 0 ]]; then
  echo "corpus gate: GREEN — ${#YAMLS[@]}/${#YAMLS[@]} passing"
  exit 0
fi

echo "corpus gate: RED — ${#FAILURES[@]}/${#YAMLS[@]} failing:"
for f in "${FAILURES[@]}"; do
  echo "  - $f (log: $LOG_DIR/$f.log)"
done
exit 1
