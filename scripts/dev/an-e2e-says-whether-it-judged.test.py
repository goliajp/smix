#!/usr/bin/env python3
"""The scan above can go red, and for the reasons it claims.

Six cases: the two defects it exists for, the two shapes that are fine,
and the two ways the scan itself can stop working — a subject it cannot
find, and prose it mistakes for code.
"""

import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
GATE = os.path.join(ROOT, "scripts", "dev", "an-e2e-says-whether-it-judged.py")

GOOD = """#!/usr/bin/env bash
set -euo pipefail
source "$ROOT/scripts/lib/e2e-binary.sh"
cannot_judge() { printf 'no emulator: %s\\n' "$*" >&2; exit 2; }
fail() { printf 'FAIL: %s\\n' "$*" >&2; exit 1; }
command -v adb >/dev/null || cannot_judge "no adb"
"$SMIX" tap --device "$SERIAL" id:ok || fail "the tap did not land"
echo PASS
"""

SKIPS_WITH_ZERO = GOOD.replace("exit 2; }", "exit 0; }")
# The defect I wrote into three scripts while converting them: the call
# without the definition. It only fires on the path nothing exercises.
CALLS_UNDEFINED = GOOD.replace(
    'cannot_judge() { printf \'no emulator: %s\\n\' "$*" >&2; exit 2; }\n', ""
)
PICKS_A_BINARY = GOOD.replace(
    'source "$ROOT/scripts/lib/e2e-binary.sh"',
    'SMIX="${SMIX_BIN:-$ROOT/target/release/smix}"',
)
# Another machine's binary, invoked over ssh: named, but not this
# script's choice of what to drive.
PROSE_ONLY = GOOD.replace(
    '"$SMIX" tap',
    'rssh "cd \'$REMOTE_REPO\' && target/release/smix sim list"\n"$SMIX" tap',
)


def run(files, expect_rc, expect_in, label, min_scripts_ok=True):
    """Build a scripts/dev of our own and judge it."""
    fails = []
    with tempfile.TemporaryDirectory() as tmp:
        dev = os.path.join(tmp, "scripts", "dev")
        os.makedirs(dev)
        # The scan wants a repository's worth of subjects before it will
        # believe a clean answer, so pad with copies of a good one.
        pad = 0 if not min_scripts_ok else 54 - len(files)
        for i in range(max(pad, 0)):
            with open(os.path.join(dev, f"pad-{i}-e2e.sh"), "w") as fh:
                fh.write(GOOD)
        for name, body in files.items():
            with open(os.path.join(dev, name), "w") as fh:
                fh.write(body)
        r = subprocess.run(
            [sys.executable, GATE, tmp], capture_output=True, text=True
        )
    if r.returncode != expect_rc:
        fails.append(f"{label}: exit {r.returncode}, wanted {expect_rc}")
    if expect_in not in r.stdout:
        fails.append(f"{label}: output lacks {expect_in!r}\n{r.stdout}")
    return fails


def main():
    fails = []
    fails += run({"a-e2e.sh": GOOD}, 0, "clean", "a script that answers 0/1/2")
    fails += run(
        {"a-e2e.sh": SKIPS_WITH_ZERO},
        1,
        "exits 0",
        "the defect: a skip that reads like a pass",
    )
    fails += run(
        {"a-e2e.sh": PICKS_A_BINARY},
        1,
        "picks its own binary",
        "the defect: a script choosing its own binary",
    )
    fails += run(
        {"a-e2e.sh": PROSE_ONLY},
        0,
        "clean",
        "another host's binary is not this script's choice",
    )
    fails += run(
        {"a-e2e.sh": CALLS_UNDEFINED},
        1,
        "and does not define it",
        "the defect: standing aside through a function that is not there",
    )

    # The scan's own failure modes.
    fails += run(
        {"a-e2e.sh": GOOD},
        1,
        "lost its subject",
        "a directory with almost nothing in it",
        min_scripts_ok=False,
    )
    fails += run(
        {"a-e2e.sh": SKIPS_WITH_ZERO, "b-e2e.sh": PICKS_A_BINARY},
        1,
        "b-e2e.sh",
        "both defects are named, not just the first",
    )

    if fails:
        print("an-e2e-says-whether-it-judged.test: FAIL")
        for f in fails:
            print(f"  - {f}")
        return 1
    print(
        "an-e2e-says-whether-it-judged.test: a skip that exits 0, a script "
        "picking its own binary, prose about one, a lost subject, and two "
        "defects at once are each judged as they should be"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
