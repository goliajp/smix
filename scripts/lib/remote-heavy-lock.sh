# shellcheck shell=bash
# Hold `bench-lock heavy` on a remote host for as long as this script runs.
#
# The federation e2e builds, runs a runner and boots a simulator on mini,
# which is also the benchmark host. They used to look for any cargo or
# xcodebuild there and step aside; mini almost always has one, so they
# almost never judged anything, and a benchmark in progress was not what
# they looked for. Heavy jobs may run side by side; a measurement may not,
# and the lock is what says so.
#
# The remote side is `bench-lock heavy … cat`: it holds the lock until its
# stdin ends. Its stdin is a pipe from a watcher here that ends when this
# script's process is gone or the lock is let go, however the script ended
# — `kill -9` included — and an ssh that drops ends it too. A FIFO held open
# on a descriptor was the first shape, and it held the lock after `kill -9`:
# every child the script had started inherited the descriptor, and one
# still running kept the FIFO open.
#
# remote_heavy_lock <host> <max-wait-seconds>
#   0   held
#   75  a benchmark kept it longer than max-wait (bench-lock's own code)
#   *   anything else: ssh or bench-lock failed; REMOTE_LOCK_SAID has why
remote_heavy_lock() {
  local host="$1" wait="$2" owner=$$ rc
  REMOTE_LOCK_DIR="$(mktemp -d)"
  (
    while kill -0 "$owner" 2>/dev/null && [ ! -e "$REMOTE_LOCK_DIR/release" ]; do
      sleep 1
    done
  ) | ssh -o ConnectTimeout=5 -o BatchMode=yes "$host" \
    "/usr/local/bin/bench-lock heavy --max-wait $wait sh -c 'echo held; exec cat >/dev/null'" \
    >"$REMOTE_LOCK_DIR/said" 2>&1 &
  REMOTE_LOCK_PID=$!
  while kill -0 "$REMOTE_LOCK_PID" 2>/dev/null; do
    grep -qx held "$REMOTE_LOCK_DIR/said" && return 0
    sleep 1
  done
  grep -qx held "$REMOTE_LOCK_DIR/said" && return 0
  # `wait` on a background pipeline waits for all of it, and the watcher
  # would otherwise outlive the ssh that already failed
  touch "$REMOTE_LOCK_DIR/release"
  wait "$REMOTE_LOCK_PID" && rc=0 || rc=$?
  REMOTE_LOCK_SAID="$(tail -5 "$REMOTE_LOCK_DIR/said")"
  return "$rc"
}

# Let it go: the watcher stops, the remote `cat` sees the end of its input,
# and bench-lock exits.
remote_heavy_unlock() {
  [ -n "${REMOTE_LOCK_PID:-}" ] || return 0
  touch "$REMOTE_LOCK_DIR/release"
  wait "$REMOTE_LOCK_PID" 2>/dev/null || true
  rm -rf "$REMOTE_LOCK_DIR"
  REMOTE_LOCK_PID=""
}
