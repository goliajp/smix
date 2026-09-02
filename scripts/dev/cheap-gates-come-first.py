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
import re
import sys

PROFILE = sys.argv[1] if len(sys.argv) > 1 else "/tmp/smix-ship-profile.tsv"
# argv[2] is how the self-test hands this gate a ship.sh it has falsified.
SHIP_ARG = sys.argv[2] if len(sys.argv) > 2 else None

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

# Steps that consume something built or booted. PRODUCES is about the
# other direction — a step that makes what a later one reads — and this is
# its mirror: `clippy` shares target/ with the build before it, the v10
# gates drive a release binary against a running emulator. Their position
# is settled by that dependency, not chosen.
#
# The measured cost is the same coin toss PRODUCES describes, and it came
# up the other way once: on one ship clippy read 4m27s and the device
# gates were slower still, so none of them tripped this check. Every cache
# warm, they read 1s, 2s and 5s — the same judgements, now reported as
# cheap ones sitting behind minutes of work. A gate whose verdict flips
# with cache temperature is not measuring what it names.
#
# Derived from ship.sh rather than listed: a step is dependent when the
# commands under its `log` line touch a build product or a device.
DEPENDENT = re.compile(
    r"\bcargo\b|\./gradlew|xcrun simctl|\badb\b|SMIX_BIN|target/release"
    r"|xcodebuild|swift test|--device\b|--serial\b|_DEVICE\b|_SERIAL\b|_SIM\b"
)

# How many of ship.sh's steps must come out dependent for the parse to be
# believable. Far below what it finds today, and far above zero: a regex
# that stopped matching would put every device gate back into the budget
# and read as a stricter check rather than a broken one.
MIN_DEPENDENT = 8


def dependent_steps(ship_src):
    """Step names whose commands touch a build product or a device."""
    out, cur, name = set(), [], None
    for ln in ship_src.splitlines():
        m = re.match(r'^\s*log "(.+?)"', ln)
        if m:
            if name is not None and DEPENDENT.search("\n".join(cur)):
                out.add(name)
            name, cur = m.group(1), []
        elif name is not None:
            cur.append(ln)
    if name is not None and DEPENDENT.search("\n".join(cur)):
        out.add(name)
    return out


# Steps that can only run after the release has gone out.
#
# Publishing itself is no longer here: those lines are logged with `note`
# now, exactly as the comment beside the loop in ship.sh always said they
# should be, so they never reach the profile. An exemption for them would
# excuse the empty set — which reads identically to one that looked.
#
# What remains is `verify what the registries took`: seconds long, and
# last by necessity. Its position is settled by the publish before it, the
# same way PRODUCES and DEPENDENT are settled by a build or a device.
#
# Found from the first real `cargo publish` in ship.sh rather than by
# section name: there is a `# --- publish dag ---` gate four hundred lines
# earlier that is a judgement and must keep facing the budget.
FIRST_PUBLISH = re.compile(r'^\s*\(\s*cd "\$ROOT" && cargo publish -p')


def after_publish_steps(ship_src):
    """Step names that come after the release starts going out."""
    lines = ship_src.splitlines()
    at = next((i for i, ln in enumerate(lines) if FIRST_PUBLISH.match(ln)), None)
    if at is None:
        return None
    out = set()
    for ln in lines[at:]:
        m = re.match(r'^\s*log "(.+?)"', ln)
        if m:
            out.add(m.group(1))
    return out


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
    ship = SHIP_ARG or os.path.join(
        os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))),
        "scripts",
        "release",
        "ship.sh",
    )
    if not os.path.isfile(ship):
        print("cheap-gates-come-first: CANNOT RUN")
        print(f"  - no ship.sh at {ship}. The exemptions are all verified")
        print("    against it, so without it this check would judge every")
        print("    device gate as a misplaced one.")
        return 2
    if True:
        ship_src = open(ship, encoding="utf-8").read()
        for name, why in {**PRODUCES, **PERMISSION}.items():
            if name not in ship_src:
                problems.append(
                    f"`{name}` is exempt on the grounds that {why} — but ship.sh no "
                    f"longer logs a step by that name. Re-verify the exemption."
                )
        # Asked of ship.sh rather than of the profile, because this gate
        # runs in the MIDDLE of a ship: publishing comes after it, so the
        # profile it reads has never yet contained a publish step. Checking
        # there made the gate red on every real run while passing on the
        # complete profile a finished ship leaves behind — which is what it
        # was verified against.
        depends = dependent_steps(ship_src)
        if len(depends) < MIN_DEPENDENT:
            problems.append(
                f"only {len(depends)} of ship.sh's steps parse as depending on "
                f"something built or booted, fewer than {MIN_DEPENDENT}. The "
                f"reader has stopped matching, and every device gate is back in "
                f"a budget it cannot be moved out of."
            )

        after_publish = after_publish_steps(ship_src)
        if after_publish is None:
            problems.append(
                "no `cargo publish -p` call found in ship.sh, so this check "
                "cannot tell which steps come after the release. It would "
                "otherwise judge the post-publish verification as a misplaced "
                "gate — which is where it must be."
            )
            after_publish = set()

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
        if name in after_publish:
            # Not added to `spent`: see FIRST_PUBLISH.
            continue
        if name in depends:
            # Not added to `spent`: see DEPENDENT. Its seconds are not
            # seconds an earlier judgement could have been paid before —
            # moving it earlier means building or booting earlier, which
            # is not earlier.
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
