#!/usr/bin/env python3
"""Does the verdict sweep go red, and only for the right reasons?

The sweep asserts three things about every verdict. Each is checked here
against a script written to break exactly one of them, plus a control
that satisfies all three — because a sweep that cannot go red is the
shape it exists to find.

The fourth case is the one this sweep got wrong twice while being
written: it counted its own shared loader as a subject, and it handed a
script the wrong kind of argument and reported the resulting
FileNotFoundError as the verdict crashing. A library is not a subject.
"""

import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
SWEEP = os.path.join(HERE, "a-verdict-answers-in-sentences.py")

GOOD = '''import sys
from verdict_io import read_json
def main():
    doc = read_json(sys.argv[1], "the tree")
    # Content, not presence. A children list whose entries are all nulls
    # is the shape a device answers with when it has nothing to say, and
    # a check for the key alone reads that as evidence — which is what
    # this fixture got wrong on its first run.
    if not any((c or {}).get("identifier") for c in doc.get("children") or []):
        print("no identifiable node in the tree", file=sys.stderr)
        return 1
    return 0
if __name__ == "__main__":
    sys.exit(main())
'''

CRASHES = '''import json, sys
def main():
    return 0 if json.load(open(sys.argv[1]))["nope"] else 1
if __name__ == "__main__":
    sys.exit(main())
'''

PASSES_ON_RUBBISH = '''import sys
from verdict_io import read_json
def main():
    read_json(sys.argv[1], "the tree")
    return 0
if __name__ == "__main__":
    sys.exit(main())
'''

SILENT = '''import sys
from verdict_io import read_json
def main():
    read_json(sys.argv[1], "the tree")
    return 1
if __name__ == "__main__":
    sys.exit(main())
'''

LIBRARY = '''"""A shared helper, not a verdict. No entry point."""
def helper():
    return 1
'''


def tree(name, body):
    d = tempfile.mkdtemp()
    rel = os.path.join(d, "scripts", "release")
    os.makedirs(rel)
    shutil.copy(os.path.join(os.path.dirname(HERE), "release", "verdict_io.py"), rel)
    with open(os.path.join(rel, name), "w") as f:
        f.write(body)
    return d


def run(root):
    d = subprocess.run([sys.executable, SWEEP, root], capture_output=True, text=True)
    return d.returncode, d.stdout + d.stderr


def expect(label, name, body, want_red, must_say=None):
    root = tree(name, body)
    try:
        rc, out = run(root)
        if "Traceback" in out:
            print(f"FAIL [{label}]: the sweep crashed:\n{out}")
            return False
        if want_red and rc == 0:
            print(f"FAIL [{label}]: stayed green\n{out}")
            return False
        if not want_red and rc != 0:
            print(f"FAIL [{label}]: went red on a correct subject\n{out}")
            return False
        if must_say and must_say not in out:
            print(f"FAIL [{label}]: wanted {must_say!r} in:\n{out}")
            return False
        print(f"ok   [{label}]")
        return True
    finally:
        shutil.rmtree(root, ignore_errors=True)


def main():
    ok = True
    ok &= expect("a verdict that judges", "a-verdict.py", GOOD, False)
    ok &= expect("one that crashes", "b-verdict.py", CRASHES, True, "crashed instead")
    ok &= expect("one that passes on rubbish", "c-verdict.py", PASSES_ON_RUBBISH,
                 True, "answered 0")
    ok &= expect("one that is red and silent", "d-verdict.py", SILENT,
                 True, "said nothing")
    ok &= expect("a library beside them is not a subject", "e-verdict.py",
                 LIBRARY, True, "CANNOT RUN")

    empty = tempfile.mkdtemp()
    os.makedirs(os.path.join(empty, "scripts", "release"))
    rc, out = run(empty)
    if rc == 0 or "CANNOT RUN" not in out:
        print(f"FAIL [no subjects at all]: a sweep that found nothing must not pass\n{out}")
        ok = False
    else:
        print("ok   [no subjects at all]")
    shutil.rmtree(empty, ignore_errors=True)

    print("verdict-sweep self-test: " + ("PASS" if ok else "FAIL"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
