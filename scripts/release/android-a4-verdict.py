#!/usr/bin/env python3
"""Is an ordinary app's window in the tree, and readable?

Two failures look identical from `/tree` alone — a window that is not
attached for accessibility, and a window attached with a root the walk
cannot read — and `/windows` exists to tell them apart. This is the
judgement over that payload.

It was inline in `android-behaviour-gate.sh` until 7.0, and it had the
one defect a verdict must not have: it could not report the thing it was
written to find.

A window whose root cannot be read **cannot name its package** — the
package comes from the root node. So the "attached but unreadable" case
never reached the check for it, which only looked at windows already
matched by package; it fell into "no window belongs to this app"
instead. And building that message crashed on
`sorted({..., None, "com.android.systemui"})`, because `None` does not
compare with `str`. The gate died with a `TypeError` where its verdict
should have been — a red that is a stack trace is a red nobody can act
on, which is the whole reason this repo writes verdicts as sentences.

So: readability is asked FIRST, over every window, before anything is
matched by name.

Usage:  android-a4-verdict.py <windows.json> <app-id>
Exit:   0 with the finding on stdout; 1 with the reason on stderr.
"""

import json
import sys
from verdict_io import read_json

# What a window is called when it cannot say. Not "None", and not an
# empty string: the reader needs to know the blank is a symptom rather
# than a missing field in the payload.
UNNAMED = "«root unreadable, so it cannot say»"


def main() -> int:
    doc = read_json(sys.argv[1], "/windows")
    app = sys.argv[2]
    rows = doc.get("windows", [])

    if not rows:
        print(
            "A4: /windows listed no windows at all — nothing here proves anything",
            file=sys.stderr,
        )
        return 1

    # Asked before any matching by name, because an unreadable root is
    # exactly what stops a window from having a name to match.
    unreadable = [r for r in rows if not r.get("rootReadable")]
    mine = [r for r in rows if r.get("package") == app]
    seen = sorted({(r.get("package") or UNNAMED) for r in rows})

    if not mine:
        if unreadable:
            print(
                f"A4: no window names {app}, and {len(unreadable)} of {len(rows)} "
                "have a root the walk could not read. A window whose root is "
                "unreadable cannot report its package, so one of those is very "
                f"likely {app}'s: present on screen and absent from the tree. "
                f"Attached: {seen}",
                file=sys.stderr,
            )
        else:
            print(
                f"A4: no window belongs to {app}. Attached: {seen}. Its window is "
                "not attached for accessibility — which reads, from /tree alone, "
                "exactly like an app with no accessibility nodes.",
                file=sys.stderr,
            )
        return 1

    mine_unreadable = [r for r in mine if not r.get("rootReadable")]
    if mine_unreadable:
        print(
            f"A4: {app} has {len(mine_unreadable)} window(s) attached whose root "
            "could not be read, so they are absent from the tree while present "
            "on screen.",
            file=sys.stderr,
        )
        return 1

    print(f"  A4a: {app} has {len(mine)} readable window(s) among {len(rows)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
