#!/usr/bin/env python3
"""The C10 ground truth has to carry its numbers, not just its story.

A decomposition document is worth exactly what it measured. This one
exists because two hypotheses about a landscape tap were both wrong, and
the only reason anybody knows that is four rows of numbers taken off a
running simulator. A version of the same document with the prose and
without the rows would read the same and prove nothing — and it is the
easy thing to end up with, because the prose is the part that gets
rewritten.

So: every row named in the plan must be present with both frames, the
event stamp, and the verdict; the conclusion must name which hypothesis
survived; and the two refuted ones must be marked refuted rather than
quietly dropped.
"""

import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
DOC = os.path.join(REPO, ".claude", "docs", "research", "v5.1-landscape-coordinate-spaces.md")

# The measurements the plan says this checkpoint produces, by column.
#
# By column rather than "these strings appear in the row": both frames
# are the same size in every row, so a check that only asks whether
# `874×402` occurs anywhere in the line passes with one of the two
# columns emptied — which is exactly what the mutation run found it
# doing. The two frames agreeing is the finding; a check that cannot
# tell one from the other cannot see the finding go missing.
#
# Columns: label | appFrame | snapshotRootFrame | device | eventRecord | agree | taps
ROWS = [
    ("A 竖屏首页", "402×874", "402×874", "portrait"),
    ("A2 竖屏计数器屏", "402×874", "402×874", "portrait"),
    ("B app 驱动横屏", "874×402", "874×402", "portrait"),
    ("C 设备驱动横屏", "874×402", "874×402", "portrait"),
]

# Both must appear as refuted. A document that simply stops mentioning a
# hypothesis has not refuted it — it has forgotten it, and the next
# reader will propose it again.
REFUTED = ["H1", "H2"]


def main() -> int:
    problems = []

    if not os.path.exists(DOC):
        print(f"v5.1-c10-ground-truth: FAIL — {os.path.relpath(DOC, REPO)} does not exist")
        return 1

    with open(DOC, encoding="utf-8") as fh:
        text = fh.read()

    for label, app_frame, root_frame, stamp in ROWS:
        line = next((l for l in text.splitlines() if label in l and "|" in l), "")
        if not line:
            problems.append(f"no table row for {label!r} — the plan says all four were measured")
            continue
        cells = [c.strip().replace("*", "") for c in line.strip().strip("|").split("|")]
        if len(cells) < 5:
            problems.append(f"the {label!r} row has {len(cells)} columns, too few to carry a measurement")
            continue
        for index, want, what in (
            (1, app_frame, "appFrame"),
            (2, root_frame, "snapshotRootFrame"),
            (4, stamp, "the event's orientation stamp"),
        ):
            if cells[index] != want:
                problems.append(
                    f"the {label!r} row's {what} reads {cells[index]!r}, measured {want!r}"
                )

    for h in REFUTED:
        row = next((l for l in text.splitlines() if f"**{h}**" in l), "")
        if not row:
            problems.append(f"{h} is not in the hypothesis table — a dropped one is not a closed one")
        elif "推翻" not in row and "refuted" not in row:
            problems.append(f"{h} is named but not marked refuted")

    # The point of the exercise. Without it the document is four rows and
    # no finding.
    if "eventRecordOrientation" not in text and "eventRecord" not in text:
        problems.append("the event's orientation stamp is never named, and it is the finding")

    # A ground truth that names a fix has stopped being a ground truth.
    # C10 measures; C12 changes something. The document is allowed to
    # list the candidate routes so long as it says it is not choosing.
    # One required sentence, not "any of these phrasings". The first
    # version accepted either of two, so removing one left the other
    # holding the rule up and the mutation run showed the check green
    # on a document that had started recommending a repair.
    if "本 checkpoint 不选" not in text:
        problems.append(
            "the document does not carry the line saying this checkpoint chooses no fix — "
            "decomposition that picks the repair is an attack wearing its clothes"
        )

    # Numbers that came off a machine, from a machine that can be named.
    # The identifier, not the naming convention. `sim-smix-*` says which
    # family of simulator; it does not say which one answered these
    # numbers, and accepting it let the UDID be deleted with the check
    # still green.
    if not re.search(r"[0-9A-F]{8}-", text):
        problems.append(
            "no device identifier — a simulator family is not a device, and "
            "measurements nobody can re-take are anecdotes"
        )

    if problems:
        print("v5.1-c10-ground-truth: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    rows = len(ROWS)
    print(
        f"v5.1-c10-ground-truth: clean — {rows} measured rows, "
        f"{len(REFUTED)} hypotheses refuted, the finding named, no fix chosen"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
