#!/usr/bin/env python3
"""Every device gate drives more than one, privileged, subject.

The corpus ran twenty flows and the Android behaviour gate ran a dozen
assertions, and between them they drove exactly two apps: Settings on
each platform. Twenty flows is not twenty subjects — it is one subject
walked twenty ways, and a system app is not an ordinary one. It is
preinstalled, it has stable accessibility ids, its windows are the
system's own.

So a defect that only shows on an ordinary app was invisible to every
gate here at once. That is what happened: a consumer reported `/tree`
carrying the SystemUI windows and not their app's, while every device
gate in this repository was green, because not one of them had ever
driven an app that was not Settings.

The fixtures existed the whole time — `test-fixtures/demo-app` on iOS
since v1, `test-fixtures/android-app` added the day this was written —
and no gate referenced either. Coverage is not decided by how many times
a gate runs. It is decided by how many different subjects it runs
against.

This does not check that the gates pass. It checks that they are
pointed at something other than the platform's own app, which is a
different question and the one nobody was asking.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CORPUS = os.path.join(ROOT, "scripts", "release", "stress-corpus")
ANDROID_GATE = os.path.join(ROOT, "scripts", "release", "android-behaviour-gate.sh")

# What "privileged" means, concretely: shipped by the platform vendor as
# part of the OS image. Prefixes rather than a list of app ids — the
# point is the category, and a list of specific system apps would need
# maintaining every time one is used.
SYSTEM_PREFIXES = ("com.apple.", "com.android.", "com.google.android.")

# A floor, for the same reason android-gate-scan has one: a path typo
# that finds no flows would satisfy every check below by finding no
# violations either, and report coverage for nothing.
MIN_FLOWS = 10

problems: list[str] = []


def corpus_app_ids() -> list[tuple[str, str]]:
    """(flow file, appId) for every flow in the corpus."""
    out = []
    if not os.path.isdir(CORPUS):
        problems.append(f"no corpus at {CORPUS}")
        return out
    for name in sorted(os.listdir(CORPUS)):
        if not name.endswith((".yaml", ".yml")):
            continue
        path = os.path.join(CORPUS, name)
        with open(path) as fh:
            for line in fh:
                m = re.match(r"^appId:\s*(\S+)", line)
                if m:
                    out.append((name, m.group(1).strip("\"'")))
                    break
    return out


def android_gate_packages() -> list[str]:
    """Packages the Android behaviour gate drives.

    Read out of the gate rather than declared here: a copy of what it
    drives is a copy that goes stale, and the drift would be invisible
    in exactly the direction this gate exists to catch.
    """
    if not os.path.isfile(ANDROID_GATE):
        problems.append(f"no Android behaviour gate at {ANDROID_GATE}")
        return []
    text = open(ANDROID_GATE).read()
    found = set()
    # Assignments (APP="com.android.settings") and anything named on an
    # `am start -n <pkg>/<activity>` line.
    for m in re.finditer(r'^\s*[A-Z_]*APP[A-Z_]*="([a-z][a-z0-9_.]+)"', text, re.M):
        found.add(m.group(1))
    for m in re.finditer(r"am start\s+-n\s+([a-z][a-z0-9_.]+)/", text):
        found.add(m.group(1))
    return sorted(found)


def ordinary(app_id: str) -> bool:
    return not app_id.startswith(SYSTEM_PREFIXES)


flows = corpus_app_ids()
if len(flows) < MIN_FLOWS:
    problems.append(
        f"found only {len(flows)} corpus flows with an appId (floor {MIN_FLOWS}) — "
        "a scan that finds nothing agrees with any corpus at all"
    )

corpus_ordinary = sorted({app for _, app in flows if ordinary(app)})
if flows and not corpus_ordinary:
    subjects = sorted({app for _, app in flows})
    problems.append(
        f"the corpus drives {len(flows)} flows and not one ordinary app — "
        f"every appId is the platform's own: {', '.join(subjects)}. "
        "A fixture app is in test-fixtures/demo-app; a flow that drives it "
        "makes the corpus twenty flows against two subjects instead of one."
    )

android_pkgs = android_gate_packages()
android_ordinary = [p for p in android_pkgs if ordinary(p)]
if android_pkgs and not android_ordinary:
    problems.append(
        "the Android behaviour gate drives only the platform's own apps: "
        f"{', '.join(android_pkgs)}. A fixture is in test-fixtures/android-app; "
        "drive it alongside Settings rather than instead of it — the Settings "
        "assertions cover the system window layer, which the fixture cannot."
    )

if problems:
    print("gate-subject-diversity: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print(
    f"gate-subject-diversity: clean — corpus drives {len(corpus_ordinary)} ordinary "
    f"app(s) across {len(flows)} flows; the Android gate drives "
    f"{len(android_ordinary)} ordinary app(s)"
)
