#!/usr/bin/env python3
"""Self-test for three-readers-agree.

It needs no device and guards a thing that rots silently — three copies of
one recorded document, each with its own green suite. A gate like that has
no excuse for being unverified, and the other v10 device gates are checked
by driving them instead because their judgement cannot leave the emulator.

Built the way the gate is: a real tree, with real files, taken away one at a
time. 9.0.0 spent three rounds on scanners whose fixtures were assembled
differently from the thing they scanned.
"""

import pathlib
import shutil
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
GATE = HERE / "three-readers-agree.py"
TREES = [
    "crates/smix-adapter-maestro/tests/fixtures/reports",
    "swift-bridge/Tests/SmixSDKTests/fixtures",
    "android-runner/sdk/src/test/resources/reports",
]

failures = []


def check(label, cond, detail=""):
    if not cond:
        failures.append(f"{label}{': ' + detail if detail else ''}")


def run_against(root):
    """Run the gate with its paths pointed at a copy of the tree."""
    src = (GATE).read_text(encoding="utf-8").replace(
        'ROOT = pathlib.Path(__file__).resolve().parents[2]',
        f'ROOT = pathlib.Path({str(root)!r})',
    )
    # The suites are the slow half and are not what this is testing, so they
    # are stubbed out. What IS tested is the byte comparison — the half that
    # would otherwise never be exercised except by a real drift.
    src = src.replace('r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)',
                      'class _R: returncode = 0; stdout = ""; stderr = ""\n        r = _R()')
    tmp = root / "_gate_under_test.py"
    tmp.write_text(src, encoding="utf-8")
    p = subprocess.run([sys.executable, str(tmp)], capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


def main():
    with tempfile.TemporaryDirectory() as td:
        root = pathlib.Path(td)
        for t in TREES:
            dst = root / t
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copytree(ROOT / t, dst)

        rc, out = run_against(root)
        check("an unmodified copy should pass", rc == 0, out.strip()[:160])

        # One tree's copy edited.
        target = root / TREES[1] / "passing.xml"
        original = target.read_text(encoding="utf-8")
        target.write_text(original + "\n<!-- drift -->", encoding="utf-8")
        rc, out = run_against(root)
        check("an edited copy should red", rc != 0, out.strip()[:160])
        check("and should name the file", "passing.xml" in out, out.strip()[:160])
        target.write_text(original, encoding="utf-8")

        # One tree's copy missing.
        gone = root / TREES[2] / "failing.xml"
        body = gone.read_text(encoding="utf-8")
        gone.unlink()
        rc, out = run_against(root)
        check("a missing copy should red", rc != 0, out.strip()[:160])
        check("and should say which reader lost it", "kotlin" in out, out.strip()[:160])
        gone.write_text(body, encoding="utf-8")

        # No payloads at all. The presence half: with nothing recorded the
        # three readers agree perfectly about nothing.
        for f in (root / TREES[0]).glob("*.xml"):
            f.unlink()
        rc, out = run_against(root)
        check("no payloads at all should red", rc != 0, out.strip()[:160])
        check(
            "and should say the readers have nothing to disagree about",
            "nothing for the three readers" in out,
            out.strip()[:160],
        )

    if failures:
        print("three-readers-agree.test: FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1
    print("three-readers-agree.test: clean — 7 assertions over a real copied tree")
    return 0


if __name__ == "__main__":
    sys.exit(main())
