#!/usr/bin/env python3
"""A release gate that runs flows on a device keeps what the device
recorded when the app under test went away.

CI's fixture left mid-flow twice in one corpus run (K2, 2026-09-25). The
rerun was green, and the red one had kept the flow's log and nothing
else: no crash report, no system log. Whether the app crashed, was
killed, or was never started could not be told, and the next red will be
the same unless every gate collects.

The set is derived: every `scripts/release/*.sh` that runs
`"$SMIX_BIN" run … --device <a variable>` (a real device, not the
`--device dry` parse). Each must source `scripts/lib/crash-evidence.sh`
and call `keep_crash_evidence_if_the_app_left`. The set is also checked
from the other side — the two gates this was written for must be in it —
so a gate that stops being found does not make the check pass by
shrinking.

Usage:
  scripts/dev/a-device-gate-keeps-crash-evidence.py [--root DIR]
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

RUNS_ON_A_DEVICE = re.compile(r'"\$SMIX_BIN" run\b[^\n]*--device "\$')
SOURCES = re.compile(r'^\s*(?:\.|source)\s+"?\$[A-Z_]*ROOT[A-Z_]*"?/scripts/lib/crash-evidence\.sh', re.M)
CALLS = re.compile(r'^\s*keep_crash_evidence_if_the_app_left\b', re.M)
# The gates this was written for (K2). Named, so the derived set cannot
# lose one of them and stay green.
MUST_BE_FOUND = {"corpus-gate.sh", "stress-gate.sh"}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=str(pathlib.Path(__file__).resolve().parents[2]))
    root = pathlib.Path(ap.parse_args().root)
    release = root / "scripts" / "release"
    found, missing = [], []
    for path in sorted(release.glob("*.sh")):
        text = path.read_text(encoding="utf-8")
        if not RUNS_ON_A_DEVICE.search(text):
            continue
        found.append(path.name)
        lacks = [
            what
            for what, pattern in (("does not source crash-evidence.sh", SOURCES), ("never calls keep_crash_evidence_if_the_app_left", CALLS))
            if not pattern.search(text)
        ]
        if lacks:
            missing.append(f"{path.relative_to(root)}: runs flows on a device and {' and '.join(lacks)}")
    lost = sorted(MUST_BE_FOUND - set(found))
    if lost:
        missing.append(
            "not found as gates that run flows on a device: "
            + ", ".join(lost)
            + " — the pattern stopped matching them, so this check would pass by looking at less"
        )
    if missing:
        print("device-gate-crash-evidence: FAIL")
        for m in missing:
            print(f"  - {m}")
        return 1
    print(f"device-gate-crash-evidence: clean — {len(found)} gate(s) run flows on a device and keep the evidence: {', '.join(found)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
