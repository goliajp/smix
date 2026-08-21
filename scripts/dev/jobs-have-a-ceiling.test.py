#!/usr/bin/env python3
"""Does the ceiling gate go red on each way a job can lack one?

The by-hand sweep of this gate had a mutation that did not land — an
edit that removed a line shared with another job rather than the one
under test, which read exactly like a rule that was never bitten. Each
case here edits one named job, so what is removed is what was meant.
"""

import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "jobs-have-a-ceiling.py")
ROOT = os.path.dirname(os.path.dirname(HERE))
WF = os.path.join(".github", "workflows", "ci.yml")
REAL = open(os.path.join(ROOT, WF), encoding="utf-8").read()


def without_ceiling(text: str, job: str) -> str:
    """Remove the timeout of one named job, and no other."""
    i = text.index(f"\n  {job}:\n")
    j = text.index("    timeout-minutes:", i)
    k = text.index("\n", j) + 1
    return text[:j] + text[k:]


def with_ceiling(text: str, job: str, minutes: int) -> str:
    i = text.index(f"\n  {job}:\n")
    j = text.index("    timeout-minutes:", i)
    k = text.index("\n", j)
    return text[:j] + f"    timeout-minutes: {minutes}" + text[k:]


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
        "a job with no ceiling",
        without_ceiling(REAL, "source-gates"),
        "has no timeout-minutes",
    )

    ok &= case(
        "a ceiling near the six-hour default",
        with_ceiling(REAL, "portable-corpus", 300),
        "the absence wearing a number",
    )

    # The gate reads structurally. If that reading breaks it would find
    # nothing to complain about, which is not the same as nothing being
    # wrong.
    ok &= case(
        "the reader cannot find the jobs block",
        REAL.replace("jobs:", "JOBS:", 1),
        "this is not reading the workflows",
    )

    # Partial blindness: the walk reads one job fewer while the file
    # still has all its `runs-on:` lines. A floor cannot catch this —
    # eight of nine is well above it — and without a second reading the
    # gate would report perfect compliance over a job it never saw.
    #
    # The first attempt at this mutation did not land: it changed a job
    # key to mixed case, which the walk accepts, so nothing was hidden
    # and nothing went red. Indenting by three spaces makes it genuinely
    # unreadable as a job while leaving its runs-on in place.
    ok &= case(
        "the walk reads one job fewer than the file has",
        REAL.replace("\n  ts-sdk:\n", "\n   ts-sdk:\n", 1),
        "must agree",
    )

    # And the mutation that has to land: removing one job's ceiling must
    # name THAT job, not another.
    p = subprocess.run(
        [sys.executable, GATE, tree_with(without_ceiling(REAL, "ts-sdk"))],
        capture_output=True,
        text=True,
    )
    if "`ts-sdk`" not in p.stdout:
        print(f"  FAIL the mutation lands on the named job\n{p.stdout}")
        ok = False
    else:
        print("  ok   the mutation lands on the named job")

    p = subprocess.run([sys.executable, GATE, ROOT], capture_output=True, text=True)
    if p.returncode != 0:
        print(f"  FAIL the real tree: red on a tree that is correct\n{p.stdout}")
        ok = False
    else:
        print("  ok   the real tree passes")

    print("=== jobs-have-a-ceiling.test:", "PASS" if ok else "FAIL", "===")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
