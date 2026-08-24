#!/usr/bin/env bash
#
# Run the :sdk assertion suite on a pinned emulator, at ship time.
#
# Those four assertions were written, committed, and then run by nothing
# at all until someone ran them by hand — the same week three Android
# defects survived because no gate executed anything on a device. This
# is the gate that would have run them.
#
# :app is deliberately absent. Its androidTest source set holds the
# runner body, one @Test that starts the HTTP server and blocks, so
# `connectedDebugAndroidTest` there does not fail — it never returns.
# Measured: three minutes forty at "Tests 0/1 completed" while /health
# answered 200. In a release script that is a hang, not a red.
#
# DEVICE SELECTION LIVES HERE, and that is load-bearing. adb-guard reads
# the text of the command it is asked to approve; once an adb call is
# inside a script, the hook sees only `bash scripts/...` and cannot
# judge it. Putting the check here keeps the same rule in force:
# emulator-NNNN is allowed, anything else is refused. Allowlisting the
# emulator form rather than denylisting a known phone means a newly
# plugged device is safe by default — the stance adb-guard takes.
#
# Env:
#   SMIX_ANDROID_SERIAL       — device to use; else the first emulator-*
#   SMIX_ANDROID_GATE_TIMEOUT_S — wall clock limit (default 600)

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
MODULE="android-runner/sdk"
RESULTS="$REPO_ROOT/$MODULE/build/outputs/androidTest-results/connected/debug"
LOG="${TMPDIR:-/tmp}/smix-android-instrumentation-gate.log"
TIMEOUT_S="${SMIX_ANDROID_GATE_TIMEOUT_S:-600}"

die() {
  echo "android instrumentation gate: $1" >&2
  exit 1
}

# --- device selection, before anything touches a device ------------------

SERIAL="${SMIX_ANDROID_SERIAL:-}"
if [[ -z "$SERIAL" ]]; then
  # Not the first emulator adb lists. That is a coin toss over whose
  # device this gate drives, and it landed on somebody else's: another
  # person's smix had an emulator on the default port, and this gate
  # drove it. pick-dev-emulator asks this machine's ledger whether smix
  # booted a device, and refuses rather than guesses.
  SERIAL="$(bash "$REPO_ROOT/scripts/dev/pick-dev-emulator.sh" 2>&1)" || {
    die "no emulator this machine's ledger answers for:
$SERIAL
  Boot one through smix — \`smix sim boot <alias>\` — or claim one that is
  already up and idle: \`smix lease claim <serial>\`. The claim is written
  down, so the next run does not have to make the same decision blind.
  SMIX_ANDROID_SERIAL still pins one for a single command, and records
  nothing — which is why it is no longer the first thing offered."
  }
fi

if [[ ! "$SERIAL" =~ ^emulator-[0-9]+$ ]]; then
  die "refusing to drive '$SERIAL': it is not an emulator serial (emulator-NNNN).
  A physical device is often attached to a developer machine, and one has been
  wiped this way before. Pin an emulator via SMIX_ANDROID_SERIAL."
fi

echo "android instrumentation gate: $MODULE on $SERIAL (timeout ${TIMEOUT_S}s)"

# --- run, with a deadline ------------------------------------------------
#
# A release gate may fail; it may not hang. There is no `timeout` binary
# on macOS, so the deadline is a background process plus a poll — the
# same conclusion corpus-gate reached, by a different route (it delegates
# to run-with-timeout.py; here the child must be a backgrounded gradle so
# its pid can be killed on expiry).

rm -rf "$RESULTS"
(
  cd "$REPO_ROOT/android-runner" \
    && ANDROID_SERIAL="$SERIAL" ./gradlew :sdk:connectedDebugAndroidTest --console=plain
) > "$LOG" 2>&1 &
GRADLE_PID=$!

WAITED=0
while kill -0 "$GRADLE_PID" 2>/dev/null; do
  if (( WAITED >= TIMEOUT_S )); then
    kill "$GRADLE_PID" 2>/dev/null
    sleep 2
    kill -9 "$GRADLE_PID" 2>/dev/null
    die "gradle did not finish within ${TIMEOUT_S}s — killed. Log: $LOG"
  fi
  sleep 2
  WAITED=$(( WAITED + 2 ))
done

wait "$GRADLE_PID"
GRADLE_RC=$?

# The judge runs either way: gradle's own exit code says whether the task
# succeeded, and the judge says whether the suite was actually covered.
# A task that "succeeded" having executed nothing is the case this gate
# exists for, so its verdict is not conditional on gradle's.
if ! python3 "$REPO_ROOT/scripts/dev/androidtest-xml-judge.py" \
      --module "$REPO_ROOT/$MODULE" --results "$RESULTS"; then
  # The judge just printed which of its four verdicts failed — coverage,
  # failures, skips. Restating it here as one guess ("did not cover the
  # suite") would describe a passing-but-failing run wrongly, so this
  # only adds what the judge cannot know.
  die "the :sdk suite did not pass the verdict above (gradle exit $GRADLE_RC). Log: $LOG"
fi

if (( GRADLE_RC != 0 )); then
  die "gradle exited $GRADLE_RC. Log: $LOG"
fi

COUNT="$(python3 "$REPO_ROOT/scripts/dev/androidtest-xml-judge.py" \
  --module "$REPO_ROOT/$MODULE" --results "$RESULTS" | grep -oE '[0-9]+/[0-9]+' | head -1)"
echo "android instrumentation gate: :sdk $COUNT on $SERIAL"
