#!/usr/bin/env python3
"""Every third-party action is pinned to a commit, and says which version.

A workflow that says `uses: someone/action@v2` runs whatever the owner
of that tag most recently moved it to. The same commit of this
repository therefore passes today and fails tomorrow with nothing here
having changed — and when it fails, the log cannot say which version
ran, because the reference does not name one.

That is not hypothetical. On 2026-08-21 two prebuild jobs, on different
operating systems, both died inside `oven-sh/setup-bun@v2` in a run
whose only change was a test fixture. Nothing in this repository could
say what had moved.

A moving tag is also the supply-chain shape: whoever can push a tag can
run code in this workflow.

So each `uses:` names a commit, with a trailing comment naming the
version that commit was. The comment is required, not decorative — a
forty-character hex string with nothing beside it tells a reader
nothing, and the next person to update it has no idea what they are
updating from.

Exempt: actions under this repository's own path, which move with it.

Usage:
  scripts/dev/actions-are-pinned.py [repo-root]
"""

import os
import re
import sys

ROOT = (
    sys.argv[1]
    if len(sys.argv) > 1
    else os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
)
WORKFLOWS = os.path.join(".github", "workflows")

# `- uses:` and `uses:` are both ordinary YAML here — a step that is a
# bare action begins the list item with it, and one with a `name:` puts
# it on a following line. The first draft of this matched only the
# second form and read 3 of 29 references while reporting clean, which
# is the failure this whole file is about, one level up.
USES = re.compile(r"^\s*-?\s*uses:\s*(\S+)\s*(?:#\s*(.*))?$")
SHA = re.compile(r"^[0-9a-f]{40}$")

# A workflow directory with no `uses:` at all would agree with every
# rule here. Both sides of that need a floor.
# 29 today. A floor near the real number is what turns "the regex
# stopped matching" from a silent pass into a red.
MIN_USES = 20


def main() -> int:
    wf_dir = os.path.join(ROOT, WORKFLOWS)
    if not os.path.isdir(wf_dir):
        print("actions-are-pinned: CANNOT RUN")
        print(f"  - {WORKFLOWS} is not in this tree")
        return 2

    problems: list[str] = []
    seen = 0

    for name in sorted(os.listdir(wf_dir)):
        if not name.endswith((".yml", ".yaml")):
            continue
        rel = os.path.join(WORKFLOWS, name)
        for lineno, line in enumerate(
            open(os.path.join(wf_dir, name), encoding="utf-8"), 1
        ):
            m = USES.match(line.rstrip("\n"))
            if not m:
                continue
            ref, comment = m.group(1), (m.group(2) or "").strip()
            # A local action, or a reusable workflow in this repo.
            if ref.startswith(("./", ".github/")):
                continue
            seen += 1
            if "@" not in ref:
                problems.append(f"{rel}:{lineno}: `{ref}` names no ref at all")
                continue
            _, at = ref.rsplit("@", 1)
            if not SHA.match(at):
                problems.append(
                    f"{rel}:{lineno}: `{ref}` is pinned to a moving ref. Whoever "
                    f"owns that tag decides what runs here, and a failure cannot "
                    f"say which version it was. Pin the commit and name the "
                    f"version in a trailing comment."
                )
            elif not comment:
                problems.append(
                    f"{rel}:{lineno}: `{ref}` is pinned to a commit with nothing "
                    f"beside it. Forty hex characters tell a reader nothing and "
                    f"leave the next update with no idea what it updates from — "
                    f"add `# <version>`."
                )

    if seen < MIN_USES:
        problems.append(
            f"only {seen} `uses:` found across {WORKFLOWS}. Below {MIN_USES} this "
            f"is not reading the workflows — every rule here is vacuously true on "
            f"a file with no actions in it."
        )

    if problems:
        print("actions-are-pinned: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    print(f"actions-are-pinned: clean — {seen} action reference(s), each a commit that says which version it is")
    return 0


if __name__ == "__main__":
    sys.exit(main())
