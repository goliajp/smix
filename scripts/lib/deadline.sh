#!/usr/bin/env bash
# Run a command with a deadline, and say when it ran out.
#
# Source this, do not run it. macOS has no `timeout`, so the scripts here
# called adb and smix with no deadline at all, and a device that had been
# pushed into a state it could not answer from held them there silently.
# The screen-height sweep did exactly that on 2026-09-23: after three
# `wm size` changes `adb shell` stopped answering, and the gate sat in its
# poll for ten minutes with no verdict (open-items Q2).
#
#   with_deadline SECS cmd args...
#
# Exits with the command's own status, or DEADLINE_STATUS (142) when the
# deadline passed — the status of a process killed by SIGALRM, which is
# what this is. A caller that sees it has not learned anything about the
# device except that it did not answer, and must say so as a verdict it
# cannot give (exit 2), never as a pass or as the product failing.

DEADLINE_STATUS=142

with_deadline() {
  local secs="$1"
  shift
  perl -e 'alarm shift @ARGV; exec @ARGV or die "with_deadline: cannot run $ARGV[0]: $!\n"' \
    "$secs" "$@"
}

# `--selftest`: prove the deadline bites and that it does not bite early.
if [[ "${BASH_SOURCE[0]}" == "$0" && "${1:-}" == "--selftest" ]]; then
  set -u
  with_deadline 1 sleep 5
  rc=$?
  [[ "$rc" == "$DEADLINE_STATUS" ]] || { echo "deadline.sh: a 5 s sleep under a 1 s deadline exited $rc, not $DEADLINE_STATUS" >&2; exit 1; }
  with_deadline 5 true
  rc=$?
  [[ "$rc" == 0 ]] || { echo "deadline.sh: \`true\` under a 5 s deadline exited $rc" >&2; exit 1; }
  with_deadline 5 false
  rc=$?
  [[ "$rc" == 1 ]] || { echo "deadline.sh: \`false\` under a deadline exited $rc, not its own 1" >&2; exit 1; }
  echo "deadline.sh: a command past its deadline exits $DEADLINE_STATUS; one inside it keeps its own status"
fi
