#!/usr/bin/env python3
"""No surface may say a successful tap proves the touch arrived.

What `HitChain.at()` computes is geometry: every named element whose
frame contains the point, as the accessibility snapshot describes it. It
is evidence about the aim. It is not, and has never been, evidence that
the app received anything. On a landscape screen every tap reports the
button it aimed at and the framebuffer does not change by one pixel,
because the point is computed in the app's space and stamped with the
device's.

Two guide passages asserted the stronger claim, and one of them told the
reader where to go looking when it failed:

    A green `tapOn` means "a touch reached the element it aimed at" …
    if the screen did not change, the no-op is downstream in the app,
    not in the harness.

That sentence sends somebody into their own code for a defect in ours.
It is the expensive kind of wrong: confident, specific, and pointing
away.

So the rule has two halves, because a ban on words is not a claim about
anything (`.claude/rule/empty-predicate.md`): the arrival-claiming
phrasings must be absent, and the aim wording must be present. A surface
that says nothing at all would satisfy the first half alone.
"""

import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))

# Surfaces a reader meets. The CLI line is what they see per tap; the
# guides are what they consult when it surprises them.
SURFACES = [
    "crates/smix-cli/src/act.rs",
    "crates/smix-driver/src/lib.rs",
    "docs/ai-guide/04-actions.md",
    "docs/ai-guide/12-authoring.md",
]

# Phrasings that assert arrival. Each is a claim C10 refuted by
# measurement, not a style preference.
FORBIDDEN = [
    (r"landed inside", "says the touch landed, which is arrival — it proves the aim"),
    (r"touch (reached|arrived at|got to)", "asserts the touch reached the element"),
    (r"touch landed inside", "asserts arrival"),
    (
        r"no-?op is downstream in the app",
        "tells the reader the app is at fault when the harness can be",
    ),
]

# The other half. Without it, deleting every sentence about taps passes.
# Anchored on the sentence that carries the claim, not on the word
# appearing anywhere in the file. Both guides discuss aiming in several
# places, so `aim` alone stayed satisfied after the one sentence that
# had to say it stopped saying it — the mutation run caught that.
REQUIRED = [
    ("crates/smix-cli/src/act.rs", r"aimed inside", "the per-tap line must say what it verified"),
    (
        "docs/ai-guide/04-actions.md",
        r"the aim was inside",
        "the actions guide must say success is about the aim, in the sentence that defines it",
    ),
    (
        "docs/ai-guide/12-authoring.md",
        r"the point aimed at was inside",
        "the authoring guide must say success is about the aim, in the sentence that defines it",
    ),
]


def main() -> int:
    problems = []
    read = 0

    for rel in SURFACES:
        path = os.path.join(REPO, rel)
        if not os.path.exists(path):
            problems.append(f"{rel} is listed here and does not exist — the list has gone stale")
            continue
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
        read += 1
        for pattern, why in FORBIDDEN:
            for m in re.finditer(pattern, text, re.IGNORECASE):
                line = text[: m.start()].count("\n") + 1
                problems.append(f"{rel}:{line} {why}: {m.group(0)!r}")

    for rel, pattern, why in REQUIRED:
        path = os.path.join(REPO, rel)
        if not os.path.exists(path):
            continue
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
        if not re.search(pattern, text, re.IGNORECASE):
            problems.append(f"{rel} {why} — an absent claim is not a correct one")

    # A scan that read nothing agrees with everything.
    if read < len(SURFACES):
        problems.append(f"only {read} of {len(SURFACES)} surfaces were read")

    if problems:
        print("a-tap-proves-aim-not-arrival: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    print(
        f"a-tap-proves-aim-not-arrival: clean — {read} surfaces, "
        f"{len(FORBIDDEN)} arrival claims absent, {len(REQUIRED)} aim statements present"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
