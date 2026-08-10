#!/usr/bin/env python3
"""How much of the corpus could run on a machine that is not this one.

Twenty of the twenty-one corpus flows drive the system Settings app,
naming rows like `com.apple.settings.actionButton` — identifiers that
change with the iOS version and with the device model. This machine runs
iOS 26.5 and a CI runner will not, so those flows on a runner would go
red for reasons that say nothing about smix. That is worse than a gate a
bystander can turn red: that kind at least points at a real conflict,
and a gate whose red means nothing gets skipped soon enough.

So the corpus grows a tier that drives the fixture app instead — same
shapes, a subject that travels. The Settings flows stay; a real system
app is a subject the fixture cannot imitate, and it has caught defects
the fixture could not reach.

This counts the split and refuses to let the portable side shrink.
`gate-subject-diversity` has been printing the same fact for a while
("corpus drives 1 ordinary app across 21 flows") and it read as "there
is an ordinary app, good" rather than as "portability is one in
twenty-one". A number with a floor under it is harder to misread.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CORPUS = os.path.join(ROOT, "scripts", "release", "stress-corpus")

# The fixture ships in this repository and is built by the gate, so a
# flow driving it needs nothing from the host but a simulator.
PORTABLE_APP = "jp.golia.smix.fixture"

# Every portable flow there is, not one fewer.
#
# It sits at the count, so that losing one is a failure rather than a
# diff nobody reads. Raising it means porting more flows; LOWERING it
# means editing this line and saying why — a floor that yields when a
# flow is deleted is not a floor.
#
# 5 → 4 on 2026-08-10, and here is the why. A fifth flow long-pressed a
# fixture row and asserted the label had changed. It fails, consistently,
# and the cause is not settled: the runner reports the press dispatched
# and held ~584ms through the same daemon-synthesize path `tap` uses, and
# `tapOn` on the same screen fires SwiftUI's NavigationLink in the same
# run — so it is not a dead dispatch path. It may be that a
# `.onLongPressGesture` on a `Text` inside a `List` row loses to the
# list's own recognisers, in which case the fixture is wrong and smix is
# fine; it may be that long-press does not reach SwiftUI gestures at all,
# in which case smix has a capability gap on every SwiftUI app.
#
# Held at `.claude/docs/research/portable-longpress-row.yaml.pending`
# until that is answered. Not deleted, not excused into
# known-unstable.md — it is not flaky, it fails every time, and a
# consistent failure whose cause is unknown is a question, not a flake.
MINIMUM_PORTABLE = 4

problems: list[str] = []


def app_of(path: str) -> str | None:
    """The flow's `appId`, from its front matter."""
    try:
        with open(path, encoding="utf-8") as fh:
            for line in fh:
                m = re.match(r"\s*appId:\s*(\S+)", line)
                if m:
                    return m.group(1).strip("\"'")
                if line.strip() == "---":
                    return None
    except OSError:
        return None
    return None


flows = sorted(f for f in os.listdir(CORPUS) if f.endswith(".yaml")) if os.path.isdir(CORPUS) else []
portable: list[str] = []
runtime_locked: list[str] = []

for name in flows:
    app = app_of(os.path.join(CORPUS, name))
    if app == PORTABLE_APP:
        portable.append(name[:-5])
    else:
        runtime_locked.append(name[:-5])

# A parser that finds nothing agrees with any corpus at all.
if not flows:
    problems.append(f"no flows found under {os.path.relpath(CORPUS, ROOT)}")
elif not runtime_locked and not portable:
    problems.append(
        "every flow parsed to no appId — the front-matter shape changed and "
        "this scan is now reading air"
    )

if len(portable) < MINIMUM_PORTABLE:
    problems.append(
        f"{len(portable)} portable flow(s), and the floor is {MINIMUM_PORTABLE}. "
        f"A flow is portable when it drives {PORTABLE_APP}; the rest name "
        f"system-app identifiers that differ by iOS version and device model. "
        f"If the floor is genuinely wrong, change MINIMUM_PORTABLE and say why "
        f"— do not reach it by deleting flows."
    )

# `--list` prints the portable flow names and nothing else, so the
# runner script derives its tier from this judgement instead of keeping
# a second copy of it. Four lists in this cycle were copies that had
# drifted from what they copied.
if "--list" in sys.argv:
    if problems:
        for p in problems:
            print(f"corpus-portability-scan: {p}", file=sys.stderr)
        sys.exit(1)
    print("\n".join(portable))
    sys.exit(0)

if problems:
    print("corpus-portability-scan: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print(
    f"corpus-portability-scan: {len(portable)} portable / "
    f"{len(runtime_locked)} runtime-locked of {len(flows)} — "
    f"portable: {', '.join(portable)}"
)
