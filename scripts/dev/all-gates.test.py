#!/usr/bin/env python3
"""What `all-gates.py` must answer, fed workflow trees rather than this one.

The runner exists because every checkpoint used to retype the same loop —
grep the workflows for `python3 scripts/dev/*.py`, run each, read `$?` —
and two things go wrong with a loop that is retyped: it drifts, and when
its pattern stops matching it runs nothing and reports nothing wrong. So
the cases here are about the reader as much as the running: it must find
exactly the gates CI runs, it must go red when a line it should have read
yields nothing, and a gate that fails must be named.
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
RUNNER = os.path.join(ROOT, "scripts", "dev", "all-gates.py")

problems: list[str] = []


def expect(label: str, ok: bool, detail: str) -> None:
    if not ok:
        problems.append(f"{label}: {detail}")


def write(path: str, body: str) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(body)


def gate(root: str, name: str, code: int, says: str = "") -> None:
    write(
        os.path.join(root, "scripts", "dev", name),
        f"import sys\nprint({says or name!r})\nsys.exit({code})\n",
    )


def run(root: str) -> tuple[int, str]:
    r = subprocess.run(
        [sys.executable, RUNNER, "--root", root],
        capture_output=True, text=True, check=False,
    )
    return r.returncode, r.stdout + r.stderr


def workflow(root: str, *lines: str) -> None:
    body = ["jobs:", "  a:", "    steps:"] + [f"      - run: {line}" for line in lines]
    write(os.path.join(root, ".github", "workflows", "ci.yml"), "\n".join(body) + "\n")


if not os.path.isfile(RUNNER):
    print("all-gates.test: FAIL")
    print(f"  - {os.path.relpath(RUNNER, ROOT)} does not exist")
    sys.exit(1)

# 1. Every gate green → exit 0, and the count says how many ran.
with tempfile.TemporaryDirectory() as t:
    gate(t, "a.py", 0)
    gate(t, "b.test.py", 0)
    workflow(t, "python3 scripts/dev/a.py", "python3 scripts/dev/b.test.py")
    code, out = run(t)
    expect("all green passes", code == 0, f"exit {code}:\n{out}")
    expect("and says two ran", "2 gate" in out, f"no count of two in:\n{out}")

# 2. One gate red → non-zero, and that gate is named with its exit code and
#    what it said. A summary that only says "something failed" sends the
#    reader back to run them one by one.
with tempfile.TemporaryDirectory() as t:
    gate(t, "a.py", 0)
    gate(t, "b.py", 3, "b-says-why")
    workflow(t, "python3 scripts/dev/a.py", "python3 scripts/dev/b.py")
    code, out = run(t)
    expect("one red gate fails the run", code != 0, f"exit 0:\n{out}")
    expect("and names it", "scripts/dev/b.py" in out, f"b.py not named:\n{out}")
    expect("with its own exit code", "3" in out, f"exit code not reported:\n{out}")
    expect("and what it said", "b-says-why" in out, f"its output was dropped:\n{out}")

# 3. Arguments on the CI line are passed, not dropped.
with tempfile.TemporaryDirectory() as t:
    write(
        os.path.join(t, "scripts", "dev", "args.py"),
        "import sys\nsys.exit(0 if sys.argv[1:] == ['--check'] else 5)\n",
    )
    workflow(t, "python3 scripts/dev/args.py --check")
    code, out = run(t)
    expect("a gate's arguments reach it", code == 0, f"exit {code}:\n{out}")

# 4. Nothing derived → red. A runner that finds no gates and says clean is
#    the one this file exists to rule out.
with tempfile.TemporaryDirectory() as t:
    workflow(t, "cargo test")
    code, out = run(t)
    expect("no gates derived is red", code != 0, f"exit 0:\n{out}")

# 5. A workflow line naming a gate that the reader could not parse → red,
#    naming the line. This is the second reader: it asks only "does this
#    line mention python3 and a scripts/dev/*.py", so when the first reader's
#    pattern drifts the two disagree instead of both going quiet.
with tempfile.TemporaryDirectory() as t:
    gate(t, "a.py", 0)
    workflow(t, "python3 scripts/dev/a.py", 'python3 "scripts/dev/quoted.py"')
    code, out = run(t)
    expect("an unread gate line is red", code != 0, f"exit 0:\n{out}")
    expect("and names the line", "quoted.py" in out, f"line not named:\n{out}")

# 6. The runner does not run itself, and the same command on two workflow
#    lines runs once.
with tempfile.TemporaryDirectory() as t:
    gate(t, "a.py", 0)
    workflow(t, "python3 scripts/dev/a.py", "python3 scripts/dev/a.py",
             "python3 scripts/dev/all-gates.py")
    code, out = run(t)
    expect("itself and duplicates are skipped", code == 0 and "1 gate" in out,
           f"exit {code}:\n{out}")

# 7. A gate named in CI that is not on disk is red, not skipped.
with tempfile.TemporaryDirectory() as t:
    gate(t, "a.py", 0)
    workflow(t, "python3 scripts/dev/a.py", "python3 scripts/dev/gone.py")
    code, out = run(t)
    expect("a missing gate is red", code != 0, f"exit 0:\n{out}")
    expect("and named", "gone.py" in out, f"gone.py not named:\n{out}")

if problems:
    print("all-gates.test: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)
print("all-gates.test: 7 cases pass")
