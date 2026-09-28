#!/usr/bin/env python3
"""A release gate that drives the Android runner keeps what the runner
logged when it goes red.

The runner writes one line per request to the device log: the route, its
answer, how long it took and, for a route with a time limit, where the
time went. A gate that failed on "the runner did not answer in time" kept
only the host's side, and by the time anyone looked the device log had
moved on, so where the time went could not be read.

The set is derived: every `scripts/release/*.sh` that brings a runner up
or drives a device with `--platform android`, or takes the shared
emulator (`E2E_ANDROID`). Each must source
`scripts/lib/android-runner-log.sh`, read the device's clock when it
starts (`android_device_now`) and collect on a red
(`collect_android_runner_log`). The set is checked from the other side
too — the gates this was written for must be in it — so a gate that stops
being found does not make the check pass by shrinking.

Usage:
  scripts/dev/a-device-gate-keeps-the-runner-log.py [--root DIR]
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

DRIVES_ANDROID = re.compile(r"--platform android\b|\bE2E_ANDROID\b")
SOURCES = re.compile(r'^\s*(?:\.|source)\s+"?\$[A-Z_]*ROOT[A-Z_]*"?/scripts/lib/android-runner-log\.sh', re.M)
READS_THE_CLOCK = re.compile(r"\bandroid_device_now\b")
COLLECTS = re.compile(r"\bcollect_android_runner_log\b")
MUST_BE_FOUND = {"android-behaviour-gate.sh", "device-e2e-tier.sh", "ship.sh"}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=str(pathlib.Path(__file__).resolve().parents[2]))
    root = pathlib.Path(ap.parse_args().root)
    found, missing = [], []
    for path in sorted((root / "scripts" / "release").glob("*.sh")):
        text = path.read_text(encoding="utf-8")
        if not DRIVES_ANDROID.search(text):
            continue
        found.append(path.name)
        lacks = [
            what
            for what, pattern in (
                ("does not source android-runner-log.sh", SOURCES),
                ("never reads the device's clock (android_device_now)", READS_THE_CLOCK),
                ("never collects the runner's log (collect_android_runner_log)", COLLECTS),
            )
            if not pattern.search(text)
        ]
        if lacks:
            missing.append(f"{path.relative_to(root)}: drives the Android runner and {' and '.join(lacks)}")
    lost = sorted(MUST_BE_FOUND - set(found))
    if lost:
        missing.append(
            "not found as gates that drive the Android runner: "
            + ", ".join(lost)
            + " — the pattern stopped matching them, so this check would pass by looking at less"
        )
    if missing:
        print("device-gate-runner-log: FAIL")
        for m in missing:
            print(f"  - {m}")
        return 1
    print(f"device-gate-runner-log: clean — {len(found)} gate(s) drive the Android runner and keep its log: {', '.join(found)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
