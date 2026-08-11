#!/usr/bin/env python3
"""Teardown puts back what you changed; it does not impose a state.

A device e2e that shuts down a simulator it did not boot is taking away
something it was lent. Run alone that looks like tidiness — the device
ends idle, the next person starts clean. Run in a suite it is the
mechanism by which one script fails four others: `v2.3-c8` resolved the
shared dev sim from an alias, drove it, and shut it down in teardown,
and every later script needing that sim failed with "Unable to lookup in
current state: Shutdown". Four of the eight failures in the first tier
run passed when run alone.

Seven scripts do this, at nine places, and none of them record whether
the device was already booted when they arrived. That is not one script
written badly; it is a suite with no agreement about who owns the
device.

So: a script may shut a device down if it booted it. The check is that
the script knows which case it is in — some record of the state on
entry — not any particular way of doing it. `smix sim boot` on an
already-booted device is a no-op, so "boot it and shut it down" is not
a way to acquire the right.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DEV = os.path.join(ROOT, "scripts", "dev")

# Shutting a device down, in the forms this repo writes it. `shutdown
# all` is a different sin and a different gate refuses it.
SHUTS_DOWN = re.compile(r"(?:sim shutdown|simctl shutdown)\s+(?!all\b)\S")

# Some record of what the device was doing when the script arrived, and
# the record has to be WRITTEN, not merely declared.
#
# The first version of this matched the name anywhere, and passed three
# scripts where the variable was initialised to `no` and never assigned
# again — the guard read "we did not boot it" every time and shut the
# device down exactly as before. A scan satisfied by a spelling, and a
# guard that was a comment with an `if` around it. Requiring an
# assignment to a truthy value is what makes the check about behaviour.
KNOWS_PRIOR_STATE = re.compile(
    r"(?:was_booted|WAS_BOOTED\w*|already[_ ]booted|booted_on_entry|"
    r"BOOTED_BEFORE|we_booted|WE_BOOTED\w*|booted_by_us)\s*=\s*(?:yes|true|1)\b",
)

# The scripts that shut a device down, named rather than discovered.
#
# Discovery alone answered "every script I found has a guard", which is
# also what it says when a script stops being found. A script that drops
# out of the set — renamed, or its shutdown moved behind a helper this
# regex does not read — would leave the count quietly one lower and the
# verdict still clean. The set found must equal this one, both ways.
SHUTS_DOWN_SCRIPTS = {
    "v2.12-c3-federation-single-node-e2e.sh",
    "v2.12-c4-federation-two-node-e2e.sh",
    "v2.12-c5-federation-cli-e2e.sh",
    "v2.3-c6-lease-reconcile-e2e.sh",
    "v2.3-c7-admission-e2e.sh",
    "v2.3-c8-recording-reconcile-e2e.sh",
    "v2.3-c9-ledger-teardown-e2e.sh",
    "v3.1-c2-machine-lease-e2e.sh",
}

# Scripts that name a shutdown without performing one, and why.
#
# `v2.13-c6` writes `xcrun simctl shutdown all` on purpose: it is the
# thing the guard under test must refuse. Listed, so that "it does not
# shut anything down" is a claim somebody made rather than a gap.
NOT_A_SHUTDOWN = {
    "v2.13-c6-guard-e2e.sh": "the `shutdown all` in it is the guard's target, not a teardown",
}

problems: list[str] = []
checked = 0
careful = 0
found: set[str] = set()

for name in sorted(os.listdir(DEV)) if os.path.isdir(DEV) else []:
    if not name.endswith("-e2e.sh"):
        continue
    path = os.path.join(DEV, name)
    try:
        body = open(path, encoding="utf-8").read()
    except OSError:
        continue
    code = "\n".join(l for l in body.splitlines() if not l.lstrip().startswith("#"))
    hits = [l for l in code.splitlines() if SHUTS_DOWN.search(l)]
    if not hits:
        continue
    checked += 1
    found.add(name)
    if KNOWS_PRIOR_STATE.search(code):
        careful += 1
        continue
    problems.append(
        f"{name} shuts a device down without recording whether it was already "
        f"booted on arrival. Alone that reads as tidiness; in a suite it is how "
        f"one script fails the ones after it.\n      {hits[0].strip()}"
    )

for missing in sorted(SHUTS_DOWN_SCRIPTS - found):
    if os.path.isfile(os.path.join(DEV, missing)):
        problems.append(
            f"{missing} is listed as shutting a device down and no longer reads "
            f"as one. Either its teardown moved somewhere this scan cannot see "
            f"it — in which case the guard is no longer checked — or it stopped "
            f"shutting devices down, in which case take it off the list."
        )
    else:
        problems.append(
            f"{missing} is on the list and does not exist. A list naming a file "
            f"that is gone stops being a list of anything."
        )

for extra in sorted(found - SHUTS_DOWN_SCRIPTS):
    problems.append(
        f"{extra} shuts a device down and is not on the list. Add it, or say in "
        f"NOT_A_SHUTDOWN why what it writes is not a teardown."
    )

# A scan that reads no scripts agrees with any suite at all.
if checked == 0:
    problems.append(
        "no e2e script was found shutting a device down — the shape changed and "
        "this scan is now reading air"
    )

if problems:
    print("teardown-restores-scan: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print(
    f"teardown-restores-scan: clean — {careful} script(s) shut a device down and "
    f"know whether they booted it; {len(NOT_A_SHUTDOWN)} name one without "
    f"performing it"
)
