#!/usr/bin/env python3
"""Which device e2e scripts can run beside which.

The device tier used to run every `scripts/dev/*-e2e.sh` one after the
other, although most of them drive one platform only: an iOS script and
an Android script share no device, so running them in turn spent the sum
of two lanes where the longer of the two would do.

A script's lane is read from its code (comments dropped), never listed:

- `serial`  — it drives both platforms, or it acts on the whole machine
              (`smix down`, a sweep). Nothing may run beside it.
- `android` — it drives the Android emulator the tier shares.
- `ios`     — it drives the simulator the tier shares.
- `none`    — it drives no device at all, so it runs beside both.

Every script lands in exactly one lane, and the total is the glob's.

Usage:
  scripts/dev/e2e-lanes.py              # "<lane>\\t<path>" per script
  scripts/dev/e2e-lanes.py --selftest
"""

import glob
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

ANDROID = re.compile(
    r"E2E_ANDROID|SMIX_ANDROID_SERIAL|\badb\b|pick-dev-emulator"
    r"|--platform android|emulator-|_ANDROID\b"
)
IOS = re.compile(
    r"SMIX_E2E_UDID|E2E_IOS|xcrun simctl|pick-dev-sim|--platform ios"
    r"|xcodebuild|SMIX_E2E_IOS"
)
# `smix down` and the sweeps act on every device this machine has, so a
# script that runs one would stop the other lane's runner mid-script.
MACHINE = re.compile(r'\$SMIX"?\s+down\b|\bsmix down\b|smix-sweep|simx-sweep')


def code_of(src: str) -> str:
    return "\n".join(ln for ln in src.splitlines() if not ln.lstrip().startswith("#"))


def lane_of(src: str) -> str:
    code = code_of(src)
    if MACHINE.search(code):
        return "serial"
    a, i = bool(ANDROID.search(code)), bool(IOS.search(code))
    if a and i:
        return "serial"
    if a:
        return "android"
    return "ios" if i else "none"


def lanes(root: str = ROOT):
    scripts = sorted(glob.glob(os.path.join(root, "scripts", "dev", "*-e2e.sh")))
    return [(lane_of(open(p, encoding="utf-8").read()), p) for p in scripts]


def selftest() -> int:
    cases = [
        ("an iOS script", 'SMIX_E2E_UDID="$1"\n"$SMIX" run x --device "$SMIX_E2E_UDID"', "ios"),
        ("no device at all", '"$SMIX" run --check flow.yaml', "none"),
        ("an Android script", 'adb -s "$SERIAL" shell true', "android"),
        ("both platforms", 'adb -s e shell true\nxcrun simctl boot "$UDID"', "serial"),
        ("the whole machine", '"$SMIX" down', "serial"),
        ("a sweep on the iOS side", 'bash scripts/dev/simx-sweep.sh\nSMIX_E2E_UDID=1', "serial"),
        ("adb only in a comment", '# adb would be wrong here\nSMIX_E2E_UDID=1', "ios"),
        ("a device word only in a comment", "# xcrun simctl boot is not ours to run\ntrue", "none"),
        ("smix down only in a comment", "# never smix down here\nadb -s e shell true", "android"),
    ]
    fails = 0
    for label, src, want in cases:
        got = lane_of(src)
        if got != want:
            print(f"  FAIL {label}: {got}, wanted {want}")
            fails += 1
        else:
            print(f"  ok   {label}")
    real = lanes()
    n_glob = len(glob.glob(os.path.join(ROOT, "scripts", "dev", "*-e2e.sh")))
    by = {}
    for lane, _ in real:
        by[lane] = by.get(lane, 0) + 1
    if len(real) != n_glob or n_glob == 0:
        print(f"  FAIL the tree: {len(real)} scripts placed, {n_glob} on disk")
        fails += 1
    elif not all(by.get(k) for k in ("ios", "android", "serial")):
        print(f"  FAIL the tree: a lane is empty ({by}) — the reading stopped matching")
        fails += 1
    else:
        print(f"  ok   the tree: {n_glob} scripts, {by}")
    print("e2e-lanes selftest:", "PASS" if not fails else f"FAIL ({fails})")
    return 1 if fails else 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--selftest"]:
        sys.exit(selftest())
    placed = lanes()
    if not placed:
        # The tier runs what this lists; an empty list would be a tier that
        # ran nothing and could not tell.
        print("e2e-lanes: no scripts/dev/*-e2e.sh found — nothing to place", file=sys.stderr)
        sys.exit(1)
    for lane, path in placed:
        print(f"{lane}\t{path}")
