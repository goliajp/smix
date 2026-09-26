#!/usr/bin/env python3
"""A reply written is not a reply sent.

Six consumer threads waited — one of them for a month — while the
answers sat in this repository. The letters existed, were correct, and
named fixes that had shipped; nobody put them where the thread was. The
consumer found one of them only because they went looking in our tree.

The two places look almost identical. The `dogfood/` directory of our
development record is what we said; the consumer's own thread directory
is where they are waiting. Only one of them gets read by the person who
asked.

So every reply says where it went, in a line the file carries:

    <!-- delivered: /Users/…/<consumer>/<their thread directory>/smix-reply-<date>.md -->
    <!-- delivered: no — superseded by the 2026-08-25 letter, which covers it -->

and this checks it. Where the thread's directory is on this machine, the
delivered file must actually be there — "I wrote the line" is not the
claim being made. Where the consumer's tree is absent, it says so rather
than passing. Our side is read from the development record, named by
SMIX_DEV_RECORD (or the argument).

`no` needs a reason with something in it. An empty one would satisfy this
check forever while describing nothing — which is the shape this repo has
watched an exemption take twice.

Usage:  a-reply-nobody-sent.py [record-root]
"""

import glob
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _dev_record  # noqa: E402

RECORD = _dev_record.root(sys.argv[1] if len(sys.argv) > 1 else None)
DOGFOOD = os.path.join(RECORD, "dogfood") if RECORD else None
LINE = re.compile(r"<!--\s*delivered:\s*(.+?)\s*-->")
# Long enough that "no — n/a" does not pass as a reason.
REASON_MIN = 20


def is_a_reply(path):
    """A letter FROM us, decided from the file rather than a list.

    The directory holds both directions — what they sent and what we
    said — and only our side can be undelivered. The filename cannot
    tell them apart: `<consumer>-reply-to-smix-…` is a reply TO us. So the
    first line decides, because a letter says who it is from:

        `# smix → <them> …`    ours
        `# 回给 <them> …`       ours, written in Chinese
        `# smix ← <them> …`    theirs
        `# <them> → smix …`    theirs

    Derived rather than listed: a hand-kept list of which files are ours
    is the second copy that goes stale (`code/derive-dont-copy`).
    """
    try:
        first = open(path, encoding="utf-8").readline().strip()
    except OSError:
        return False
    if "→ smix" in first or "← " in first or "-> smix" in first:
        return False
    return first.startswith("# smix →") or first.startswith("# 回给")


def main():
    if RECORD is None:
        print(_dev_record.not_named("reply-sent"))
        return 2
    if not os.path.isdir(DOGFOOD):
        print(
            f"reply-sent: CANNOT RUN — {DOGFOOD} does not exist. "
            "Green must not mean read nothing."
        )
        return 2

    replies = sorted(p for p in glob.glob(os.path.join(DOGFOOD, "*.md")) if is_a_reply(p))
    if not replies:
        # A sweep that finds nothing agrees with every tree there is.
        print(f"reply-sent: CANNOT RUN — no reply letters under {DOGFOOD}")
        return 2

    unmarked, hollow, missing, verified, unverifiable, declined = [], [], [], 0, 0, 0
    for path in replies:
        rel = os.path.relpath(path, RECORD)
        text = open(path, encoding="utf-8").read()
        found = LINE.findall(text)
        if not found:
            unmarked.append(rel)
            continue
        # A letter can go to more than one thread — the 08-24 answer went
        # to two — so every line it carries is checked, not just the first.
        if len(found) > 1 and any(w.lower().startswith("no") for w in found):
            hollow.append(f"{rel}: says both 'no' and a destination")
            continue
        where = found[0]
        if where.lower().startswith("no"):
            reason = where[2:].lstrip(" —-:")
            if len(reason) < REASON_MIN:
                hollow.append(f"{rel}: 'no' with nothing after it ({where!r})")
            else:
                declined += 1
            continue
        for where in found:
            target = os.path.expanduser(where)
            parent = os.path.dirname(target)
            if not os.path.isdir(parent):
                unverifiable += 1
            elif os.path.isfile(target):
                verified += 1
            else:
                missing.append(
                    f"{rel}: says it went to {where}, and that thread is on this "
                    f"machine without it"
                )
        continue
        target = os.path.expanduser(where)
        parent = os.path.dirname(target)
        if not os.path.isdir(parent):
            # The consumer's tree is not on this machine. Not a finding
            # about the letter, and not a pass either.
            unverifiable += 1
        elif os.path.isfile(target):
            verified += 1
        else:
            # The thread IS here and the letter is not in it. This is the
            # thing the gate exists for.
            missing.append(f"{rel}: says it went to {where}, and that thread is on this machine without it")

    print(f"reply-sent: {verified} delivered and checked on this machine")
    if declined:
        print(f"reply-sent: {declined} deliberately not sent, each with a reason")
    if unverifiable:
        print(
            f"reply-sent: {unverifiable} name a thread this machine does not have "
            f"— unchecked here, checked where that tree lives"
        )

    problems = []
    for rel in unmarked:
        problems.append(f"{rel}: says nothing about where it went")
    problems.extend(hollow)
    problems.extend(missing)
    if problems:
        print("\nreply-sent: RED — a letter nobody can show was sent\n")
        for p in problems:
            print(f"  {p}")
        print(
            "\n  Add `<!-- delivered: <path> -->` naming the thread it went to, or\n"
            "  `<!-- delivered: no — <why> -->`. Six threads waited on letters that\n"
            "  existed; one of them for a month."
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
