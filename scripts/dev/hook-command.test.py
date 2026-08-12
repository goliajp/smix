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


# Where one command ends.
#
# The guards match a pattern against text and then take a word out of it.
# While the text was the whole Bash call, those two could land on
# different commands: `{TOOL} -s emulator-5554 shell getprop` followed by
# `{TOOL} -s <a physical serial> install` was allowed through, because the
# emulator pin on the first line answered for the second. That is the one
# thing the guard exists to stop.
#
# So the payload comes back one command per line, and each is judged
# alone. The separators are shell's, and quoting is the half that has to
# be right: `shell input text "note; ..."` is one command, and splitting
# inside the quotes would invent a second one out of a string somebody is
# typing into a text field.
SPLIT_CASES = [
    (
        "&& separates, and the flag stays with the command it belongs to",
        f"curl -s https://example.com/x -o /tmp/x && {TOOL} -s emulator-5554 install x",
        2,
    ),
    (
        "a newline separates",
        f"{TOOL} -s emulator-5554 shell getprop x\n{TOOL} -s {SERIAL} install x",
        2,
    ),
    (
        "so do ; || and |",
        f"{TOOL} devices ; {TOOL} devices || {TOOL} devices | grep x",
        4,
    ),
    (
        "a separator inside quotes is text somebody is typing, not a command",
        f'{TOOL} -s emulator-5554 shell input text "note; {TOOL} install x"',
        1,
    ),
    (
        "and inside single quotes",
        f"{TOOL} -s emulator-5554 shell input text 'a && b'",
        1,
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

    for name, command, want in SPLIT_CASES:
        lines = [l for l in run(command).splitlines() if l.strip()]
        if len(lines) != want:
            failures.append(
                f"{name}\n      expected {want} command(s), got {len(lines)}: {lines}"
            )

    # The pairing, stated directly: the command carrying the URL must not
    # be the one the guard reads a device out of. Counting lines alone
    # would pass an implementation that split in the wrong places.
    first = run(
        f"curl -s https://example.com/x -o /tmp/x && {TOOL} -s emulator-5554 install x"
    ).splitlines()
    if first and TOOL in first[0]:
        failures.append(
            "the URL's command and the device command came back as one\n"
            f"      first line was: {first[0]!r}"
        )

    if failures:
        print("hook-command.test: FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1

    print(f"hook-command.test: {len(CASES) + len(SPLIT_CASES) + 1} cases pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
