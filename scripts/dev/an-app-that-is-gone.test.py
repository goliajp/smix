#!/usr/bin/env python3
"""an-app-that-is-gone.py must go red on the three shapes it exists for.

Three trees built in a temp dir, judged by the real gate: one unqualified
mention (red, and the line is named), every mention qualified (green),
and no mention at all (red — an empty set is not a clean one).
"""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "an-app-that-is-gone.py")


def build(root: str, doc_line: str, guard_body: str) -> None:
    os.makedirs(os.path.join(root, "docs"))
    os.makedirs(os.path.join(root, "plugin"))
    os.makedirs(os.path.join(root, "crates", "smix-cli", "src"))
    with open(os.path.join(root, "README.md"), "w") as fh:
        fh.write("# fixture\n")
    with open(os.path.join(root, "docs", "guide.md"), "w") as fh:
        fh.write(doc_line + "\n")
    with open(os.path.join(root, "crates", "smix-cli", "src", "capsule.rs"), "w") as fh:
        fh.write(guard_body + "\n")


def run(root: str) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, GATE, root], capture_output=True, text=True)


def main() -> int:
    fail = 0
    guard_ok = "// Simulator.app on Xcode <= 26 pops a window; DeviceHub does not"

    with tempfile.TemporaryDirectory() as tmp:
        build(tmp, "close the Simulator.app window first", guard_ok)
        r = run(tmp)
        if r.returncode != 1 or "docs/guide.md:1" not in r.stdout:
            print(f"an unqualified mention should be red and named: exit {r.returncode}\n{r.stdout}")
            fail = 1

    with tempfile.TemporaryDirectory() as tmp:
        build(tmp, "close the Simulator.app window first (Xcode <= 26)", guard_ok)
        r = run(tmp)
        if r.returncode != 0:
            print(f"a qualified mention should be green: exit {r.returncode}\n{r.stdout}")
            fail = 1

    with tempfile.TemporaryDirectory() as tmp:
        build(tmp, "nothing to see here", "// DeviceHub is the simulator UI on Xcode 27")
        r = run(tmp)
        if r.returncode != 1 or "empty set" not in r.stdout:
            print(f"zero mentions should be red as an empty set: exit {r.returncode}\n{r.stdout}")
            fail = 1

    with tempfile.TemporaryDirectory() as tmp:
        build(tmp, "close the Simulator.app window first (Xcode <= 26)", "// Simulator.app on Xcode <= 26 only")
        r = run(tmp)
        if r.returncode != 1 or "DeviceHub" not in r.stdout:
            print(f"a guard that never mentions DeviceHub should be red: exit {r.returncode}\n{r.stdout}")
            fail = 1

    if fail:
        return 1
    print("an-app-that-is-gone.test: unqualified, qualified, empty, and a guard without Device Hub are each judged as they should be")
    return 0


if __name__ == "__main__":
    sys.exit(main())
