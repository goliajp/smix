#!/usr/bin/env python3
"""Does the publish-DAG gate go red on each way the list can be wrong?

Every rule in the gate is removed here in turn — as a mutation of the
input rather than of the code — and each mutation must produce a
sentence naming what is wrong. A rule that has never been red has not
been verified, and a red that is a traceback has not been read.
"""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "publish-dag-is-complete.py")
ROOT = os.path.dirname(os.path.dirname(HERE))


def run(root: str) -> tuple[int, str]:
    p = subprocess.run(
        [sys.executable, GATE, root], capture_output=True, text=True
    )
    return p.returncode, p.stdout + p.stderr


def tree_with_ship(ship_text: str) -> str:
    """A copy of this workspace whose ship.sh is `ship_text`.

    Symlinked rather than copied: cargo metadata must see the real
    manifests, and the point of each case is that one side of the
    comparison is wrong while the other is genuine.
    """
    d = tempfile.mkdtemp()
    for name in os.listdir(ROOT):
        if name in (".git", "target"):
            continue
        os.symlink(os.path.join(ROOT, name), os.path.join(d, name))
    os.unlink(os.path.join(d, "scripts"))
    scripts = os.path.join(d, "scripts")
    os.makedirs(os.path.join(scripts, "release"))
    for sub in os.listdir(os.path.join(ROOT, "scripts")):
        if sub != "release":
            os.symlink(
                os.path.join(ROOT, "scripts", sub), os.path.join(scripts, sub)
            )
    with open(os.path.join(scripts, "release", "ship.sh"), "w") as fh:
        fh.write(ship_text)
    return d


REAL_SHIP = open(
    os.path.join(ROOT, "scripts", "release", "ship.sh"), encoding="utf-8"
).read()


def case(name: str, ship_text: str, must_say: str) -> bool:
    root = tree_with_ship(ship_text)
    code, out = run(root)
    if code == 0:
        print(f"  FAIL {name}: gate passed on input it should refuse")
        return False
    if "Traceback" in out:
        print(f"  FAIL {name}: red by raising, not by judging\n{out}")
        return False
    if must_say not in out:
        print(f"  FAIL {name}: red, but does not say {must_say!r}\n{out}")
        return False
    print(f"  ok   {name}")
    return True


def main() -> int:
    ok = True

    # A crate nothing depends on, dropped: the silent one.
    ok &= case(
        "a crate missing from the list",
        REAL_SHIP.replace("  smix-usbmux\n", "", 1),
        "smix-usbmux is a publishable crate and is not in",
    )

    # A name that is not a crate.
    ok &= case(
        "a stale name in the list",
        REAL_SHIP.replace("  smix-usbmux\n", "  smix-usbmux smix-gone\n", 1),
        "which is not a publishable crate",
    )

    # Same crate twice.
    ok &= case(
        "a crate listed twice",
        REAL_SHIP.replace("  smix-usbmux\n", "  smix-usbmux smix-usbmux\n", 1),
        "more than once",
    )

    # Order broken: put the CLI first, before everything it depends on.
    ok &= case(
        "a crate published before what it depends on",
        REAL_SHIP.replace("CRATES=(\n", "CRATES=(\n  smix-cli\n", 1).replace(
            "\n  smix-cli\n)", "\n)", 1
        ),
        "which it depends on",
    )

    # The empty predicate: no array at all. An empty list and an empty
    # workspace would agree, so this must refuse rather than pass.
    ok &= case(
        "no list at all",
        REAL_SHIP.replace("CRATES=(", "CRATES_RENAMED=(", 1),
        "this is not a publish list",
    )

    # And the real tree passes.
    code, out = run(ROOT)
    if code != 0:
        print(f"  FAIL the real tree: gate is red on a tree that is correct\n{out}")
        ok = False
    else:
        print("  ok   the real tree passes")

    print("=== publish-dag-is-complete.test:", "PASS" if ok else "FAIL", "===")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
