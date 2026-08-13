#!/usr/bin/env python3
"""Every way of naming an element is written down on every surface, or signed off.

`tapOn: { point: "50%,80%" }` had worked in flows and all four SDKs for
two majors. It did not exist in MCP or the CLI, and nothing noticed,
because nothing was looking at that axis: `mcp-cli-parity-scan` asks
whether every MCP *tool* has a CLI command behind it, and
`plugin-capability-parity` asks whether a skill names a real command.
Which selector forms a surface accepts is a different question, and it
had no answer anywhere. Four forms were missing from MCP and five from
the CLI when this was written.

So each surface declares, in its own source, what it does with each
variant of the canonical `Selector`:

    // selector-surface: Point — `point:50%,80%`, dispatched by cmd_tap
    // selector-surface: Anchor — none, modifiers have no CLI form at all

The declarations live with the code rather than in a table here, for the
reason `mcp-cli-parity-scan` gives for the same choice: a central table
goes stale exactly the way the route list and three copies of "are these
modifiers empty" did.

A `none` needs its reason. "Not listed" and "decided against" look
identical in a diff and mean opposite things, and only one of them is a
decision.

A declaration that says a form IS supported has to name a word that
appears in that file. Otherwise the declaration is prose: it says the
surface accepts something, nothing checks, and the gate reports coverage
for a line somebody wrote once.
"""

from __future__ import annotations

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

CANONICAL = os.path.join("crates", "smix-selector", "src", "lib.rs")

# Surface -> the file that owns its selector reading.
SURFACES = {
    "yaml": os.path.join("crates", "smix-adapter-maestro", "src", "parser.rs"),
    "CLI": os.path.join("crates", "smix-cli", "src", "act.rs"),
    "MCP": os.path.join("crates", "smix-mcp", "src", "selector_params.rs"),
}

DECLARATION = re.compile(
    r"//\s*selector-surface:\s*(\w+)\s*[—-]\s*(.+?)\s*$", re.M
)

# Floors. A scan that reads no variants agrees that every surface is
# complete, and a surface with two declarations agrees it was audited.
# Eleven variants and four supported forms per surface were the measured
# numbers the day this was written; both only ever grow.
MIN_VARIANTS = 11
MIN_SUPPORTED = 4
# And a floor on the surfaces themselves. Both floors above guard the
# rows; nothing guarded the table. With `SURFACES` emptied, "every
# surface declares every form" is true of no surfaces at all, and this
# gate would report clean while reading nothing — the shape it was
# written to catch, in its own source. Found by applying
# `.claude/rule/empty-predicate.md` to the gate rather than to the code.
MIN_SURFACES = 3


def read(rel: str) -> str:
    with open(os.path.join(ROOT, rel), encoding="utf-8") as fh:
        return fh.read()


def canonical_variants(src: str) -> list[str]:
    m = re.search(r"pub enum Selector \{(.*?)\n\}", src, re.S)
    if not m:
        return []
    return re.findall(r"^    ([A-Z]\w+)\s*[\{\(,]", m.group(1), re.M)


def main() -> int:
    problems: list[str] = []

    variants = canonical_variants(read(CANONICAL))
    if len(variants) < MIN_VARIANTS:
        print("selector-surface-scan: FAIL")
        print(
            f"  - {len(variants)} variant(s) parsed out of {CANONICAL} — this scan is "
            f"reading air. An empty set of variants makes 'every variant is declared' "
            f"true of every surface, including one that declares nothing."
        )
        return 1

    if len(SURFACES) < MIN_SURFACES:
        print("selector-surface-scan: FAIL")
        print(
            f"  - {len(SURFACES)} surface(s) listed — this scan is reading air. "
            f"With no surfaces, 'every surface declares every form' is true and "
            f"says nothing."
        )
        return 1

    for surface, rel in SURFACES.items():
        body = read(rel)
        # The declaration lines are not evidence for themselves. A line
        # reading ``Point — the `point` form`` contains the word `point`,
        # so searching the whole file finds it and the check passes on a
        # file that never mentions the form anywhere else. Strip them
        # first: what is being asked is whether the CODE agrees.
        evidence = "\n".join(
            l for l in body.splitlines() if "selector-surface:" not in l
        )
        declared: dict[str, str] = {}
        for variant, detail in DECLARATION.findall(body):
            if variant in declared:
                problems.append(
                    f"{surface} declares {variant} twice in {rel} — one line per variant, "
                    f"so a change has one place to be made"
                )
            declared[variant] = detail

        supported = 0
        for variant in variants:
            if variant not in declared:
                problems.append(
                    f"{surface} says nothing about {variant}. Add a "
                    f"`// selector-surface: {variant} — …` line to {rel}: how it is "
                    f"written here, or `none — <why>`. Silence is how `Point` stayed "
                    f"missing from two surfaces for two majors"
                )
                continue
            detail = declared[variant]
            if detail.startswith("none"):
                if not re.match(r"none\s*[—,-]\s*\S", detail):
                    problems.append(
                        f"{surface}'s {variant} is `none` with no reason ({rel}). "
                        f"'Not listed' and 'decided against' read the same in a diff "
                        f"and mean opposite things"
                    )
                continue
            supported += 1
            # The declaration has to point at something real in the file.
            token = re.search(r"`([^`]+)`", detail)
            if not token:
                problems.append(
                    f"{surface}'s {variant} declaration names no word to look for "
                    f"({rel}) — put the written form in backticks so this can check "
                    f"the file agrees with the sentence"
                )
            elif token.group(1).split(":")[0].strip("`") not in evidence:
                problems.append(
                    f"{surface} declares {variant} as `{token.group(1)}` and {rel} "
                    f"contains no such word — the declaration has become prose"
                )

        for variant in declared:
            if variant not in variants:
                problems.append(
                    f"{surface} declares {variant}, which is not a variant of Selector "
                    f"any more ({rel}) — the enum moved and the declaration did not"
                )

        if supported < MIN_SUPPORTED:
            problems.append(
                f"{surface} declares only {supported} supported form(s) — below the "
                f"floor of {MIN_SUPPORTED}, which means this scan is agreeing with a "
                f"surface it has not really read"
            )

    if problems:
        print("selector-surface-scan: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    print(
        f"selector-surface-scan: clean — {len(variants)} selector forms, "
        f"{len(SURFACES)} surfaces, every pair declared"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
