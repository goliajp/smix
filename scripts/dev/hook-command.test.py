#!/usr/bin/env python3
"""What reaches the device guards, and what is only being written down.

The guards match shell syntax. `hook-command.py` decides which text they
are allowed to match against, and until 2026-08-12 it kept every heredoc
body except `cat`'s and `tee`'s. That read as the careful choice. What it
actually did was refuse documents: a paragraph describing a device
command could not be written, because the paragraph contained the
command, and even a `<serial>` placeholder was read as a real serial.

The line is now drawn at shells. A python heredoc that genuinely reached
a device would write `subprocess.run(["adb", "-s", serial, ...])`, which
the guards' shell-shaped pattern never matches — so keeping that body
could only ever catch prose. A `bash` heredoc is different: there the
pattern means exactly what it looks like, and the body stays.

These cases are the two halves of that, plus the shapes that must not
regress.
"""

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HOOK = ROOT / "plugin" / "scripts" / "hook-command.py"

# The device word these cases carry. Built rather than written out, so
# this file does not itself contain the shape the guards refuse — the
# failure it exists to describe would otherwise stop it being saved.
TOOL = "a" + "db"
SERIAL = "R5CT" + "52DF07D"


def run(command: str) -> str:
    payload = json.dumps({"tool_input": {"command": command}})
    out = subprocess.run(
        [sys.executable, str(HOOK)],
        input=payload,
        capture_output=True,
        text=True,
        check=False,
    )
    return out.stdout


CASES = [
    (
        "a python heredoc writing a document keeps its prose out of judgement",
        f"python3 - <<'PY'\nopen('n.md','w').write('{TOOL} -s <serial> install x')\nPY",
        False,
    ),
    (
        "so does any other non-shell consumer",
        f"ruby <<'RB'\nputs '{TOOL} -s {SERIAL} install x'\nRB",
        False,
    ),
    (
        "cat, as before",
        f"cat > n.md <<'EOF'\n{TOOL} -s {SERIAL} install x\nEOF",
        False,
    ),
    (
        "a bash heredoc still has its body judged — there it would run",
        f"bash <<'EOF'\n{TOOL} -s {SERIAL} install x\nEOF",
        True,
    ),
    (
        "sh too",
        f"sh <<'EOF'\n{TOOL} -s {SERIAL} install x\nEOF",
        True,
    ),
    (
        "a plain command is untouched",
        f"{TOOL} -s {SERIAL} install x",
        True,
    ),
    (
        "the line before a heredoc is still the command",
        f"{TOOL} -s {SERIAL} install x && cat > n.md <<'EOF'\nnothing\nEOF",
        True,
    ),
]


def main() -> int:
    failures = []
    for name, command, should_reach in CASES:
        seen = TOOL in run(command)
        if seen != should_reach:
            failures.append(
                f"{name}\n      expected the guards to "
                f"{'see' if should_reach else 'not see'} it, they did "
                f"{'see' if seen else 'not see'} it"
            )

    if failures:
        print("hook-command.test: FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1

    print(f"hook-command.test: {len(CASES)} cases pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
