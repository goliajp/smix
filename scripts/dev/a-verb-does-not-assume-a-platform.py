#!/usr/bin/env python3
"""Every runner verb either handles both platforms or says it cannot.

`runner cycle` reads iOS state.json unconditionally. Typed against an
Android device it goes looking in another platform's records, finds
nothing, and answers "no runner recorded — cycle only cycles a known
runner". A consumer reported that sentence as a state problem, because
it reads as one. The verb has never worked there: `fn cycle` does not
exist in the Android runner at all, and the subcommand does not even
take a --device.

That is the shape §9 #1 forbids — a capability that is not available
must be a loud error, never a quiet wrong answer — and the shape v6.2
was about: the platform is a property of the device you named, not an
argument that defaults to iOS.

Three verbs dispatch straight into the iOS path. Two of them are
deliberate and say so here. The third was not, and nothing was watching.

This does not demand that every verb work everywhere. It demands that
each one be on a list with a reason, so the next verb that quietly
assumes a platform is a red rather than a consumer's puzzle.

Usage:
  scripts/dev/a-verb-does-not-assume-a-platform.py [repo-root]
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
MAIN = os.path.join("crates", "smix-cli", "src", "main.rs")

# What counts as not assuming a platform. Three ways a verb can be
# innocent, and each was found by the first run flagging something that
# turned out to be fine:
#
#   - it names the Android path, branches on a platform, or resolves a
#     device (from which the platform is read);
#   - it reads the machine-level lease ledger, which is shared by both
#     platforms by construction (§9 #9);
#   - it talks to the runner over HTTP, which both platforms answer on
#     the same port.
#
# Widened from evidence rather than to make the gate pass: `runner
# forward` does resolve a device, `runner list` reads the shared
# ledger, and `runner list-sessions` asks the runner. Only `cycle` was
# the real thing.
KNOWS_PLATFORM = re.compile(
    r"runner_android|Platform::|platform|resolve_device|serial"
    r"|machine_leases|HttpRunnerClient"
)

# Verbs that reach only one platform on purpose, each with why. An
# entry here is a statement that somebody looked, not permission to
# stop looking.
IOS_ONLY_ON_PURPOSE = {
    "Supervise": (
        "the supervisor tails an xcodebuild process and restarts it; the Android "
        "runner is an instrumentation the platform restarts on its own"
    ),
}

MIN_VERBS = 5


def without_comments(text: str) -> str:
    """Drop `//` comments before matching.

    Load-bearing, not tidiness. The first version read the arm's whole
    text, and the comment explaining why `cycle` takes a platform
    contains the word "platform" — so removing the parameter left the
    gate green. `workflow-scan` carries the same warning for the same
    reason: a checker that matches prose is checking what the code says
    about itself rather than what it does.
    """
    text = "\n".join(re.sub(r"//.*$", "", line) for line in text.splitlines())
    # String literals go too. The refusal message this verb prints
    # contains "--platform android --device <serial>", and a checker
    # that reads it is reading the prose the code emits about itself —
    # the same fault one line up, wearing quotes.
    return re.sub(r'"(?:[^"\\]|\\.)*"', '""', text)


def dispatch_blocks(text: str) -> dict[str, str]:
    """Each `RunnerAction::<Verb>` arm body, by verb."""
    starts = [
        (m.group(1), m.start())
        for m in re.finditer(r"RunnerAction::([A-Z][A-Za-z]*)\s*(?:\{|=>)", text)
    ]
    # Keep only the dispatch arms — the declaration site has no `=>`.
    arms = [(v, i) for v, i in starts if "=>" in text[i : i + 200]]
    out: dict[str, str] = {}
    for n, (verb, i) in enumerate(arms):
        end = arms[n + 1][1] if n + 1 < len(arms) else len(text)
        out.setdefault(verb, text[i:end])
    return out


def main() -> int:
    path = os.path.join(ROOT, MAIN)
    if not os.path.isfile(path):
        print("a-verb-does-not-assume-a-platform: CANNOT RUN")
        print(f"  - {MAIN} is not in this tree")
        return 2

    text = open(path, encoding="utf-8").read()
    arms = dispatch_blocks(text)
    problems: list[str] = []

    if len(arms) < MIN_VERBS:
        problems.append(
            f"found {len(arms)} runner verb dispatch arm(s), fewer than "
            f"{MIN_VERBS}. The reader has stopped matching, and a scan that reads "
            f"nothing finds nothing to complain about."
        )

    for verb, body in sorted(arms.items()):
        if KNOWS_PLATFORM.search(without_comments(body)):
            if verb in IOS_ONLY_ON_PURPOSE:
                problems.append(
                    f"`runner {verb.lower()}` is declared iOS-only "
                    f'("{IOS_ONLY_ON_PURPOSE[verb]}") and now reaches more than one '
                    f"platform. The declaration should go with the limitation."
                )
            continue
        if verb not in IOS_ONLY_ON_PURPOSE:
            problems.append(
                f"`runner {verb.lower()}` dispatches into one platform's path "
                f"without naming a platform or a device, and is not declared as "
                f"single-platform. Typed against the other platform it will read "
                f"the wrong records and answer as though the user's state were "
                f"wrong — which is how `runner cycle` reached a consumer."
            )

    for verb in IOS_ONLY_ON_PURPOSE:
        if verb not in arms:
            problems.append(
                f"`{verb}` is declared iOS-only and is not a runner verb any more. "
                f"An entry that outlived its subject reads as a considered decision."
            )

    if problems:
        print("a-verb-does-not-assume-a-platform: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    print(
        f"a-verb-does-not-assume-a-platform: clean — {len(arms)} runner verbs, "
        f"{len(IOS_ONLY_ON_PURPOSE)} single-platform and each declared with why"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
