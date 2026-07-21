#!/usr/bin/env python3
"""Judge an instrumentation run by how much of the suite actually ran.

`failures="0"` is true of a suite that executed nothing. So is
`errors="0"`. A run that never reached the device, or reached it and
skipped everything, produces a result file that reads exactly like
success — and the Android instrumentation suites went from written to
run-for-the-first-time without any gate noticing, so "looks like it
passed" is not a hypothetical failure mode here.

The expected count comes from disk: every `@Test` under the module's
androidTest source set is counted at judging time. Writing the number
into the gate instead would mean a fifth assertion, added later, could
be skipped while the gate stayed green on four.

Four verdicts, each a different way a green can be hollow:
  * no result files at all — the task never ran, as opposed to running
    and finding nothing
  * fewer executed than exist on disk
  * failures or errors
  * anything skipped — @Ignore is not a pass, and adding one is the
    cheapest possible way to turn a red suite green

Usage:
  androidtest-xml-judge.py --module android-runner/sdk \\
      --results android-runner/sdk/build/outputs/androidTest-results/connected/debug
"""

import argparse
import glob
import os
import re
import sys
import xml.etree.ElementTree as ET

# `@Test` on its own line or preceding a fun on the same line. Kotlin
# annotations may carry arguments, hence the optional parenthesised part.
TEST_ANNOTATION = re.compile(r"@Test\b(?:\s*\([^)]*\))?")


def count_tests_on_disk(module_dir):
    """How many @Test functions the module's androidTest sources declare."""
    root = os.path.join(module_dir, "src", "androidTest")
    total = 0
    files = 0
    for path in glob.glob(os.path.join(root, "**", "*.kt"), recursive=True):
        with open(path, encoding="utf-8") as f:
            body = f.read()
        found = len(TEST_ANNOTATION.findall(body))
        if found:
            files += 1
            total += found
    return total, files


def sum_result_xml(results_dir):
    """Totals across every TEST-*.xml in the directory."""
    paths = sorted(glob.glob(os.path.join(results_dir, "TEST-*.xml")))
    totals = {"tests": 0, "failures": 0, "errors": 0, "skipped": 0}
    for path in paths:
        try:
            suite = ET.parse(path).getroot()
        except ET.ParseError as e:
            print(f"androidtest-xml-judge: {path} is not parseable XML: {e}", file=sys.stderr)
            return paths, None
        for key in totals:
            totals[key] += int(suite.get(key, "0"))
    return paths, totals


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--module", required=True, help="module dir, e.g. android-runner/sdk")
    ap.add_argument("--results", required=True, help="dir holding TEST-*.xml")
    args = ap.parse_args()

    expected, source_files = count_tests_on_disk(args.module)
    if expected == 0:
        print(
            f"androidtest-xml-judge: no @Test found under {args.module}/src/androidTest — "
            f"either the path is wrong or the suite is empty; both make a pass meaningless",
            file=sys.stderr,
        )
        return 1

    paths, totals = sum_result_xml(args.results)
    if not paths:
        print(
            f"androidtest-xml-judge: no TEST-*.xml in {args.results} — the task did not run. "
            f"That is different from running and finding nothing, and only this check "
            f"tells them apart.",
            file=sys.stderr,
        )
        return 1
    if totals is None:
        return 1

    problems = []
    if totals["tests"] != expected:
        problems.append(
            f"{expected} @Test on disk ({source_files} file(s)) but {totals['tests']} executed — "
            f"the run covered less than the suite"
        )
    if totals["failures"] or totals["errors"]:
        problems.append(f"{totals['failures']} failure(s), {totals['errors']} error(s)")
    if totals["skipped"]:
        problems.append(
            f"{totals['skipped']} skipped — a skip is not a pass, and @Ignore is the "
            f"cheapest way to turn a red suite green"
        )

    if problems:
        print("androidtest-xml-judge: FAIL", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1

    print(
        f"androidtest-xml-judge: {totals['tests']}/{expected} executed, "
        f"0 failures, 0 errors, 0 skipped"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
