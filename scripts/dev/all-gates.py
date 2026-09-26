#!/usr/bin/env python3
"""Run every Python gate CI runs, read each one's exit code, name the red ones.

Running every gate used to mean retyping the same loop: grep the
workflows for `python3 scripts/dev/*.py`, run each, read `$?`. A loop that
is retyped drifts, and a loop whose pattern stops matching runs nothing
and reports nothing wrong — the same shape as a gate reading air. This is
that loop, written once, with the gate list derived from the workflows
rather than written down, and with a second reader that has to agree.

Two readers:

* the first parses each workflow line for `python3 scripts/dev/<name>.py`
  plus its arguments, and is what gets run;
* the second only asks whether a line mentions `python3` and a
  `scripts/dev/*.py` at all. A line the second sees and the first cannot
  parse is red, naming the line — so when the first reader's pattern
  drifts, the two disagree instead of both going quiet.

Usage:
    python3 scripts/dev/all-gates.py [--root DIR]
"""

from __future__ import annotations

import argparse
import glob
import os
import re
import shlex
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SELF = "scripts/dev/all-gates.py"

# The command, then its arguments up to the first shell operator.
COMMAND = re.compile(r"python3 (scripts/dev/[A-Za-z0-9._-]+\.py)((?: [^|&;#\n]*)?)")
# The second reader: anything that looks like it names a gate.
MENTIONS = re.compile(r"python3\b.*scripts/dev/[A-Za-z0-9._-]+\.py")


def derive(root: str) -> tuple[list[list[str]], list[str]]:
    """The gate commands CI runs, and the lines that name a gate unreadably."""
    commands: list[list[str]] = []
    seen: set[tuple[str, ...]] = set()
    unread: list[str] = []
    for wf in sorted(glob.glob(os.path.join(root, ".github", "workflows", "*.yml"))):
        rel = os.path.relpath(wf, root)
        with open(wf, encoding="utf-8") as fh:
            for n, line in enumerate(fh, 1):
                if line.lstrip().startswith("#"):
                    continue
                found = list(COMMAND.finditer(line))
                if MENTIONS.search(line) and not found:
                    unread.append(f"{rel}:{n}: {line.strip()}")
                for m in found:
                    argv = [m.group(1), *shlex.split(m.group(2))]
                    if argv[0] == SELF or tuple(argv) in seen:
                        continue
                    seen.add(tuple(argv))
                    commands.append(argv)
    return commands, unread


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=REPO)
    root = os.path.abspath(ap.parse_args().root)

    commands, unread = derive(root)
    problems: list[str] = [
        f"a workflow line names a gate this runner could not read: {u}" for u in unread
    ]
    if not commands:
        problems.append(
            "no gate was derived from .github/workflows — either CI runs none, or the "
            "pattern that finds them stopped matching; both are red"
        )

    for argv in commands:
        path = os.path.join(root, argv[0])
        if not os.path.isfile(path):
            problems.append(f"{argv[0]} is named in CI and is not on disk")
            continue
        r = subprocess.run(
            [sys.executable, *argv], cwd=root, capture_output=True, text=True, check=False
        )
        if r.returncode != 0:
            said = (r.stdout + r.stderr).strip().splitlines()[-6:]
            problems.append(
                f"{' '.join(argv)} exited {r.returncode}:\n      " + "\n      ".join(said)
            )

    if problems:
        print(f"all-gates: FAIL — {len(problems)} of {len(commands)} gate(s) or reading problems")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(f"all-gates: clean — {len(commands)} gate(s) derived from CI, each exited 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
