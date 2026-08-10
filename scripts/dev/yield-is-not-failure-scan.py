#!/usr/bin/env python3
""""I am not going to disturb this" is a skip, not a failure.

A device e2e that finds the machine busy — a build in flight, a port
taken, another batch holding the runner — and chooses to stand aside has
not found a defect. Reporting `fail` there turns the gate red for
something the product did not do, and a gate that goes red for reasons
unrelated to the code is one people stop reading. That is how a suite
gets disabled rather than fixed.

Two federation scripts did exactly this, and one of them one line apart:
a busy batch owner was a `skip`, and a `cargo build` in flight was a
`fail` — the same species of condition, opposite conclusions, adjacent
lines. Both said "yielding" in the message.

This reads the refusal, not the situation: a line that reaches `fail`
while announcing that it is yielding, standing aside, or waiting for
something else to finish is the shape being caught. Judging what is
"really" an environment problem is not something a scan can do; judging
what a line says about itself is.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DEV = os.path.join(ROOT, "scripts", "dev")

# Words a line uses when it is standing aside rather than judging.
YIELDING = re.compile(
    r"yielding|in flight|busy|already (?:running|serves)|batch owner|"
    r"re-run when it is idle",
    re.I,
)

problems: list[str] = []
checked = 0

for name in sorted(os.listdir(DEV)) if os.path.isdir(DEV) else []:
    if not name.endswith("-e2e.sh"):
        continue
    path = os.path.join(DEV, name)
    try:
        lines = open(path, encoding="utf-8").read().splitlines()
    except OSError:
        continue
    checked += 1
    for n, line in enumerate(lines, 1):
        if line.lstrip().startswith("#"):
            continue
        if "fail " not in line and "fail(" not in line:
            continue
        # The message, not the condition: `fail "..."`.
        m = re.search(r'fail\s+"([^"]*)"', line)
        if not m:
            continue
        if YIELDING.search(m.group(1)):
            problems.append(
                f"{name}:{n} reports failure while standing aside: "
                f'"{m.group(1)}". If the script is declining to disturb '
                f"something, that is a skip — a red here is about the machine, "
                f"not about smix"
            )

# A scan that reads no scripts agrees with any suite at all.
if checked == 0:
    problems.append(f"no e2e scripts found under {os.path.relpath(DEV, ROOT)}")

if problems:
    print("yield-is-not-failure-scan: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print(f"yield-is-not-failure-scan: clean — {checked} e2e scripts, none call a yield a failure")
