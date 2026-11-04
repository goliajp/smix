#!/usr/bin/env python3
"""An e2e that boots a device beyond the release's own two waits for room.

The device tier runs its lanes side by side, and smix's device budget refuses
a boot the machine has no room for. A script in one lane that boots a second
or third device while a script in another lane holds one was refused outright
(2026-11-05: v11.0-c9k, v11.0-c4, v6.1-c1, each red for that and nothing it
judges). With `--wait` the boot queues until the other lane gives the room
back, which changes nothing a script judges about where or how it boots.

Rule: in a script that names an extra device (a SECOND / THIRD / DESKTOP /
TABLET alias, or a second simulator), every `sim boot` it runs carries
`--wait`. The scripts listed in EXEMPT boot without it on purpose and say why.

Exit 0 clean, 1 a boot without --wait, 2 nothing found to check.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
EXTRA = re.compile(r"E2E_(ANDROID|IOS)_(SECOND|THIRD)|E2E_ANDROID_(DESKTOP|TABLET)|SECOND_SIM")
BOOT = re.compile(r'sim boot\s+"\$')

EXEMPT = {
    # its subject is a boot whose caller is ended part-way; queueing would
    # move the signal from the boot to the wait
    "v11.0-c9k-an-emulator-quits-cleanly-e2e.sh": "the boot is interrupted on purpose",
}


def offenders(text: str) -> list[tuple[int, str]]:
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        if stripped.startswith("#") or not BOOT.search(line):
            continue
        if "--wait" not in line:
            out.append((n, stripped))
    return out


def main() -> int:
    scripts = sorted((ROOT / "scripts/dev").glob("*-e2e.sh"))
    extra = [p for p in scripts if EXTRA.search(p.read_text(encoding="utf-8"))]
    if not extra:
        print("an-extra-device-waits-for-room: no e2e names an extra device — the pattern no longer matches")
        return 2
    bad = []
    checked = 0
    for p in extra:
        if p.name in EXEMPT:
            continue
        found = offenders(p.read_text(encoding="utf-8"))
        checked += sum(1 for line in p.read_text(encoding="utf-8").splitlines() if BOOT.search(line) and not line.strip().startswith("#"))
        bad += [(p.name, n, line) for n, line in found]
    for name in EXEMPT:
        if not (ROOT / "scripts/dev" / name).exists():
            bad.append((name, 0, "exempt, but the script is gone: drop it from EXEMPT"))
    if bad:
        print("an-extra-device-waits-for-room: FAIL")
        for name, n, line in bad:
            print(f"  - {name}:{n}: {line}")
        return 1
    print(f"an-extra-device-waits-for-room: clean — {len(extra)} scripts name an extra device, {checked} boots all wait, {len(EXEMPT)} exempt")
    return 0


if __name__ == "__main__":
    sys.exit(main())
