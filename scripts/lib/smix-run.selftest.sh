#!/usr/bin/env bash
# Self-test for scripts/lib/smix-run. Run as `bash scripts/lib/smix-run --selftest`.
#
# Each case is a small e2e script that sources e2e-binary.sh with a fake
# smix, calls smix-run in one of the shapes the real scripts use, and
# prints CONTINUED if it got past the call. The fake stands in for the
# one thing that matters here: what smix printed and how it exited.

set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
FAILED=0

cat >"$WORK/smix" <<'FAKE'
#!/usr/bin/env bash
# $1 = run; behaviour from FAKE_MODE.
case "$FAKE_MODE" in
  pass)      echo '{"ok":true}'; exit 0 ;;
  verdict)   echo 'error: sdk: FAIL [ELEMENT_NOT_FOUND]: no element matched id row-7' >&2; exit 3 ;;
  driver)    echo 'step 3 (tapOn): running' ; echo 'error: sdk: FAIL [DRIVER_ERROR]: runner /find returned non-JSON body' >&2; exit 3 ;;
  unknown)   echo 'error: sdk: FAIL [SOMETHING_NEW]: a code this build does not know' >&2; exit 3 ;;
  nocode)    echo 'error: parse: flow.yaml: expected a mapping' >&2; exit 2 ;;
  sleep)     echo $$ >"$FAKE_PIDFILE"; sleep 30; exit 0 ;;
esac
FAKE
chmod +x "$WORK/smix"

# $1 = case name, $2 = mode, $3 = the call line; rest: expectations as
# `exit=N`, `out~TEXT` (stdout contains), `err~TEXT` (stderr contains),
# `out!TEXT` (stdout does not contain).
check() {
  local name="$1" mode="$2" call="$3"
  shift 3
  cat >"$WORK/case.sh" <<CASE
#!/usr/bin/env bash
set -euo pipefail
SMIX_BIN="$WORK/smix"
source "$HERE/e2e-binary.sh"
source "$HERE/deadline.sh"
rc=0
$call
echo "CONTINUED rc=\$rc"
CASE
  local status=0
  FAKE_MODE="$mode" FAKE_PIDFILE="$WORK/pid" bash "$WORK/case.sh" >"$WORK/out" 2>"$WORK/err" || status=$?
  local e ok=1
  for e in "$@"; do
    case "$e" in
      exit=*) [ "$status" = "${e#exit=}" ] || { echo "smix-run selftest: $name: exited $status, expected ${e#exit=}"; ok=0; } ;;
      out~*) grep -qF -- "${e#out~}" "$WORK/out" || { echo "smix-run selftest: $name: stdout lacks '${e#out~}'"; ok=0; } ;;
      out!*) ! grep -qF -- "${e#out!}" "$WORK/out" || { echo "smix-run selftest: $name: stdout has '${e#out!}'"; ok=0; } ;;
      err~*) grep -qF -- "${e#err~}" "$WORK/err" || { echo "smix-run selftest: $name: stderr lacks '${e#err~}'"; ok=0; } ;;
    esac
  done
  if [ "$ok" = 0 ]; then
    FAILED=1
    sed 's/^/    out: /' "$WORK/out"
    sed 's/^/    err: /' "$WORK/err"
  fi
}

CAPTURE='out="$("$SMIX_RUN" flow.yaml 2>&1)" || rc=$?; printf "%s\n" "$out"'
QUIET='"$SMIX_RUN" flow.yaml >/dev/null 2>&1 || rc=$?'

check "a pass passes" pass "$CAPTURE" \
  exit=0 'out~{"ok":true}' 'out~CONTINUED rc=0'
check "a verdict goes back to the script" verdict "$CAPTURE" \
  exit=0 'out~CONTINUED rc=3' 'out~FAIL [ELEMENT_NOT_FOUND]'
check "a driver failure inside \$(…) ends the script, naming smix's code" driver "$CAPTURE" \
  exit=1 'out!CONTINUED' 'err~smix run failed with DRIVER_ERROR' 'err~runner /find returned non-JSON body'
check "a driver failure under >/dev/null 2>&1 still says why" driver "$QUIET" \
  exit=1 'out!CONTINUED' 'err~smix run failed with DRIVER_ERROR'
check "a code this script cannot place is not a verdict" unknown "$CAPTURE" \
  exit=1 'out!CONTINUED' 'err~smix run failed with SOMETHING_NEW'
check "a failure with no code is not a verdict" nocode "$CAPTURE" \
  exit=1 'out!CONTINUED' 'err~no failure code (exit 2)' 'err~expected a mapping'
check "an env-prefixed call behind env -u is judged the same" driver \
  'env -u NOTHING SMIX_RUNNER_PORT=1 "$SMIX_RUN" flow.yaml >/dev/null 2>&1 || rc=$?' \
  exit=1 'out!CONTINUED' 'err~DRIVER_ERROR'
check "a wrapper with no binary ends the script, not reads as a failed flow" pass \
  'if SMIX_RUN_BIN= "$SMIX_RUN" flow.yaml >/dev/null 2>&1; then rc=passed; else rc=failed-as-expected; fi' \
  exit=1 'out!CONTINUED' 'err~SMIX_RUN_BIN is not set'
check "a deadline ends smix and reports the deadline's status" sleep \
  'with_deadline 1 "$SMIX_RUN" flow.yaml >/dev/null 2>&1 || rc=$?; [ "$rc" = "$DEADLINE_STATUS" ] || rc="not-the-deadline:$rc"' \
  exit=0 "out~CONTINUED rc=142"
if [ -s "$WORK/pid" ]; then
  sleep 0.5
  if kill -0 "$(cat "$WORK/pid")" 2>/dev/null; then
    echo "smix-run selftest: the deadline ended the wrapper and left smix (pid $(cat "$WORK/pid")) running"
    kill "$(cat "$WORK/pid")" 2>/dev/null
    FAILED=1
  fi
else
  echo "smix-run selftest: the deadline case never started the fake smix"
  FAILED=1
fi

# The verdict set is read from smix-error, and reading nothing must not
# pass for "nothing is a verdict".
verdicts="$(python3 "$HERE/failure-codes.py" verdicts)" || { echo "smix-run selftest: failure-codes.py failed"; FAILED=1; }
printf '%s\n' "$verdicts" | grep -qx ELEMENT_NOT_FOUND || { echo "smix-run selftest: ELEMENT_NOT_FOUND is not read as a verdict"; FAILED=1; }
printf '%s\n' "$verdicts" | grep -qx DRIVER_ERROR && { echo "smix-run selftest: DRIVER_ERROR is read as a verdict"; FAILED=1; }

if [ "$FAILED" = 0 ]; then
  echo "smix-run selftest: 9 cases + the code set, all as expected"
fi
exit "$FAILED"
