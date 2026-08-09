#!/usr/bin/env python3
"""What `flake-classify.py` must answer, fed records rather than a device.

The classifier decides whether a flow passed, needed a retry, or failed.
Nothing here touches a simulator: the question is arithmetic over the
attempt records `smix run` writes, and a test that needed a device would
run nowhere.

Two of these cases are the ones that would quietly go wrong:

- The attempt file is capped at 32 flows, so a corpus of 21 run twice
  leaves two entries for the same name. Reading the first is reading the
  previous batch.
- A flow with no record at all must not read as a pass. Absence of
  evidence has been mistaken for evidence three times in this cycle
  already, and every time it read as coverage that was not there.
"""

import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CLASSIFY = os.path.join(ROOT, "scripts", "dev", "flake-classify.py")

problems: list[str] = []


def attempt(index: int, ok: bool) -> dict:
    return {
        "attempt_index": index,
        "status": "ok" if ok else "error",
        "error_class": None if ok else "EXIT_3",
        "ips_generated": None,
        "wall_ms": 1200,
    }


def classify(records: object, flow: str) -> str:
    """Run the classifier against a temporary attempt file."""
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as fh:
        if records is not None:
            json.dump(records, fh)
        path = fh.name
    try:
        if records is None:
            os.unlink(path)
        out = subprocess.run(
            [sys.executable, CLASSIFY, flow, "--attempts", path],
            capture_output=True,
            text=True,
            check=False,
        )
        return out.stdout.strip() or f"<no output, exit {out.returncode}: {out.stderr.strip()}>"
    finally:
        if os.path.exists(path):
            os.unlink(path)


def expect(label: str, got: str, want: str) -> None:
    if got != want:
        problems.append(f"{label}: expected {want}, got {got}")


expect(
    "first attempt succeeded",
    classify([{"flow_name": "nav", "attempts": [attempt(0, True)]}], "nav"),
    "PASS",
)

expect(
    "failed then succeeded",
    classify(
        [{"flow_name": "nav", "attempts": [attempt(0, False), attempt(1, True)]}],
        "nav",
    ),
    "FLAKE",
)

expect(
    "every attempt failed",
    classify(
        [{"flow_name": "nav", "attempts": [attempt(0, False), attempt(1, False)]}],
        "nav",
    ),
    "FAIL",
)

# The cap makes this the normal case, not an edge one: run the corpus
# twice and every flow has two entries. Taking the first is reading the
# previous batch's answer and calling it this one's.
expect(
    "the same flow twice — the last entry wins",
    classify(
        [
            {"flow_name": "nav", "attempts": [attempt(0, True)]},
            {"flow_name": "nav", "attempts": [attempt(0, False), attempt(1, False)]},
        ],
        "nav",
    ),
    "FAIL",
)

expect(
    "another flow's record is not this flow's",
    classify([{"flow_name": "other", "attempts": [attempt(0, True)]}], "nav"),
    "NORECORD",
)

# The shape the real source emits. The hand-written snake_case fixtures
# above passed for a whole implementation that could not read a single
# real record: they matched what the author believed the source was, and
# the source was `smix diagnostic dump --json`'s camelCase, from a store
# the old JSON file had stopped mirroring in July.
expect(
    "the wire's camelCase, which is what smix actually emits",
    classify(
        [
            {
                "flowName": "nav",
                "attempts": [
                    {"attemptIndex": 0, "status": "error", "errorClass": "EXIT_3"},
                    {"attemptIndex": 1, "status": "ok", "errorClass": None},
                ],
            }
        ],
        "nav",
    ),
    "FLAKE",
)

expect(
    "no attempt file at all",
    classify(None, "nav"),
    "NORECORD",
)

expect(
    "an empty attempt file",
    classify([], "nav"),
    "NORECORD",
)

if problems:
    print("flake-classify.test: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print("flake-classify.test: 8 cases pass")
