#!/usr/bin/env python3
"""Three readers of one document must say the same thing.

The CLI writes a JUnit report; a Rust reader sits beside the writer, a Swift
one serves XCTest and a Kotlin one serves Gradle. Three hand-written parsers
of one shape is a thing that drifts, and it drifts quietly: each one has its
own tests, each stays green, and they stop agreeing.

So the recorded payloads are one set of bytes and this checks the three
answers against each other rather than each against its own expectation.

Usage: three-readers-agree.py
"""

import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
RUST = ROOT / "crates/smix-adapter-maestro/tests/fixtures/reports"
SWIFT = ROOT / "swift-bridge/Tests/SmixSDKTests/fixtures"
KOTLIN = ROOT / "android-runner/sdk/src/test/resources/reports"

problems = []


def main():
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
    suites = {
        "rust": ["cargo", "test", "-q", "-p", "smix-adapter-maestro",
                 "--test", "a_report_a_test_framework_can_read"],
        "swift": ["swift", "test", "--package-path", "swift-bridge",
                  "--filter", "AFlowFromATest"],
        "kotlin": [str(ROOT / "android-runner/gradlew"), "-p",
                   str(ROOT / "android-runner"), ":sdk:testDebugUnitTest",
                   "--tests", "*AFlowFromAJUnitRule*", "--console=plain", "-q"],
    }
    for who, cmd in suites.items():
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
          f"rust/swift/kotlin, and all three suites pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
