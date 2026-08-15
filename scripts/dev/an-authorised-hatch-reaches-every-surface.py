#!/usr/bin/env python3
"""An authorised escape hatch must reach every surface it was authorised for.

Section 9 #3 of the charter forbids xpath and coordinate selectors, and
authorises exactly two exceptions on the same grounds: `tap_at_coord`
and `swipe_at_coord`. The first reached the CLI and MCP surfaces. The
second did not — and `docs/ai-guide/verb-parity.md` ticks both platforms
for "swipe between coordinates", so somebody following the documentation
writes a flow that cannot be written.

It travelled a whole major version because nothing asked. The gate over
selector shapes watches selectors, and an escape hatch is deliberately
not a selector; it rides a different axis and that axis had no gate.
Wiring the missing half fixes this once. This asks the question every
time.

Two halves, per `.claude/rule/empty-predicate.md`:

  - every authorised hatch is present on every surface, and
  - every coordinate API that was NOT authorised is absent from all of
    them.

The second half is what stops this file from becoming a licence. A gate
that only checks presence would nod along while `fill_at_coord` appeared
next to the two the charter names, which is precisely the expansion §9
#3 was written to prevent.
"""

import argparse
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))

# Every surface a caller can reach a hatch through. Four, not two: the
# CLI and MCP are the ones a reader names first, and a gate written
# around those two would be blind to exactly the kind of omission it
# exists to catch — one surface quietly missing while the others agree.
SURFACES = [
    ("CLI", "crates/smix-cli/src/main.rs"),
    ("MCP", "crates/smix-mcp/src/main.rs"),
    ("TS SDK", "npm/smix-rn/src/App.ts"),
    ("napi", "crates/smix-node/src/lib.rs"),
]

# What "present" looks like, per hatch, per surface.
#
# Not one token checked everywhere: each surface names the same thing
# its own way, and the first draft looked for the bare words `from` and
# `to` in every file. Both are ordinary English, both occur in every one
# of these files for unrelated reasons, and the gate reported the CLI
# and MCP as complete while neither could swipe between two points. A
# pattern that matches prose is not a check.
PRESENCE = {
    "tap_at_coord": {
        "CLI": r'"point"|point:',
        "MCP": r"\bpoint\b",
        "TS SDK": r"\btapAtCoord\b",
        "napi": r"\btap_at_coord\b",
    },
    "swipe_at_coord": {
        # The flag spelling, not the field name. A struct field called
        # `swipe_from` with no `#[arg(long = ...)]` on it is not a
        # surface anybody can reach, and accepting it let the harness
        # delete the flag with the gate still green.
        "CLI": r'long = "from"',
        "MCP": r'"from"|from:\s*Option|swipe_from',
        "TS SDK": r"\bswipeAtCoord\b",
        "napi": r"\bswipe_at_coord\b",
    },
}

# Coordinate APIs the charter does not authorise. Named rather than
# inferred: "anything ending in _at_coord" would let a future rename
# through, and the charter lists these as the ones that would need their
# own decision.
NOT_AUTHORISED = ["fill_at_coord", "anchor_at_coord", "scroll_at_coord"]


def read(rel, root=REPO):
    path = os.path.join(root, rel)
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf-8") as fh:
        return fh.read()


def main() -> int:
    # `--root` so the harness can drive a fixture tree. Without it the
    # only tree this gate has ever judged is the one it lives in, and
    # half its rules concern a surface that is missing — a state the
    # repository is not in and cannot be put into to check.
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=REPO)
    args = ap.parse_args()
    root = os.path.abspath(args.root)

    problems = []
    checked = 0

    texts = {}
    for label, rel in SURFACES:
        text = read(rel, root)
        if text is None:
            problems.append(f"{label}: {rel} does not exist — the surface list has gone stale")
            continue
        texts[label] = text
        checked += 1

    if checked < len(SURFACES):
        problems.append(
            f"only {checked} of {len(SURFACES)} surfaces were read, and a scan "
            "that read nothing agrees with everything"
        )

    for hatch, per_surface in PRESENCE.items():
        verb = hatch.split("_at_coord")[0]
        for label, text in texts.items():
            pattern = per_surface.get(label)
            if pattern is None:
                problems.append(
                    f"{hatch} has no pattern for the {label} surface — a hatch "
                    f"nobody wrote a check for is a hatch this gate cannot see"
                )
                continue
            if not re.search(pattern, text):
                problems.append(
                    f"{label}: {verb} is authorised to take coordinates and this "
                    f"surface does not offer them — a reader following the guides "
                    f"writes something that cannot be written"
                )

    for forbidden in NOT_AUTHORISED:
        for label, text in texts.items():
            if forbidden in text:
                problems.append(
                    f"{label}: {forbidden} is on the surface and section 9 #3 does "
                    f"not authorise it — a new coordinate API needs its own decision, "
                    f"not this gate's silence"
                )

    if problems:
        print("an-authorised-hatch-reaches-every-surface: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    print(
        f"an-authorised-hatch-reaches-every-surface: clean — "
        f"{len(PRESENCE)} hatches on {checked} surfaces, "
        f"{len(NOT_AUTHORISED)} unauthorised ones absent"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
