#!/usr/bin/env python3
"""The sweep finds the gate that says yes about nothing — and only it.

Two fake gates in a fake repo. One reads a file and judges it; one reads
the same file and returns 0 whatever it says. The second is the shape
this whole instrument exists to find, and a sweep that reports both, or
neither, would be useless in opposite directions.

The third fake reads nothing at all. It must land in "no subject to
take" rather than in the findings: a self-test that builds its own
fixture is not a gate with a missing subject, and calling it one would
force an exemption list — which is the escape hatch this repo has
watched become a hiding place.
"""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
SCANNER = os.path.join(HERE, "a-gate-without-its-subject.py")

HONEST = '''#!/usr/bin/env python3
import os, sys
p = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "subject.txt")
try:
    body = open(p, encoding="utf-8").read()
except OSError:
    print("its subject is not here, which proves nothing about it", file=sys.stderr)
    sys.exit(1)
sys.exit(0 if "good" in body else 1)
'''

HOLLOW = '''#!/usr/bin/env python3
import os
p = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "subject.txt")
try:
    open(p, encoding="utf-8").read()
except OSError:
    pass
print("nothing to complain about")
raise SystemExit(0)
'''

SELFCONTAINED = '''#!/usr/bin/env python3
print("built its own fixture; reads nothing from the repo")
raise SystemExit(0)
'''

SHIP = '''#!/usr/bin/env bash
python3 "$ROOT/scripts/dev/honest-gate.py"
python3 "$ROOT/scripts/dev/hollow-gate.py"
python3 "$ROOT/scripts/dev/selfcontained-gate.py"
'''


def build(root: str) -> None:
    os.makedirs(os.path.join(root, "scripts", "dev"), exist_ok=True)
    os.makedirs(os.path.join(root, "scripts", "release"), exist_ok=True)
    for name, body in (
        ("honest-gate.py", HONEST),
        ("hollow-gate.py", HOLLOW),
        ("selfcontained-gate.py", SELFCONTAINED),
    ):
        open(os.path.join(root, "scripts", "dev", name), "w", encoding="utf-8").write(body)
    open(os.path.join(root, "scripts", "release", "ship.sh"), "w", encoding="utf-8").write(SHIP)
    open(os.path.join(root, "subject.txt"), "w", encoding="utf-8").write("good\n")
    subprocess.run(["git", "init", "-q"], cwd=root, check=True)
    subprocess.run(["git", "add", "-A"], cwd=root, check=True)


def main() -> int:
    with tempfile.TemporaryDirectory() as root:
        build(root)
        done = subprocess.run(
            [sys.executable, SCANNER, root], capture_output=True, text=True, timeout=300
        )
    out = done.stdout + done.stderr
    problems = []

    if done.returncode == 0:
        problems.append(
            "the sweep answered 0 with a hollow gate in the tree — it would "
            "answer 0 with the real ones too"
        )
    if "hollow-gate.py" not in out:
        problems.append("the hollow gate was not named in the findings")
    for line in out.splitlines():
        if line.strip().startswith("scripts/dev/honest-gate.py: exit 0"):
            problems.append(
                "the honest gate was reported as a finding — it refused when "
                "its subject was gone, which is the behaviour being asked for"
            )
    if "selfcontained-gate.py" not in out:
        problems.append("the gate that reads nothing was not reported at all")
    else:
        after = out.split("selfcontained-gate.py", 1)[1]
        if "exit 0 after" in after.split("\n")[0]:
            problems.append(
                "a gate that reads nothing was called a finding — that forces "
                "an exemption list, which is what this avoids"
            )

    if problems:
        print("gate-subject.test: RED")
        for p in problems:
            print(f"  {p}")
        print("\n--- scanner output ---")
        print(out)
        return 1
    print("gate-subject.test: clean — names the hollow gate, and only it")
    return 0


if __name__ == "__main__":
    sys.exit(main())
