#!/usr/bin/env python3
"""Does the pinning gate go red on each way a reference can be wrong?

Written the same afternoon as the gate, because the gate's rules had
been swept by hand exactly once and a sweep that ran once is a claim
about that minute.
"""

import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "actions-are-pinned.py")
ROOT = os.path.dirname(os.path.dirname(HERE))
WF = os.path.join(".github", "workflows", "ci.yml")
REAL = open(os.path.join(ROOT, WF), encoding="utf-8").read()


def tree_with(workflow_text: str) -> str:
    d = tempfile.mkdtemp()
    os.makedirs(os.path.join(d, ".github", "workflows"))
    with open(os.path.join(d, WF), "w", encoding="utf-8") as fh:
        fh.write(workflow_text)
    return d


def case(name: str, text: str, must_say: str) -> bool:
    p = subprocess.run(
        [sys.executable, GATE, tree_with(text)], capture_output=True, text=True
    )
    out = p.stdout + p.stderr
    if p.returncode == 0:
        print(f"  FAIL {name}: passed on input it should refuse")
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

    ok &= case(
        "a moving tag",
        REAL.replace(
            "uses: oven-sh/setup-bun@0c5077e51419868618aeaa5fe8019c62421857d6 # v2.2.0",
            "uses: oven-sh/setup-bun@v2",
            1,
        ),
        "is pinned to a moving ref",
    )

    ok &= case(
        "a commit with no version beside it",
        REAL.replace(
            "uses: Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6 # v2.9.2",
            "uses: Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6",
            1,
        ),
        "with nothing beside it",
    )

    ok &= case(
        "a reference with no ref at all",
        REAL.replace(
            "uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0",
            "uses: actions/checkout",
            1,
        ),
        "names no ref at all",
    )

    # The one the gate found in itself: a reader that stops matching
    # reports perfect compliance. The floor is what turns that into red.
    ok &= case(
        "the reader stops matching",
        REAL.replace("- uses:", "- USES:"),
        "this is not reading the workflows",
    )

    p = subprocess.run([sys.executable, GATE, ROOT], capture_output=True, text=True)
    if p.returncode != 0:
        print(f"  FAIL the real tree: red on a tree that is correct\n{p.stdout}{p.stderr}")
        ok = False
    else:
        print("  ok   the real tree passes")

    print("=== actions-are-pinned.test:", "PASS" if ok else "FAIL", "===")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
