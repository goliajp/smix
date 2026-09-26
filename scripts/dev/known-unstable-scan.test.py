#!/usr/bin/env python3
"""What `known-unstable-scan.py` must answer, with and without a list.

The list is a local input named by SMIX_KNOWN_UNSTABLE. Without one the
corpus gate excuses nothing, so the scan passes and says so; with one it
holds every row to the bar; with a path that is not there it is red,
because somebody meant to give a list that is not being read.
"""

import glob
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SCAN = os.path.join(ROOT, "scripts", "dev", "known-unstable-scan.py")
CORPUS = os.path.join(ROOT, "scripts", "release", "stress-corpus")

problems = []


def run(list_path):
    env = {k: v for k, v in os.environ.items() if k != "SMIX_KNOWN_UNSTABLE"}
    if list_path is not None:
        env["SMIX_KNOWN_UNSTABLE"] = list_path
    out = subprocess.run([sys.executable, SCAN], capture_output=True, text=True, env=env)
    return out.returncode, out.stdout + out.stderr


def expect(label, ok, detail):
    if not ok:
        problems.append(f"{label}: {detail}")


flows = sorted(glob.glob(os.path.join(CORPUS, "*.yaml")))
if not flows:
    print("known-unstable-scan.test: FAIL\n  - no corpus flow to name — the fixture has no subject")
    sys.exit(1)
flow = os.path.basename(flows[0])[: -len(".yaml")]
HEAD = "| Flow | Symptom | Measured rate | Attempts | Notes |\n|---|---|---|---|---|\n"
GOOD = f"| `{flow}` | the list scrolls past the row and the tap lands on its neighbour | 3/40 runs | 4 attempts, see notes | — |\n"

code, out = run(None)
expect("no list given passes", code == 0, f"exit {code}:\n{out}")
expect("and says nothing is excused", "no flow is excused" in out, out)

with tempfile.TemporaryDirectory() as tmp:
    good = os.path.join(tmp, "good.md")
    open(good, "w").write(HEAD + GOOD)
    code, out = run(good)
    expect("a list held to the bar passes", code == 0, f"exit {code}:\n{out}")
    expect("and names the excused flow", flow in out, out)

    vague = os.path.join(tmp, "vague.md")
    open(vague, "w").write(HEAD + GOOD.replace("3/40 runs", "sometimes"))
    code, out = run(vague)
    expect("a row with no measured rate is red", code == 1, f"exit {code}:\n{out}")

    ghost = os.path.join(tmp, "ghost.md")
    open(ghost, "w").write(HEAD + GOOD.replace(flow, "no-such-flow-here"))
    code, out = run(ghost)
    expect("a row naming no flow is red", code == 1, f"exit {code}:\n{out}")

    code, out = run(os.path.join(tmp, "absent.md"))
    expect("a named path that is not there is red", code == 1, f"exit {code}:\n{out}")

if problems:
    print("known-unstable-scan.test: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)
print("known-unstable-scan.test: 5 cases pass")
