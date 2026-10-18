# shellcheck shell=bash
# A gate's failure in the gate's own words.
#
# The ship used to name each gate's usual reason for going red, and the
# usual reason is not always the one: a-tap-that-cannot-land-says-so
# failed because its runner never came up, and the ship said a tap had
# been reported as landed.
#
# run_gate <name> <command...> runs the gate, shows its output, and on a
# red leaves GATE_SAID holding its name, exit code and the reasons it
# listed ("  - ..." lines).
run_gate() {
  local name="$1" log="/tmp/smix-ship-gate-$1.log" rc=0
  shift
  "$@" >"$log" 2>&1 || rc=$?
  cat "$log"
  [ "$rc" = 0 ] && return 0
  local said
  said="$(grep -E '^  - ' "$log" | head -3 | sed 's/^  - //' | paste -sd ';' -)"
  GATE_SAID="$name FAILED (exit $rc): ${said:-it gave no reason; its output is in $log}"
  return 1
}
