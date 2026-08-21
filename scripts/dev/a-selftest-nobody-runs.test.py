#!/usr/bin/env python3
"""Does the orphan-self-test gate go red when a self-test is orphaned?

This one has to pass its own rule: a gate about self-tests nobody runs,
with no self-test, would be the joke it exists to prevent.
"""

import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "a-selftest-nobody-runs.py")
ROOT = os.path.dirname(os.path.dirname(HERE))


def tree(drop_reference=None, extra_selftest=None):
    # No `str | None` in the signature: /usr/bin/python3 on this machine
    # is old enough that the annotation is evaluated and raises at
    # import. preflight runs under a newer interpreter and the ship runs
    # under a login shell — a script that only loads in one of them
    # passes every rehearsal and fails the performance.
    d = tempfile.mkdtemp()
    os.makedirs(os.path.join(d, ".github", "workflows"))
    shutil.copytree(os.path.join(ROOT, "scripts"), os.path.join(d, "scripts"))
    for name in os.listdir(os.path.join(ROOT, ".github", "workflows")):
        shutil.copy(
            os.path.join(ROOT, ".github", "workflows", name),
            os.path.join(d, ".github", "workflows", name),
        )
    if drop_reference:
        for rel in (
            os.path.join("scripts", "dev", "preflight.sh"),
            os.path.join(".github", "workflows", "ci.yml"),
            os.path.join("scripts", "release", "ship.sh"),
        ):
            p = os.path.join(d, rel)
            if os.path.isfile(p):
                s = open(p, encoding="utf-8").read()
                open(p, "w", encoding="utf-8").write(s.replace(drop_reference, "REMOVED"))
    if extra_selftest:
        open(os.path.join(d, "scripts", "dev", extra_selftest), "w").write("# nothing\n")
    return d


def case(name: str, root: str, must_say: str) -> bool:
    p = subprocess.run([sys.executable, GATE, root], capture_output=True, text=True)
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

    # A self-test that exists and nothing names.
    ok &= case(
        "a brand new self-test nobody wired",
        tree(extra_selftest="never-wired-anywhere.test.py"),
        "is invoked by nothing",
    )

    # An existing one whose references are removed — the shape that
    # actually happened, an hour after the gate it tests was written.
    ok &= case(
        "an existing self-test loses its references",
        tree(drop_reference="publish-dag-is-complete.test"),
        "is invoked by nothing",
    )

    # An exemption that outlived its subject. The list is empty today,
    # and empty is when this matters most: whoever adds the first entry
    # has a reason that day, and nobody reads it again. A name that no
    # longer exists then goes on excusing nothing while reading as a
    # considered decision.
    d = tree()
    gate_copy = os.path.join(d, "scripts", "dev", "a-selftest-nobody-runs.py")
    src = open(gate_copy, encoding="utf-8").read()
    open(gate_copy, "w", encoding="utf-8").write(
        src.replace(
            "DRIVEN_ELSEWHERE: dict[str, str] = {}",
            'DRIVEN_ELSEWHERE: dict[str, str] = {"gone-long-ago.test.py": "an e2e"}',
            1,
        )
    )
    q = subprocess.run([sys.executable, gate_copy, d], capture_output=True, text=True)
    if q.returncode == 0 or "outlived its subject" not in q.stdout:
        print(f"  FAIL a stale exemption\n{q.stdout}{q.stderr}")
        ok = False
    else:
        print("  ok   a stale exemption")

    p = subprocess.run([sys.executable, GATE, ROOT], capture_output=True, text=True)
    if p.returncode != 0:
        print(f"  FAIL the real tree: red on a tree that is correct\n{p.stdout}")
        ok = False
    else:
        print("  ok   the real tree passes")

    print("=== a-selftest-nobody-runs.test:", "PASS" if ok else "FAIL", "===")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
