#!/usr/bin/env python3
"""Three readers of one document must say the same thing.

The CLI writes a JUnit report; a Rust reader sits beside the writer, a Swift
one serves XCTest and a Kotlin one serves Gradle. Three hand-written parsers
of one shape is a thing that drifts, and it drifts quietly: each one has its
own tests, each stays green, and they stop agreeing.

So the recorded payloads are one set of bytes and this checks the three
answers against each other rather than each against its own expectation.

The three suites need three toolchains and no one machine in CI has all
three: the Swift package carries an .xcframework, which SwiftPM on Linux
refuses outright. So the caller names which suites it can run, and
--assert-ci-union reads the workflow back and requires the named sets to
cover all three. A suite nobody runs would otherwise look exactly like a
suite that passes.

Usage: three-readers-agree.py [--suites rust,swift,kotlin]
       three-readers-agree.py --assert-ci-union
"""

import argparse
import pathlib
import re
import subprocess
import sys

ALL_SUITES = ("rust", "swift", "kotlin")

ROOT = pathlib.Path(__file__).resolve().parents[2]
RUST = ROOT / "crates/smix-adapter-maestro/tests/fixtures/reports"
SWIFT = ROOT / "swift-bridge/Tests/SmixSDKTests/fixtures"
KOTLIN = ROOT / "android-runner/sdk/src/test/resources/reports"

problems = []


def assert_ci_union():
    """Every reader must be exercised somewhere in CI.

    The union of the --suites named across the workflow has to be all three.
    A reader that fell out of every invocation is a reader whose copy can rot
    without a word -- and it reads, from the outside, exactly like one that
    passes.
    """
    wf = ROOT / ".github/workflows/ci.yml"
    text = wf.read_text()
    calls = re.findall(r"three-readers-agree\.py([^\n]*)", text)
    if not calls:
        print("three-readers-agree: FAIL")
        print("  - the workflow never runs this gate, so none of the three "
              "readers is exercised in CI")
        return 1
    covered = set()
    # Not this very check: it names no suite, and counting it as all three
    # made the assertion true no matter what the other jobs ran.
    calls = [c for c in calls if "--assert-ci-union" not in c]
    if not calls:
        print("three-readers-agree: FAIL")
        print("  - the workflow asserts the union and never runs a suite, so "
              "the assertion is about nothing")
        return 1
    for tail in calls:
        m = re.search(r"--suites[= ]([a-z,]+)", tail)
        named = m.group(1).split(",") if m else list(ALL_SUITES)
        unknown = [n for n in named if n not in ALL_SUITES]
        if unknown:
            print("three-readers-agree: FAIL")
            print(f"  - the workflow names a suite that does not exist: "
                  f"{', '.join(unknown)}")
            return 1
        covered |= set(named)
    missing = [s for s in ALL_SUITES if s not in covered]
    if missing:
        print("three-readers-agree: FAIL")
        print(f"  - no CI job runs the {', '.join(missing)} reader's suite; "
              f"it would rot silently while this gate stayed green")
        return 1
    print(f"three-readers-agree: CI covers all three suites "
          f"({len(calls)} invocations)")
    return 0


def main(suites):
    names = sorted(p.name for p in RUST.glob("*.xml"))
    if not names:
        print("three-readers-agree: FAIL")
        print("  - no recorded payloads under the Rust fixtures — there is "
              "nothing for the three readers to disagree about")
        return 1

    # The bytes themselves, before anybody parses them. Three copies of one
    # document is the thing that goes stale; the parsers agreeing about
    # different bytes would prove nothing.
    for name in names:
        a = (RUST / name).read_bytes()
        for where, d in (("swift", SWIFT), ("kotlin", KOTLIN)):
            f = d / name
            if not f.exists():
                problems.append(
                    f"{name} is recorded for Rust and missing from {where} — "
                    f"that reader is being tested against a document the "
                    f"others no longer share"
                )
            elif f.read_bytes() != a:
                problems.append(
                    f"{name} differs between Rust and {where}: the three "
                    f"readers are agreeing about different bytes"
                )

    # And every reader has to actually be exercised. A copy nobody parses
    # is a copy that can rot without a word.
    runnable = {
        "rust": ["cargo", "test", "-q", "-p", "smix-adapter-maestro",
                 "--test", "a_report_a_test_framework_can_read"],
        "swift": ["swift", "test", "--package-path", "swift-bridge",
                  "--filter", "AFlowFromATest"],
        "kotlin": [str(ROOT / "android-runner/gradlew"), "-p",
                   str(ROOT / "android-runner"), ":sdk:testDebugUnitTest",
                   "--tests", "*AFlowFromAJUnitRule*", "--console=plain", "-q"],
    }
    for who in suites:
        cmd = runnable[who]
        r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
        if r.returncode != 0:
            problems.append(
                f"the {who} reader's suite did not pass — it is one of the "
                f"three and cannot be excused: "
                f"{(r.stdout + r.stderr).strip().splitlines()[-1][:120] if (r.stdout + r.stderr).strip() else 'no output'}"
            )

    if problems:
        print("three-readers-agree: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(f"three-readers-agree: {len(names)} payloads, byte-identical across "
          f"rust/swift/kotlin; suites run here: {','.join(suites)}")
    return 0


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--suites", default=",".join(ALL_SUITES))
    ap.add_argument("--assert-ci-union", action="store_true")
    a = ap.parse_args()
    if a.assert_ci_union:
        sys.exit(assert_ci_union())
    picked = [s for s in a.suites.split(",") if s]
    bad = [s for s in picked if s not in ALL_SUITES]
    if bad or not picked:
        sys.exit(f"--suites takes any of {','.join(ALL_SUITES)}, got: {a.suites}")
    sys.exit(main(picked))
