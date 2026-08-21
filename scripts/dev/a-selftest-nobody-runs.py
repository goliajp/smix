#!/usr/bin/env python3
"""Every gate self-test is invoked by something.

A `*.test.py` beside a gate exists to prove the gate can still go red:
it mutates the gate's input and demands a sentence back. That proof is
worth exactly as much as the last time it ran.

This repository has been here before with other things — fifteen fuzz
targets with nothing running them, a hygiene scan named only in two
comments — and it happened again the same day this file was written: a
gate written that morning, mutation-swept by hand once, and wired
nowhere. Its five mutations had been true for an hour and would have
gone on reading as true forever.

A self-test nobody runs is a claim about the past.

Usage:
  scripts/dev/a-selftest-nobody-runs.py [repo-root]
"""

import os
import sys

ROOT = (
    sys.argv[1]
    if len(sys.argv) > 1
    else os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
)
DEV = os.path.join("scripts", "dev")

# Where a self-test may be invoked from. Three, for the reason
# workflow-scan gives about gates: preflight is the local habit, CI is
# the branch, the ship is the release.
CALLERS = [
    os.path.join("scripts", "dev", "preflight.sh"),
    os.path.join(".github", "workflows", "ci.yml"),
    os.path.join("scripts", "release", "ship.sh"),
]

# Self-tests driven by something other than a named call — each with
# the reason, because silence must not be how one becomes exempt.
#
# Empty, and checked for still being warranted below. An exemption list
# is the hatch that becomes a hiding place: whoever adds an entry has a
# reason that day, and nobody reads it again. So an entry naming a file
# that no longer exists is itself a failure — the exemption outlived
# its subject and would go on excusing a name that means nothing.
DRIVEN_ELSEWHERE: dict[str, str] = {}

MIN_SELFTESTS = 10


def callers_text(root: str) -> str:
    out = []
    for rel in CALLERS:
        path = os.path.join(root, rel)
        if os.path.isfile(path):
            out.append(open(path, encoding="utf-8").read())
    # Plus any e2e script, which may drive a self-test as a step.
    dev = os.path.join(root, DEV)
    for name in sorted(os.listdir(dev)):
        if name.endswith(".sh"):
            out.append(open(os.path.join(dev, name), encoding="utf-8").read())
    return "\n".join(out)


def main() -> int:
    dev = os.path.join(ROOT, DEV)
    if not os.path.isdir(dev):
        print("a-selftest-nobody-runs: CANNOT RUN")
        print(f"  - {DEV} is not in this tree")
        return 2

    text = callers_text(ROOT)
    selftests = sorted(n for n in os.listdir(dev) if n.endswith(".test.py"))
    problems: list[str] = []

    for name in selftests:
        stem = name[: -len(".py")]  # e.g. contract-scan.test
        if stem in text or name in text:
            continue
        if name in DRIVEN_ELSEWHERE:
            continue
        problems.append(
            f"{DEV}/{name} is invoked by nothing. It exists to prove its gate can "
            f"still go red, and that proof is worth what it was worth the last time "
            f"it ran — which, wired nowhere, is the day somebody ran it by hand."
        )

    # The exemptions, checked against reality. An entry for a file that
    # is gone excuses nothing and hides that it excuses nothing.
    for name, reason in DRIVEN_ELSEWHERE.items():
        if not os.path.isfile(os.path.join(dev, name)):
            problems.append(
                f"{DEV}/{name} is exempt (\"{reason}\") and does not exist. An "
                f"exemption that outlived its subject reads as a considered "
                f"decision and is a leftover."
            )
        elif not reason.strip():
            problems.append(
                f"{DEV}/{name} is exempt with no reason given. An exemption "
                f"nobody can read is indistinguishable from an oversight."
            )

    if len(selftests) < MIN_SELFTESTS:
        problems.append(
            f"only {len(selftests)} self-test(s) found in {DEV}. Below "
            f"{MIN_SELFTESTS} this is not reading the directory, and finding none "
            f"to complain about is not the same as there being none."
        )

    if problems:
        print("a-selftest-nobody-runs: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    print(
        f"a-selftest-nobody-runs: clean — {len(selftests)} self-test(s), each "
        f"invoked by preflight, CI, the ship, or a named e2e"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
