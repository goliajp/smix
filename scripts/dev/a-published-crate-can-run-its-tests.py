#!/usr/bin/env python3
"""Which published crates can run their own tests, and which cannot.

`cargo package` verifies by BUILDING the library. It never builds the
tests, so a crate whose tests read files that are not in the package
publishes cleanly and fails the first time somebody downloads it and
types `cargo test`. Measured on 2026-08-21: the published
smix-runner-wire does not compile its test suite, because seven of its
tests `include_str!` Swift sources three directories above the crate
root.

Those tests are RIGHT to do that. Pinning a Rust assertion directly to
the Swift file it describes is how this repository keeps the two in
step, and moving the file into the crate would be a copy that goes
stale. The defect is not the reach; it is that nothing said out loud
which packages cannot be tested where they land.

So this does not forbid it. It counts it, per crate, and refuses only a
crate whose reach is UNDECLARED — every one of them is either listed
here with the reason, or it is a surprise waiting for a stranger.

Usage:
  scripts/dev/a-published-crate-can-run-its-tests.py [repo-root]
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys

ROOT = (
    sys.argv[1]
    if len(sys.argv) > 1
    else os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
)

# A test reaching above its crate root. Both notations this repository
# uses: the compile-time include and the runtime path join.
REACHES = re.compile(
    r'include_str!\("\.\./\.\./\.\.|include_bytes!\("\.\./\.\./\.\.'
    r'|CARGO_MANIFEST_DIR"\)\)\s*\.join\("\.\."'
)

# Crates whose test suites are known to reach outside the package, with
# the reason. Being on this list is not permission to be careless — it
# is a statement that somebody looked and decided the reach is worth
# more than a testable package.
REACHES_ON_PURPOSE = {
    "smix-adapter-maestro": "pins verb-table assertions to the corpus and the guides",
    "smix-cli": "pins CLI help assertions to the guides that document them",
    "smix-error": "pins failure-shape assertions to the Swift and Kotlin runners",
    "smix-mcp": "pins tool-surface assertions to the plugin manifest",
    "smix-runner-wire": "pins route-shape assertions to the Swift route sources",
    "smix-sdk": "pins one assertion to the shared corpus",
    "smix-store": "pins ledger assertions to the runner's own writes",
}

MIN_CRATES = 10


def publishable(root: str) -> list[str]:
    out = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=root,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    meta = json.loads(out)
    return sorted(p["name"] for p in meta["packages"] if p.get("publish") != [])


def reaches(root: str, crate: str) -> int:
    tests = os.path.join(root, "crates", crate, "tests")
    if not os.path.isdir(tests):
        return 0
    total = 0
    for dirpath, _, names in os.walk(tests):
        for name in names:
            if not name.endswith(".rs"):
                continue
            try:
                body = open(os.path.join(dirpath, name), encoding="utf-8").read()
            except (OSError, UnicodeDecodeError):
                continue
            total += len(REACHES.findall(body))
    return total


def main() -> int:
    crates = publishable(ROOT)
    if len(crates) < MIN_CRATES:
        print("a-published-crate-can-run-its-tests: FAIL")
        print(
            f"  - cargo metadata reports {len(crates)} publishable crate(s), fewer "
            f"than {MIN_CRATES}. This is not the smix workspace, and every count "
            f"below would be zero for the wrong reason."
        )
        return 1

    problems: list[str] = []
    declared_and_reaching = 0

    for crate in crates:
        n = reaches(ROOT, crate)
        if n and crate not in REACHES_ON_PURPOSE:
            problems.append(
                f"{crate}: {n} test reference(s) read files above the crate root, "
                f"and it is not on the declared list. The package publishes and "
                f"its test suite does not compile where it lands — nothing else "
                f"says so, because cargo package verifies the library and never "
                f"builds the tests."
            )
        elif n:
            declared_and_reaching += 1

    for crate, reason in sorted(REACHES_ON_PURPOSE.items()):
        if crate not in crates:
            problems.append(
                f"{crate} is declared as reaching outside its package (\"{reason}\") "
                f"and is not a publishable crate here. An entry that outlived its "
                f"subject reads as a considered decision."
            )
        elif not reaches(ROOT, crate):
            problems.append(
                f"{crate} is declared as reaching outside its package (\"{reason}\") "
                f"and no longer does. The declaration should go with the reach — "
                f"otherwise the next crate to start reaching inherits an excuse "
                f"nobody wrote for it."
            )

    if problems:
        print("a-published-crate-can-run-its-tests: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    print(
        f"a-published-crate-can-run-its-tests: clean — {len(crates)} publishable "
        f"crates, {declared_and_reaching} whose tests deliberately read files the "
        f"package does not carry, each declared with why"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
