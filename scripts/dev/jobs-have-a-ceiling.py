#!/usr/bin/env python3
"""Every CI job says how long it may take.

Without `timeout-minutes` a job runs until GitHub's default six hours.
A job that hangs — a device that never boots, a flow that waits on a
window that never appears, a fetch that stalls — then holds a runner
for an afternoon and blocks everything queued behind it, and the only
signal is that nothing has finished.

The ceiling is not a target. Each is set well above the measured
maximum, because a limit at the observed time turns a slow runner into
a red and teaches people to raise it rather than read it.

This reads the workflow structurally rather than with a YAML parser,
which is not in the CI image: a job is a two-space key under `jobs:`,
and its body is everything indented further. The floor below refuses a
run that finds almost nothing — a regex that stops matching reports
perfect compliance, which is the failure mode this whole family of
gates keeps finding in itself.

Usage:
  scripts/dev/jobs-have-a-ceiling.py [repo-root]
"""

import os
import re
import sys

ROOT = (
    sys.argv[1]
    if len(sys.argv) > 1
    else os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
)
WORKFLOWS = os.path.join(".github", "workflows")

JOBS_KEY = re.compile(r"^jobs:\s*$")
JOB = re.compile(r"^  ([A-Za-z][A-Za-z0-9_-]*):\s*$")
TIMEOUT = re.compile(r"^    timeout-minutes:\s*(\d+)\s*$")

# A job may not sit above this. Six hours is the default it would
# otherwise inherit; anything approaching that is the absence wearing a
# number.
MAX_MINUTES = 60
MIN_JOBS = 5


def jobs_in(text: str) -> list[tuple[str, list[str]]]:
    """Job names and their body lines, from the `jobs:` block onward."""
    lines = text.splitlines()
    try:
        start = next(i for i, l in enumerate(lines) if JOBS_KEY.match(l)) + 1
    except StopIteration:
        return []
    found: list[tuple[str, list[str]]] = []
    current: str | None = None
    body: list[str] = []
    for line in lines[start:]:
        m = JOB.match(line)
        if m:
            if current is not None:
                found.append((current, body))
            current, body = m.group(1), []
        elif current is not None:
            if line.strip() and not line.startswith("  "):
                break  # left the jobs block entirely
            body.append(line)
    if current is not None:
        found.append((current, body))
    return found


def main() -> int:
    wf_dir = os.path.join(ROOT, WORKFLOWS)
    if not os.path.isdir(wf_dir):
        print("jobs-have-a-ceiling: CANNOT RUN")
        print(f"  - {WORKFLOWS} is not in this tree")
        return 2

    problems: list[str] = []
    seen = 0

    for name in sorted(os.listdir(wf_dir)):
        if not name.endswith((".yml", ".yaml")):
            continue
        rel = os.path.join(WORKFLOWS, name)
        text = open(os.path.join(wf_dir, name), encoding="utf-8").read()
        for job, body in jobs_in(text):
            seen += 1
            found = [TIMEOUT.match(l) for l in body]
            minutes = next((int(m.group(1)) for m in found if m), None)
            if minutes is None:
                problems.append(
                    f"{rel}: job `{job}` has no timeout-minutes, so it may run for "
                    f"GitHub's default six hours. A job that hangs then holds a "
                    f"runner for an afternoon and the only signal is that nothing "
                    f"finished."
                )
            elif minutes > MAX_MINUTES:
                problems.append(
                    f"{rel}: job `{job}` allows {minutes} minutes, over the "
                    f"{MAX_MINUTES} this repository's longest job needs three times "
                    f"over. A ceiling near the default is the absence wearing a "
                    f"number."
                )

    if seen < MIN_JOBS:
        problems.append(
            f"only {seen} job(s) found across {WORKFLOWS}. Below {MIN_JOBS} this is "
            f"not reading the workflows, and every rule here is vacuously true on a "
            f"file it cannot parse."
        )

    if problems:
        print("jobs-have-a-ceiling: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    print(f"jobs-have-a-ceiling: clean — {seen} job(s), each with a ceiling under {MAX_MINUTES}m")
    return 0


if __name__ == "__main__":
    sys.exit(main())
