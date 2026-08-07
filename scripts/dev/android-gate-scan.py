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

import glob
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

COMPILE_TASK = "assembleDebugAndroidTest"

# Where the CLI names the test it launches on the device. That string has
# to match a @Test that exists on disk; when it stops matching, the
# instrumentation reports `OK (0 tests)` — a silent no-op that reads like
# success. Nothing checked the pair until this gate.
SERVER_ENTRY_SOURCE = "crates/smix-capsule/src/runner_android.rs"

# ship.sh does not run the device suite inline; it calls this. So ship's
# coverage is read as ship.sh plus the delegates it names by path. A
# delegate nobody calls is caught separately.
SHIP_DELEGATES = {
    "scripts/release/android-instrumentation-gate.sh": "device selection and the "
    "deadline do not belong in ship.sh's body, and an adb call inside a script is "
    "invisible to the PreToolUse guard — so the delegate carries the same "
    "emulator-only rule itself",
}

# A DEFERRED table stood here while the previous two checkpoints landed:
# an androidTest source set no gate ran yet, with a note on where its
# fate would be decided. Both entries are settled, so the table is gone
# rather than left empty — an empty one would suggest that "add it to
# DEFERRED" is a legitimate way to dispose of a new source set. Kind is
# derived from disk below instead.

# Task names that need no separate demand, and why. Stated rather than
# silently skipped: a reader must be able to tell "covered elsewhere"
# from "never considered".
#
# NOTHING READS THIS. It documents a judgement rather than enforcing it —
# the checks below demand specific tasks instead of ruling others out, so
# no code path consults this table. It is left because the reasoning is
# worth keeping, and labelled because a constant that looks like a
# criterion while taking no part in any decision is the exact shape this
# repository keeps getting caught by.
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


TEST_FN = re.compile(r"@Test\b(?:\s*\([^)]*\))?\s*(?:@\w+\s*)*fun\s+(\w+)", re.S)
PACKAGE = re.compile(r"^\s*package\s+([\w.]+)", re.M)
CLASS = re.compile(r"^\s*(?:@\w+(?:\([^)]*\))?\s*)*class\s+(\w+)", re.M)


def module_dir(name):
    return os.path.join(ANDROID, name.lstrip(":").replace(":", os.sep))


def test_names(module_name):
    """Fully-qualified `pkg.Class#fun` for every @Test in androidTest.

    Read from disk so the classification below is evidence rather than a
    constant someone has to remember to update.
    """
    root = os.path.join(module_dir(module_name), "src", "androidTest")
    names = []
    for path in glob.glob(os.path.join(root, "**", "*.kt"), recursive=True):
        with open(path, encoding="utf-8") as f:
            body = f.read()
        package = PACKAGE.search(body)
        klass = CLASS.search(body)
        if not package or not klass:
            continue
        for fn in TEST_FN.findall(body):
            names.append(f"{package.group(1)}.{klass.group(1)}#{fn}")
    return names


def server_entry():
    """The test name the CLI launches on the device."""
    with open(os.path.join(ROOT, SERVER_ENTRY_SOURCE), encoding="utf-8") as f:
        body = f.read()
    match = re.search(r'SERVER_ENTRY:\s*&str\s*=\s*"([^"]+)"', body)
    if not match:
        return None
    return match.group(1)


def modules():
    """Modules declared in settings.gradle.kts, with their source sets."""
    found = {}
    for name in INCLUDE.findall(read(SETTINGS)):
        path = module_dir(name)
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

        # Compilation is demanded of every androidTest source set,
        # everywhere. It is the counterpart of the iOS build-for-testing
        # step: compile the body that ships, without starting a device.
        if sets["instrumentation"]:
            missing = [g for g in GATES if not covers(gates[g], module, COMPILE_TASK)]
            if missing:
                failures.append(
                    f"{module} has src/androidTest/ that {', '.join(missing)} never "
                    f"compiles. Add the bare {COMPILE_TASK} there — :app's source "
                    f"set is the runner body users receive, and it went uncompiled "
                    f"by any gate until this check existed."
                )

    # --- kind, derived from disk --------------------------------------
    entry = server_entry()
    if entry is None:
        failures.append(
            f"{SERVER_ENTRY_SOURCE}: no SERVER_ENTRY constant found — this scan "
            f"cannot tell the runner body from an assertion suite without it."
        )
    else:
        by_module = {m: test_names(m) for m, s in found.items() if s["instrumentation"]}
        hosts = [m for m, names in by_module.items() if entry in names]
        if len(hosts) != 1:
            failures.append(
                f"SERVER_ENTRY is {entry!r} but that names a @Test in {len(hosts)} "
                f"module(s) {hosts}. It must match exactly one on disk: when it "
                f"matches none, the device launch reports 'OK (0 tests)' and reads "
                f"like a pass. Checked in {SERVER_ENTRY_SOURCE}."
            )
        else:
            runner_body = hosts[0]
            if len(by_module[runner_body]) != 1:
                failures.append(
                    f"{runner_body} is the runner body (it holds SERVER_ENTRY) but "
                    f"declares {len(by_module[runner_body])} @Test: "
                    f"{by_module[runner_body]}. It must hold exactly that one. An "
                    f"assertion parked here invites someone to demand a connected "
                    f"task of this module — which does not fail, it never returns."
                )
            for module, names in sorted(by_module.items()):
                if module == runner_body:
                    continue
                if not names:
                    failures.append(
                        f"{module} has src/androidTest/ with no @Test in it — an "
                        f"empty source set is unfinished work, not a kind."
                    )
                    continue
                ship_text = gates["scripts/release/ship.sh"] + "\n".join(
                    read(d) for d in SHIP_DELEGATES if os.path.isfile(os.path.join(ROOT, d))
                )
                if not covers(ship_text, module, INSTRUMENTATION_TASK):
                    failures.append(
                        f"{module} holds {len(names)} assertion(s) that the release "
                        f"path never runs on a device. Wire "
                        f"{module}:{INSTRUMENTATION_TASK} into scripts/release/ship.sh "
                        f"or a delegate it names."
                    )
        for delegate in SHIP_DELEGATES:
            if delegate not in gates["scripts/release/ship.sh"]:
                failures.append(
                    f"{delegate} is treated as part of ship's coverage but ship.sh "
                    f"does not call it — the coverage would be imaginary."
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

    # The downgrade is named on every run. CI compiles and does not
    # execute; if that only lived in a plan document, a green CI would
    # keep reading as "Android behaviour is covered".
    assertions = sorted(
        m
        for m, s in found.items()
        if s["instrumentation"] and entry not in test_names(m)
    )
    detail = "; ".join(f"{m} ({len(test_names(m))} tests)" for m in assertions)
    body = next(
        (m for m, s in found.items() if s["instrumentation"] and entry in test_names(m)),
        "?",
    )
    print(
        f"android-gate-scan: clean — {len(found)} modules; "
        f"androidTest compile: preflight+CI+ship; "
        f"instrumentation: ship only — {detail} on a pinned emulator; "
        f"CI has no emulator; "
        f"{body} is the runner body ({entry}), never a connected task"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
