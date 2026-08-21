#!/usr/bin/env python3
"""The counts written beside hand-copied tables are the counts.

`web/src/data/verbs.ts` is a hand-curated subset of `VERB_TABLE`, and
its header states how large the full table is. `llms.txt` states the
same number in prose. Neither is generated from the table, and the
release procedure says of the first, in as many words, that it is
"checked here or nowhere".

A number written beside a list is a second source of truth. This one
happens to be right today — 49, 49 and 49 — and the way it stops being
right is that somebody adds a verb, which is the ordinary act this
project does every release.

The subset is deliberately a subset and its size is not checked. What is
checked is every claim about the FULL table's size, wherever it is
written down.

Usage:
  scripts/dev/a-hand-copied-table-says-a-number.py [repo-root]
"""

from __future__ import annotations

import os
import re
import sys

ROOT = (
    sys.argv[1]
    if len(sys.argv) > 1
    else os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
)

TABLE = os.path.join("crates", "smix-verbs", "src", "lib.rs")

# Where the size of the full table is asserted in prose, and the
# pattern that finds the number. Named rather than searched for: a scan
# that finds claims wherever they happen to be cannot tell "moved" from
# "deleted".
CLAIMS = [
    (
        os.path.join("web", "src", "data", "verbs.ts"),
        re.compile(r"full table is (\d+) entries"),
    ),
    (
        "llms.txt",
        re.compile(r"canonical yaml verb table \((\d+) entries\)"),
    ),
]

# Below this the table has not been read.
MIN_ENTRIES = 20


def real_count(root: str) -> int | None:
    path = os.path.join(root, TABLE)
    if not os.path.isfile(path):
        return None
    s = open(path, encoding="utf-8").read()
    try:
        i = s.index("pub static VERB_TABLE: &[VerbEntry] = &[")
        j = s.index("\n];", i)
    except ValueError:
        return None
    body = s[i:j]
    # Comments dropped: the table's own prose names verbs that are
    # deliberately absent, and counting those would inflate it.
    body = "\n".join(re.sub(r"//.*$", "", line) for line in body.splitlines())
    return len(re.findall(r"\bv\s*\(", body))


def main() -> int:
    n = real_count(ROOT)
    if n is None:
        print("a-hand-copied-table-says-a-number: CANNOT RUN")
        print(f"  - could not read VERB_TABLE from {TABLE}")
        return 2

    problems: list[str] = []
    if n < MIN_ENTRIES:
        problems.append(
            f"VERB_TABLE read as {n} entries, fewer than {MIN_ENTRIES}. The reader "
            f"has stopped matching, and every comparison below would be against a "
            f"number that is wrong in the same direction."
        )

    checked = 0
    for rel, pattern in CLAIMS:
        path = os.path.join(ROOT, rel)
        if not os.path.isfile(path):
            problems.append(f"{rel} is not in this tree, and it states the table's size")
            continue
        body = open(path, encoding="utf-8").read()
        found = pattern.findall(body)
        if not found:
            problems.append(
                f"{rel} no longer states the table's size in the form this checks "
                f"({pattern.pattern!r}). Either the sentence moved — in which case "
                f"this must follow it — or the claim is gone and this entry should "
                f"go with it. Silence is not the same as agreement."
            )
            continue
        for claim in found:
            checked += 1
            if int(claim) != n:
                problems.append(
                    f"{rel} says the verb table has {claim} entries and it has {n}. "
                    f"Nothing generates that number; somebody typed it, and adding "
                    f"a verb is the ordinary act that makes it wrong."
                )

    if problems:
        print("a-hand-copied-table-says-a-number: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    print(
        f"a-hand-copied-table-says-a-number: clean — VERB_TABLE has {n} entries and "
        f"{checked} written claim(s) agree"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
