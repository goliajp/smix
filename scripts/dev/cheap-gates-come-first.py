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

# Steps that make something a later step consumes. Their position is a
# dependency rather than a choice, and their measured cost is whatever
# the build cache happened to hold: `cargo build --release` read 0s on
# a warm run and 28s on a cold one, and this check asked for the 0s one
# to be moved to the front. A build is not a judgement — nothing about
# it fails cheaply and early, which is the whole premise here.
#
# Each is verified to still exist in ship.sh below, so a renamed or
# deleted step cannot leave a line here excusing nothing in particular.
PRODUCES = {
    "every cell is declared": "it asks the compiled verb-by-form table what it "
    "says, so it needs the adapter built — one second here, two and a half "
    "minutes at the front of the run",
    "selector matrix in the guide": "same compiled table, same reason",
    "cargo build -p smix-cli --release (for corpus gate)": "the corpus and android "
    "behaviour gates drive the binary it writes",
    "android unit tests + androidTest compile (sdk + app; compiles kotlin bindings)": "the "
    "kotlin bindings it compiles are what the publish leg publishes",
    "SmixRunner UITest build": "the corpus gate drives the runner it builds",
}

# The permission to publish, rather than a judgement about the tree.
#
# Every gate in this profile ran because it passed, so there is nowhere
# earlier to put anything: "behind the smoke gate" is where the whole
# ship is. Counting its minutes towards what a later judgement could
# have been paid before would ask for gates to run ahead of the thing
# that allows them to run at all.
#
# It is also the one step whose cost is a coin toss on a stamp: it
# re-runs when the last pass is over an hour old, so it read six minutes
# on this run and nothing on the one before. Verified to still exist in
# ship.sh with the others below.
PERMISSION = {
    "smoke gate stale or missing — running smoke first": "it is the permission to "
    "publish; every gate after it runs because it passed",
}


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

    # The exemptions are checked from the other side: a name here that
    # ship.sh no longer logs is excusing nothing, and would go on
    # excusing it silently.
    ship = os.path.join(
        os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))),
        "scripts",
        "release",
        "ship.sh",
    )
    if os.path.isfile(ship):
        ship_src = open(ship, encoding="utf-8").read()
        for name, why in {**PRODUCES, **PERMISSION}.items():
            if name not in ship_src:
                problems.append(
                    f"`{name}` is exempt on the grounds that {why} — but ship.sh no "
                    f"longer logs a step by that name. Re-verify the exemption."
                )

    # How many rows the comparison actually looked at. A check that
    # examined nothing and printed "clean" is the shape this repository
    # keeps finding: found by making the comparison always false, which
    # left the summary unchanged.
    judged = 0
    spent = 0
    for secs, name in rows:
        if name in PERMISSION:
            # Not added to `spent`: see PERMISSION.
            continue
        if name in SELF or name in PRODUCES:
            # Not added to `spent`, for the reason written above PRODUCES:
            # a build is not a judgement, so its minutes are not minutes a
            # later judgement could have been paid before. Counting them
            # asked the device gates to run ahead of the binary they drive
            # -- five runs of chasing that, and the last one had them five
            # minutes late behind their own dependency.
            continue
        judged += 1
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

    # The comparison has to have run. Exemptions could grow to cover the
    # whole profile, or the condition could stop being asked, and either
    # way the sentence below would go on saying nothing is waiting.
    if judged < MIN_GATES:
        print("cheap-gates-come-first: FAIL")
        print(
            f"  - only {judged} of {len(rows)} rows were compared against the "
            f"budget; the rest are exempt or were never asked. A verdict about "
            f"ordering that examined {judged} steps is not one."
        )
        return 1

    print(
        f"cheap-gates-come-first: clean — {len(rows)} gates ({judged} judged), "
        f"{spent // 60}m total, "
        f"no seconds-long judgement waiting behind minutes of work"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
