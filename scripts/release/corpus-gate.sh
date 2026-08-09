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
# Default corpus: the 20-flow stress corpus built in-tree at v2.8-C5. The
# older default pointed at a consumer PR path that is not tracked in this
# repo; the in-tree corpus is what ship.sh and preflight can reach without
# a consumer checkout.
DEFAULT_CORPUS="$REPO_ROOT/scripts/release/stress-corpus"

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

# Portable to macOS's /bin/bash 3.2 (which has no `mapfile`): read into
# an array via while-read on NUL-safe input isn't needed here — the file
# names come from `find`, and none in-tree carry newlines.
YAMLS=()
while IFS= read -r line; do
  YAMLS+=("$line")
done < <(find "$CORPUS_DIR" -type f -name '*.yaml' | sort)
if [[ ${#YAMLS[@]} -eq 0 ]]; then
  echo "error: no .yaml files under $CORPUS_DIR" >&2
  exit 2
fi

SMIX_BIN="${SMIX_BIN:-$(command -v smix)}"
if [[ -z "$SMIX_BIN" ]]; then
  echo "error: smix binary not on PATH (set SMIX_BIN)" >&2
  exit 2
fi

# A port of this gate's own, so a bystander runner cannot turn it red.
. "$REPO_ROOT/scripts/lib/gate-port.sh"

STAMP="$(date +%Y%m%d-%H%M%S)"
LOG_DIR="$REPO_ROOT/.tmp/release-gate/$STAMP"
mkdir -p "$LOG_DIR"

echo "corpus gate: sim=$SMIX_CORPUS_SIM corpus=$CORPUS_DIR yamls=${#YAMLS[@]} port=$SMIX_RUNNER_PORT log=$LOG_DIR"

# --- teardown ------------------------------------------------------------

cleanup() {
  echo "corpus gate: tearing down runner"
  "$SMIX_BIN" runner down >/dev/null 2>&1 || true
  # The corpus's `takeScreenshot` steps write to the working directory,
  # which for the release gate is the repo root — so a gate run left six
  # untracked PNGs behind every time. They are named by the flows, so
  # the list comes from the flows rather than from a copy of it here.
  for shot in $(grep -rh 'takeScreenshot' "$CORPUS_DIR"/*.yaml \
                | sed 's/.*takeScreenshot: *//' | sort -u); do
    rm -f "$REPO_ROOT/$shot"
  done
  # Diagnostic dump AFTER runner down — captures the last observed state.
  "$SMIX_BIN" diagnostic dump --json \
    > "$LOG_DIR/diagnostic-dump.json" 2>/dev/null || true
}
trap cleanup EXIT

# --- setup ---------------------------------------------------------------

# `sim boot` is idempotent in intent — an already-booted sim is fine here,
# the gate needs it up not freshly cycled. `xcrun simctl` returns exit 149
# with SimError 405 ("Unable to boot device in current state: Booted") on a
# booted sim; treat that as success.
echo "corpus gate: ensuring sim $SMIX_CORPUS_SIM is booted"
if "$SMIX_BIN" sim boot "$SMIX_CORPUS_SIM" >"$LOG_DIR/sim-boot.log" 2>&1; then
  :
elif grep -q 'current state: Booted' "$LOG_DIR/sim-boot.log"; then
  echo "corpus gate: sim already booted"
else
  echo "error: sim boot failed"
  cat "$LOG_DIR/sim-boot.log" >&2
  exit 3
fi

# The fixture app, which is what makes this corpus more than one subject.
#
# Twenty flows against Settings is one subject walked twenty ways, and a
# system app is not an ordinary one — preinstalled, stable ids, windows
# owned by the system. A defect that only shows on an ordinary app was
# invisible to every device gate here at once, which is how a consumer
# found `/tree` carrying the SystemUI windows and not their app's while
# everything in this repository was green.
#
# A build failure fails the gate rather than letting the fixture flow go
# red on "the app is not installed": those are different causes and the
# second one names the wrong thing.
echo "corpus gate: building and installing the fixture app"
if ! bash "$REPO_ROOT/scripts/dev/build-fixture-app.sh" >"$LOG_DIR/fixture-build.log" 2>&1; then
  echo "error: the fixture app did not build" >&2
  tail -20 "$LOG_DIR/fixture-build.log" >&2
  exit 3
fi
FIXTURE_APP="$REPO_ROOT/test-fixtures/demo-app/build/SmixFixture.app"
if ! "$SMIX_BIN" sim install "$SMIX_CORPUS_SIM" "$FIXTURE_APP"      >"$LOG_DIR/fixture-install.log" 2>&1; then
  echo "error: the fixture app did not install on $SMIX_CORPUS_SIM" >&2
  tail -20 "$LOG_DIR/fixture-install.log" >&2
  exit 3
fi

# iOS `runner up` requires --bundle: the runner latches XCUIApplication to
# it. The stress corpus drives Preferences (v2.8-C5 shipping form); a flow
# naming a different appId rebinds per request through the App-Bundle-Id
# header, which is how the fixture flow reaches its own app.
: "${SMIX_CORPUS_BUNDLE:=com.apple.Preferences}"
echo "corpus gate: bringing runner up --bundle $SMIX_CORPUS_BUNDLE (auto-syncs sources on version drift)"
"$SMIX_BIN" runner up "$SMIX_CORPUS_SIM" --bundle "$SMIX_CORPUS_BUNDLE" >"$LOG_DIR/runner-up.log" 2>&1 \
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
  # `smix run` takes the flow yaml as a positional (per `smix run --help`);
  # a stale `--script` flag would be rejected as "unexpected argument".
  # --device is required (post-fold: App is not bound to a UDID otherwise).
  #
  # No --retry. It was here on a wrong reading: TAP_MISSED on the two
  # nav flows was called an iOS animation race, and it was the hit chain
  # being snapshotted after the touch, so a tap that opened a screen saw
  # the destination under its own coordinate and reported itself a miss.
  # Retrying only made the gate quieter about it. A flow that needs two
  # attempts here is a finding, not a pass.
  if python3 "$REPO_ROOT/scripts/dev/run-with-timeout.py" "$SMIX_CORPUS_TIMEOUT_S" \
       "$SMIX_BIN" run "$yaml" --device "$SMIX_CORPUS_SIM" \
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
