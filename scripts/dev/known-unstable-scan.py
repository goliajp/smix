#!/usr/bin/env python3
""""Known unstable" means measured and attributed, not tired of.

A flow on that list has its FLAKE excused by the corpus gate. That is a
real reduction in what the gate promises, so the bar for getting on the
list is a number and a history: how often it fails, and what was tried.
Without those, "known" is a word someone reached for after a bad
afternoon, and the list becomes the place flaky tests go to be forgotten.

So each row must name a flow that exists, carry a measured rate with a
digit in it, and cite attempts. This also checks the reverse — a row for
a flow that no longer exists is a rule about nothing, still quietly
excusing a name.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CORPUS = os.path.join(ROOT, "scripts", "release", "stress-corpus")
LIST = os.path.join(CORPUS, "known-unstable.md")

problems: list[str] = []


def rows() -> list[list[str]]:
    """Table rows, as cell lists, skipping the header and separator."""
    if not os.path.isfile(LIST):
        return []
    out = []
    for line in open(LIST, encoding="utf-8"):
        line = line.strip()
        if not line.startswith("|") or set(line) <= set("|- "):
            continue
        cells = [c.strip() for c in line.strip("|").split("|")]
        if len(cells) >= 5 and cells[0] != "Flow":
            out.append(cells)
    return out


entries = rows()

for cells in entries:
    flow = cells[0].strip("`")
    symptom, rate, attempts = cells[1], cells[2], cells[3]

    if not os.path.isfile(os.path.join(CORPUS, f"{flow}.yaml")):
        problems.append(
            f"{flow!r} is excused here and has no yaml in the corpus — either "
            f"it was renamed, or this row is excusing a flow that no longer runs"
        )
    if not re.search(r"\d", rate):
        problems.append(
            f"{flow!r} has no measured rate ({rate!r}). A number, from runs "
            f"someone actually did — 'sometimes' is how a flow gets excused "
            f"without anyone knowing how often"
        )
    if len(symptom) < 30:
        problems.append(
            f"{flow!r}'s symptom is {len(symptom)} characters. Say what the "
            f"failure looks like, or the next reader cannot tell whether the "
            f"failure they are seeing is this one"
        )
    if not re.search(r"\d", attempts):
        problems.append(
            f"{flow!r} lists no attempts. What was tried is what stops the "
            f"next person repeating it"
        )

# A parser that finds nothing agrees with any list at all — including one
# that has quietly grown. But an empty table is also the state this file
# is trying to reach, and the two must be told apart by something other
# than the parse that is in doubt.
#
# The witness is the raw text: a row is a line beginning and ending with
# a pipe, counted without the parser. If there are pipe-rows and the
# parser found none, the parser is broken. If there are none either way,
# the table is empty — which happened on 2026-08-22, when the last
# excused flow was fixed and this branch refused the good news.
if os.path.isfile(LIST):
    raw_rows = [
        line
        for line in open(LIST, encoding="utf-8").read().splitlines()
        if line.strip().startswith("|")
        and line.strip().endswith("|")
        and not set(line.strip()) <= set("|- ")
        and "Measured rate" not in line
    ]
    if raw_rows and not entries:
        problems.append(
            f"{os.path.relpath(LIST, ROOT)} has {len(raw_rows)} table row(s) and "
            f"this scan parsed none — the table's shape changed and the scan is "
            f"reading air"
        )

if problems:
    print("known-unstable-scan: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

if not entries:
    print("known-unstable-scan: clean — no flow is excused")
else:
    names = ", ".join(c[0].strip("`") for c in entries)
    print(
        f"known-unstable-scan: clean — {len(entries)} flow(s) excused with a "
        f"measured rate and a history: {names}"
    )
