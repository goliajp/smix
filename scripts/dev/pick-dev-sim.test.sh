#!/usr/bin/env bash
# What the picker may hand a release gate.
#
# It used to answer from a naming convention: a booted sim called
# `sim-smix-*` is ours. On 2026-08-11 `sim-smix-02` was booted by
# something that writes no ledger, matched the name, and was handed to
# the ship smoke gate — which started a runner on it and terminated the
# Safari somebody had running there. The name was right and the sim was
# not free.
#
# So the rule is now "a ledger says smix booted it", and these drive it
# with fakes: a stub `smix` whose `lease owner` exit code is scripted,
# and a stub `xcrun` that reports whatever booted sims a case needs.
# Nothing here touches a real device, which is the point — the rule has
# to be checkable without one, or it is only ever checked by accident.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PICKER="$ROOT/scripts/dev/pick-dev-sim.sh"
PASS=0
FAIL=0

ok()  { echo "  PASS: $*"; PASS=$((PASS + 1)); }
bad() { echo "  FAIL: $*"; FAIL=$((FAIL + 1)); }
has() { case "$1" in *"$2"*) return 0 ;; *) return 1 ;; esac }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/bin"

# `xcrun simctl list devices -j` — the booted set a case wants.
make_xcrun() {  # $@ = "UDID NAME" pairs
    {
        echo '#!/usr/bin/env bash'
        echo 'python3 - <<'"'"'PY'"'"''
        echo 'import json'
        echo 'rows = ['
        for pair in "$@"; do
            set -- $pair
            echo "  {\"udid\": \"$1\", \"name\": \"$2\", \"state\": \"Booted\"},"
        done
        echo ']'
        echo 'print(json.dumps({"devices": {"iOS-26-5": rows}}))'
        echo 'PY'
    } > "$WORK/bin/xcrun"
    chmod +x "$WORK/bin/xcrun"
}

# A `smix` whose `lease owner` exits with a per-UDID code.
make_smix() {  # $@ = "UDID:RC" pairs
    {
        echo '#!/usr/bin/env bash'
        echo 'if [ "$1" = "lease" ] && [ "$2" = "owner" ] && [ "$3" = "--help" ]; then exit 0; fi'
        echo 'if [ "$1" = "lease" ] && [ "$2" = "owner" ]; then'
        echo '  case "$3" in'
        for pair in "$@"; do
            echo "    ${pair%%:*}) exit ${pair##*:} ;;"
        done
        echo '    *) exit 3 ;;'
        echo '  esac'
        echo 'fi'
        echo 'exit 1'
    } > "$WORK/bin/smix"
    chmod +x "$WORK/bin/smix"
}

run_picker() {
    PATH="$WORK/bin:$PATH" SMIX_BIN="$WORK/bin/smix" bash "$PICKER" 2>"$WORK/err"
}

echo "=== 1. a booted sim nobody recorded is not taken ==="
# The incident, exactly: the name matches and no ledger mentions it.
make_xcrun "AAAA-02 sim-smix-02"
make_smix "AAAA-02:3"
OUT="$(run_picker)"; RC=$?
ERR="$(cat "$WORK/err")"
if [ "$RC" != 0 ] && [ -z "$OUT" ] && has "$ERR" "no ledger says smix booted"; then
    ok "refused, and said why"
else
    bad "exit $RC out='$OUT' err='$ERR'"
fi

echo "=== 2. one that a ledger records is handed over ==="
make_xcrun "BBBB-03 sim-smix-03"
make_smix "BBBB-03:0"
OUT="$(run_picker)"; RC=$?
if [ "$RC" = 0 ] && [ "$OUT" = "BBBB-03" ]; then
    ok "printed the eligible UDID"
else
    bad "exit $RC out='$OUT' err='$(cat "$WORK/err")'"
fi

echo "=== 3. the recorded one is chosen over the unrecorded one ==="
# Both carry the repo's name; only one is on the books. The naming rule
# would have called this ambiguous and refused — or, before that, taken
# whichever came first.
make_xcrun "AAAA-02 sim-smix-02" "BBBB-03 sim-smix-03"
make_smix "AAAA-02:3" "BBBB-03:0"
OUT="$(run_picker)"; RC=$?
if [ "$RC" = 0 ] && [ "$OUT" = "BBBB-03" ]; then
    ok "the busy one was skipped, not counted as ambiguity"
else
    bad "exit $RC out='$OUT' err='$(cat "$WORK/err")'"
fi

echo "=== 4. two on the books is still the caller's decision ==="
make_xcrun "BBBB-03 sim-smix-03" "CCCC-04 sim-smix-04"
make_smix "BBBB-03:0" "CCCC-04:0"
OUT="$(run_picker)"; RC=$?
ERR="$(cat "$WORK/err")"
if [ "$RC" != 0 ] && has "$ERR" "name the"; then
    ok "refused and listed them"
else
    bad "exit $RC out='$OUT'"
fi

echo "=== 5. 'I cannot ask' is not 'go ahead' ==="
# exit 1 from `lease owner` means the question failed — a resolution
# error, an unreadable ledger. Reading that as eligible would be the
# same mistake in a new place.
make_xcrun "DDDD-05 sim-smix-05"
make_smix "DDDD-05:1"
OUT="$(run_picker)"; RC=$?
if [ "$RC" != 0 ] && [ -z "$OUT" ]; then
    ok "an unanswerable question is not a licence"
else
    bad "exit $RC out='$OUT'"
fi

echo "=== 6. a smix that cannot answer refuses, rather than guessing ==="
make_xcrun "BBBB-03 sim-smix-03"
printf '#!/usr/bin/env bash\nexit 2\n' > "$WORK/bin/smix"; chmod +x "$WORK/bin/smix"
OUT="$(PATH="$WORK/bin:/usr/bin:/bin" SMIX_BIN="$WORK/bin/smix" bash "$PICKER" 2>"$WORK/err")"
RC=$?
ERR="$(cat "$WORK/err")"
if [ "$RC" != 0 ] && has "$ERR" "can answer who booted"; then
    ok "no fallback to the name when the ledger cannot be consulted"
else
    bad "exit $RC out='$OUT' err='$ERR'"
fi

echo
echo "=== $PASS passed, $FAIL failed ==="
[ "$FAIL" = 0 ]
