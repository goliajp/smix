"""The smix a Python script in this tree drives — asked of the one resolver.

`scripts/lib/e2e-binary.sh` decides which binary every script drives:
this tree's debug build, unless SMIX_BIN names another. The Python gates
had their own answers — `./target/release/smix` relative to wherever
they were started, or the newer of the two builds — so a hand run of a
gate could judge a different binary than the e2e beside it (M2). This
asks the shell resolver rather than restating it.
"""

from __future__ import annotations

import os
import subprocess

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
RESOLVER = os.path.join(ROOT, "scripts", "lib", "e2e-binary.sh")


def this_tree_smix() -> str:
    """The binary to drive, or SystemExit naming why there is none."""
    r = subprocess.run(
        ["bash", "-c", '. "$1" >/dev/null && printf %s "$SMIX"', "_", RESOLVER],
        capture_output=True,
        text=True,
        check=False,
    )
    if r.returncode != 0 or not r.stdout:
        raise SystemExit(r.stderr.strip() or f"{RESOLVER} named no binary")
    return r.stdout
