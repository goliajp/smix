#!/usr/bin/env python3
"""Every rule in the ground-truth check can still refuse something.

Eight rules, removed one at a time from the document rather than from
the checker, because what this guards is the document. A rule nothing
depends on is not a rule, and two of these were exactly that when first
written: both accepted either of two phrasings, so deleting one left the
other holding it up and the check stayed green on a document that had
started recommending a repair.

Each mutation asserts it actually landed before reading the verdict. A
substitution that matched nothing produces the same green as a rule that
carries no weight, and telling those two apart afterwards is not
possible — the first run of this harness called three rules weightless
and one of the three was a mutation that never applied.

The search strings are in the document's language because they have to
match the document byte for byte; everything this file says for itself
is in the repository's.
"""

import pathlib
import subprocess
import sys

DOC = pathlib.Path(".claude/docs/research/v5.1-landscape-coordinate-spaces.md")
GATE = ["python3", "scripts/dev/v5.1-c10-ground-truth-is-complete.py"]

MUTATIONS = [
    ("drop the C row's appFrame", "| C 设备驱动横屏 | 874×402 |", "| C 设备驱动横屏 | (未测) |"),
    (
        "blank the B row's snapshot root",
        "| B app 驱动横屏 | **874×402** | **874×402** |",
        "| B app 驱动横屏 | **874×402** | (略) |",
    ),
    (
        "change the B row's event stamp",
        "| **874×402** | **portrait** | **portrait** |",
        "| **874×402** | **portrait** | **landscapeRight** |",
    ),
    (
        "stop calling H2 refuted",
        "H2** 瞄对但那个窗口不收合成事件 | **推翻**",
        "H2** 瞄对但那个窗口不收合成事件 | 待查",
    ),
    ("drop H1 from the table", "| **H1** 瞄错", "| ~~H1~~ 瞄错"),
    ("never name the event's orientation stamp", "eventRecord", "xxx"),
    ("start prescribing a fix", "本 checkpoint 不选", "推荐走第 1 条"),
    ("stop naming the device", "UDID `89980B43-…`", "某台模拟器"),
]


def main() -> int:
    original = DOC.read_text(encoding="utf-8")
    failures = []

    for name, before, after in MUTATIONS:
        if before not in original:
            failures.append(f"{name}: the search string is not in the document — this case tested nothing")
            continue
        DOC.write_text(original.replace(before, after, 1), encoding="utf-8")
        changed = DOC.read_text(encoding="utf-8") != original
        result = subprocess.run(GATE, capture_output=True, text=True)
        DOC.write_text(original, encoding="utf-8")

        if not changed:
            failures.append(f"{name}: the document came back unchanged")
        elif result.returncode == 0:
            failures.append(f"{name}: the check stayed green — nothing depends on that rule")
        else:
            why = [l.strip()[2:] for l in result.stdout.splitlines() if l.strip().startswith("-")]
            print(f"  {name} → {why[0][:78] if why else 'refused'}")

    if failures:
        print("v5.1-c10-ground-truth.test: FAILED")
        for f in failures:
            print(f"  - {f}")
        return 1

    print(f"v5.1-c10-ground-truth.test: {len(MUTATIONS)} rules all carry weight")
    return 0


if __name__ == "__main__":
    sys.exit(main())
