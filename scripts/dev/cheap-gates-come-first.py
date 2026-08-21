#!/usr/bin/env python3
"""A gate that costs seconds must not sit behind one that costs minutes.

6.3.0 was stopped four times by second-level judgements — a stale
version number, an undefined variable, three platform packages left at
the previous version, an empty gpg key — each of them sitting at gate 50
of 53, and each costing ninety minutes of compiling and device work to
reach. The fix that release was to move them to the front. Nothing has
recorded the cost of the ones that remain, so the next gate added in the
wrong place is invisible until somebody pays for it.

The ship now writes a profile: seconds, then the gate's name, one line
each. This reads it and refuses an ordering where a cheap gate waits
behind an expensive one — with a tolerance, because two gates that both
take a second are not an ordering problem, and the expensive ones are
allowed to be late.

This runs inside the ship, after every gate and before the first
publish leg, where the profile always exists because the ship has just
written it. Outside a ship there is nothing to read, and it says CANNOT
RUN rather than clean — a check that passes when it has read nothing is
not a check, which is the same rule the reconciler refuses an empty
corpus under.

Usage:
  scripts/dev/cheap-gates-come-first.py [profile.tsv]
"""

import os
import sys

PROFILE = sys.argv[1] if len(sys.argv) > 1 else "/tmp/smix-ship-profile.tsv"

# A gate under this is cheap enough that reaching it should not cost
# anything. One that takes longer has earned its place further down.
CHEAP_SECONDS = 5
# How much expensive work a cheap gate may sit behind before it is worth
# saying so. Five minutes is the point where a failed ship stops being a
# re-run and starts being an afternoon.
PATIENCE_SECONDS = 300
# A profile with almost nothing in it is not a profile.
MIN_GATES = 20

# The two entries this check cannot judge, because it is them.
#
# Named exactly rather than matched by pattern: an exemption that
# matches a shape excuses whatever grows into that shape later, and
# these two are the only entries whose lateness is a property of what
# they are. Everything else in the profile is a gate whose position is
# a choice somebody made.
#
# They cost a second between them, so exempting them hides nothing.
SELF = (
    "gate ordering",
    "the ordering gate can still go red",
)


def main() -> int:
    if not os.path.isfile(PROFILE):
        print("cheap-gates-come-first: CANNOT RUN")
        print(f"  - no profile at {PROFILE}. The ship writes one as it goes, so")
        print("    an absent profile inside a ship means the writing broke, not")
        print("    that the ordering is fine. Refusing rather than agreeing:")
        print("    a check that passes when it has read nothing is not a check.")
        return 2

    rows = []
    for line in open(PROFILE, encoding="utf-8"):
        secs, _, name = line.rstrip("\n").partition("\t")
        if not name:
            continue
        try:
            rows.append((int(secs), name))
        except ValueError:
            continue

    if len(rows) < MIN_GATES:
        print("cheap-gates-come-first: FAIL")
        print(
            f"  - the profile has {len(rows)} gate(s), fewer than {MIN_GATES}. "
            f"Either the ship stopped early or the profile is not being written — "
            f"and an ordering check over three lines agrees with everything."
        )
        return 1

    problems = []
    spent = 0
    for secs, name in rows:
        if name in SELF:
            spent += secs
            continue
        if secs <= CHEAP_SECONDS and spent >= PATIENCE_SECONDS:
            problems.append(
                f"`{name}` takes {secs}s and sits behind {spent // 60}m of work. "
                f"A judgement this cheap belongs where it can be paid for in "
                f"seconds — at the front, with the version and credential checks."
            )
        spent += secs

    if problems:
        print("cheap-gates-come-first: FAIL")
        for p in problems:
            print(f"  - {p}")
        print(
            f"  read from {PROFILE}; total {spent // 60}m across {len(rows)} gates"
        )
        return 1

    print(
        f"cheap-gates-come-first: clean — {len(rows)} gates, {spent // 60}m total, "
        f"no seconds-long judgement waiting behind minutes of work"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
