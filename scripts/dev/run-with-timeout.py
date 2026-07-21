#!/usr/bin/env python3
"""Run a command under a wall-clock limit. GNU `timeout`, without it.

corpus-gate.sh called `timeout` directly. macOS does not ship it —
it is GNU coreutils, `gtimeout` if you brew it — so on a stock Mac the
shell answered "command not found", exit 127, for every yaml in the
corpus. Every one was recorded FAIL, the gate was therefore always RED,
and ship.sh could not finish on the machine smix is developed on.

The failure mode is worth naming: a missing tool did not read as a
missing tool. It read as a product that fails every test it has.

Exit codes follow GNU timeout so the caller's contract is unchanged:
124 on timeout, otherwise the command's own code (127 if it cannot be
executed at all).

Usage: run-with-timeout.py <seconds> <command> [args…]
"""

import subprocess
import sys


def main():
    if len(sys.argv) < 3:
        print(
            "usage: run-with-timeout.py <seconds> <command> [args…]",
            file=sys.stderr,
        )
        return 2

    try:
        seconds = float(sys.argv[1])
    except ValueError:
        print(f"run-with-timeout: not a number of seconds: {sys.argv[1]}", file=sys.stderr)
        return 2

    argv = sys.argv[2:]
    try:
        # No capture: the caller redirects, and holding output would
        # turn a hung child's log into something nobody can read until
        # it finishes — which is exactly the case this exists for.
        return subprocess.run(argv, timeout=seconds).returncode
    except subprocess.TimeoutExpired:
        print(
            f"run-with-timeout: killed after {seconds:g}s: {' '.join(argv)}",
            file=sys.stderr,
        )
        return 124
    except FileNotFoundError:
        print(f"run-with-timeout: no such command: {argv[0]}", file=sys.stderr)
        return 127


if __name__ == "__main__":
    sys.exit(main())
