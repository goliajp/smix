#!/usr/bin/env python3
"""Check that no Android test task sits outside every gate.

The three Android defects of 2026-07-20/21 — a force-key-events header
nobody read, an empty set_target_bundle_id, and the README's placeholder
package baked into the view-id lookup — all survived for the same
reason. CI and ship.sh ran `:sdk:testDebugUnitTest`, qualified to one
module, so the app module's eight JVM test files were run by nothing at
all. Among them were the five written that week to cover those very
defects: the tests existed, passed locally, and guarded nothing.

A hand-kept list of "which task is wired where" would rot the same way
the qualified task name did. So the module list is re-derived on every
run from settings.gradle.kts and the source sets on disk, and compared
against the gradle invocations the three gate files actually contain. A
module added next year is inside the gate on arrival, or this fails.

Deliberately does NOT call gradle. The CI job that runs the source gates
is an ubuntu box with no JDK and no Android SDK; a scan that shells out
to ./gradlew would work here and die there, which is the sort of gate
that gets deleted rather than fixed.

Checks:
  1. At least MIN_MODULES modules were found. A path typo that finds
     nothing would otherwise pass every remaining check vacuously —
     the failure mode the no_json_state gate was given a floor for.
  2. Every module with src/test/ has its unit test task covered by all
     three gates. Three, not one: preflight is the local habit, CI is
     the branch, ship.sh is the release. A task missing from ship alone
     is a hole on exactly the path that reaches users.
  3. Every module with src/androidTest/ is either covered the same way
     or named in DEFERRED with the checkpoint that will cover it. A new
     instrumentation suite cannot appear unremarked.
  4. This script is itself invoked by all three gates. adb-guard died of
     precisely this: the script was committed, the line that runs it was
     not.

Exit non-zero on any failure.
"""

import os
import re
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
ANDROID = os.path.join(ROOT, "android-runner")
SETTINGS = "android-runner/settings.gradle.kts"

# Below this, assume the enumeration broke rather than the project shrank.
MIN_MODULES = 2

GATES = (
    "scripts/dev/preflight.sh",
    ".github/workflows/ci.yml",
    "scripts/release/ship.sh",
)

UNIT_TASK = "testDebugUnitTest"
INSTRUMENTATION_TASK = "connectedDebugAndroidTest"

# androidTest source sets no gate runs, each with where that is decided.
#
# The two are not the same kind of thing, and saying so here is the point
# of the entry being prose rather than a boolean. :sdk holds assertions.
# :app holds the runner itself — one @Test that starts the HTTP server
# and blocks forever, which is what `smix runner up --platform android`
# is running. Demanding connectedDebugAndroidTest of it would not fail;
# it would never return, and a release script would hang.
DEFERRED = {
    ":app": "runner host, not a suite — whether it belongs in a gate at all "
            "is C3's call; docs/plan-cold/v2.2-android-behavioural-gate.md",
    ":sdk": "C2 runs it, C3 gates it — "
            "docs/plan-cold/v2.2-android-behavioural-gate.md",
}

# Task names that need no separate demand, and why. Stated rather than
# silently skipped: a reader must be able to tell "covered elsewhere"
# from "never considered".
EXEMPT = {
    "testReleaseUnitTest": "same src/test/ sources as the debug variant",
    "test": "aggregate over both variants; the debug variant is demanded",
    "build": "chains the unit tests already demanded",
    "buildNeeded": "chains the unit tests already demanded",
    "buildDependents": "chains the unit tests already demanded",
    "deviceAndroidTest": "Device Provider path over the same androidTest sources",
    "deviceCheck": "aggregate over deviceAndroidTest",
    "connectedAndroidTest": "aggregate over connectedDebugAndroidTest",
    "connectedCheck": "aggregate over connectedDebugAndroidTest",
}

INCLUDE = re.compile(r'^\s*include\(\s*"(:[A-Za-z0-9_\-:]+)"\s*\)', re.M)
# `./gradlew [flags] task [task…]` — captures the whole task run so a
# bare name and a :module:-qualified one can be told apart.
GRADLEW = re.compile(r"gradlew\s+([^\n|&;>]*)")


def read(rel):
    """Gate file contents with whole-line comments dropped.

    Both file types comment with `#`, and the first version of this scan
    matched raw text — so a CI comment that merely mentioned this script
    by name counted as running it, and ship.sh's note about an old
    `gradlew :sdk:publish` looked like an invocation. Describing a
    command is not issuing one; sim-guard learned the same distinction
    about heredoc bodies the same day.
    """
    with open(os.path.join(ROOT, rel), encoding="utf-8") as f:
        lines = f.read().splitlines()
    return "\n".join(line for line in lines if not line.lstrip().startswith("#"))


def modules():
    """Modules declared in settings.gradle.kts, with their source sets."""
    found = {}
    for name in INCLUDE.findall(read(SETTINGS)):
        path = os.path.join(ANDROID, name.lstrip(":").replace(":", os.sep))
        found[name] = {
            "unit": os.path.isdir(os.path.join(path, "src", "test")),
            "instrumentation": os.path.isdir(os.path.join(path, "src", "androidTest")),
        }
    return found


def covers(gate_text, module, task):
    """Does this gate file invoke `task` for `module`?

    A bare task name covers every module; a qualified one covers only
    the module it names. Qualifying is what caused the hole this gate
    exists for, so the distinction is the whole point.
    """
    for run in GRADLEW.findall(gate_text):
        for token in run.split():
            if token.startswith("-"):
                continue
            if token == task:
                return True
            if token == f"{module}:{task}":
                return True
    return False


def main():
    failures = []
    found = modules()
    gates = {gate: read(gate) for gate in GATES}

    if len(found) < MIN_MODULES:
        failures.append(
            f"{SETTINGS}: found {len(found)} module(s), expected at least "
            f"{MIN_MODULES} — the enumeration is more likely broken than the "
            f"project. Check the include(\":x\") parsing before trusting a pass."
        )

    for module, sets in sorted(found.items()):
        if sets["unit"]:
            missing = [g for g in GATES if not covers(gates[g], module, UNIT_TASK)]
            if missing:
                failures.append(
                    f"{module} has src/test/ but {module}:{UNIT_TASK} is not run "
                    f"by: {', '.join(missing)}. Use the bare task name so every "
                    f"module is covered, or name this one explicitly."
                )

        if sets["instrumentation"]:
            missing = [
                g for g in GATES
                if not covers(gates[g], module, INSTRUMENTATION_TASK)
            ]
            if missing and module not in DEFERRED:
                failures.append(
                    f"{module} has src/androidTest/ that no gate runs, and no "
                    f"DEFERRED entry says when it will. Add it to DEFERRED with "
                    f"the checkpoint, or wire {module}:{INSTRUMENTATION_TASK}."
                )

    for gate in GATES:
        if "android-gate-scan" not in gates[gate]:
            failures.append(
                f"{gate} does not invoke android-gate-scan — a scan nothing "
                f"runs is the exact shape of the bug it checks for."
            )

    if failures:
        print("android-gate-scan: FAIL", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    deferred = ", ".join(f"{m}:{INSTRUMENTATION_TASK}" for m in sorted(DEFERRED))
    print(f"android-gate-scan: clean — {len(found)} modules; deferred: {deferred}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
