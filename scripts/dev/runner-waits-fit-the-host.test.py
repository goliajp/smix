#!/usr/bin/env python3
"""What `runner-waits-fit-the-host.py` must answer. Each case copies the
real files, changes one thing, and the gate must name it; the unchanged
copy must be clean."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
GATE = os.path.join(ROOT, "scripts", "dev", "runner-waits-fit-the-host.py")

TABLE = "crates/smix-runner-client/src/route_limits.rs"
IOS = "swift-bridge/Sources/SmixRunnerCore/SmixRunnerServer.swift"
ANDROID = "android-runner/app/src/androidTest/kotlin/dev/smix/runner/RunnerTest.kt"
TIMELINE = "swift-bridge/Sources/SmixRunnerCore/TouchTimeline.swift"
BURST = "android-runner/app/src/main/kotlin/dev/smix/runner/TapBurst.kt"
HOST_DIR = "crates/smix-runner-client/src"

# (name, file, old, new, what the verdict must mention)
RED = [
    ("the runner says more than the table",
     IOS, "// LONGEST WAIT /back: 20500 ms", "// LONGEST WAIT /back: 30500 ms",
     "/back: ios says 30500"),
    ("the table says less than the runner",
     TABLE, 'r("/back", ms(20_500), ms(2_000), true)', 'r("/back", ms(0), ms(2_000), true)',
     "/back: ios says 20500 and"),
    ("a route with no statement",
     ANDROID, "// LONGEST WAIT /clear-text:", "// nothing to say about /clear-text:",
     "android: /clear-text is registered and states no longest wait"),
    ("a statement for a route nobody registers",
     ANDROID, "// LONGEST WAIT /windows: 0 ms", "// LONGEST WAIT /windows: 0 ms\n                // LONGEST WAIT /ghost: 0 ms",
     "which it does not register"),
    ("a route the table does not have",
     TABLE, '    r("/windows", NONE, ms(0), false),\n', "",
     "/windows is registered and missing"),
    ("a table row for a route the runner does not register",
     TABLE, 'r("/display", NONE, ms(0), false)', 'r("/display", ms(0), ms(0), false)',
     "/display is listed for ios"),
    ("a request-set route sent with the plain wait",
     "crates/smix-runner-client/src/hide_keyboard.rs", ".json_post_within(", ".json_post(",
     "/hide-keyboard: its wait is set by the request"),
    ("a fixed route turned into a request-set one the host does not size",
     TABLE, 'r("/swipe-once", ms(0), ms(500), true)', 'r("/swipe-once", ms(0), REQ, true)',
     "/swipe-once: android says 500 and"),
    ("the host assumes another burst interval",
     TIMELINE, "defaultIntervalMs: Int = 80", "defaultIntervalMs: Int = 120",
     "burst interval: the host assumes 80 ms and iOS TouchTimeline uses 120 ms"),
    ("the host calls a route the table does not have",
     "crates/smix-runner-client/src/hide_keyboard.rs", '"/hide-keyboard",\n', '"/hide-keyboard-now",\n',
     "the host calls /hide-keyboard-now"),
    ("a route that states its wait twice",
     IOS, "// LONGEST WAIT /tap: 3000 ms", "// LONGEST WAIT /tap: 3000 ms\n    // LONGEST WAIT /tap: 3000 ms",
     "/tap states its longest wait twice"),
    ("a reading that finds no routes",
     ANDROID, 'uri == "', 'path == "',
     "read only 0 registered routes"),
    ("a table whose rows cannot be read",
     TABLE, '    r("/', '    row("/',
     "read only 3 routes"),
]

# Cases whose change is every occurrence rather than the first.
EVERY = {"a reading that finds no routes", "a table whose rows cannot be read"}

problems: list[str] = []


def tree() -> str:
    t = tempfile.mkdtemp()
    for rel in (TABLE, IOS, ANDROID, TIMELINE, BURST):
        os.makedirs(os.path.dirname(os.path.join(t, rel)), exist_ok=True)
        shutil.copy(os.path.join(ROOT, rel), os.path.join(t, rel))
    os.makedirs(os.path.join(t, HOST_DIR), exist_ok=True)
    for name in os.listdir(os.path.join(ROOT, HOST_DIR)):
        if name.endswith(".rs"):
            shutil.copy(os.path.join(ROOT, HOST_DIR, name), os.path.join(t, HOST_DIR, name))
    return t


def run(root: str) -> tuple[int, str]:
    p = subprocess.run([sys.executable, GATE, "--root", root], capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


root = tree()
rc, out = run(root)
if rc != 0:
    problems.append(f"the unchanged files should be clean:\n{out}")
shutil.rmtree(root)

for name, rel, old, new, must in RED:
    root = tree()
    path = os.path.join(root, rel)
    text = open(path).read()
    if old not in text:
        problems.append(f"{name}: the case no longer applies — {old!r} is not in {rel}")
        shutil.rmtree(root)
        continue
    open(path, "w").write(text.replace(old, new) if name in EVERY else text.replace(old, new, 1))
    rc, out = run(root)
    if rc != 1:
        problems.append(f"{name}: exit {rc}, wanted 1:\n{out}")
    elif "Traceback" in out:
        problems.append(f"{name}: red by crash, not by verdict:\n{out}")
    elif must not in out:
        problems.append(f"{name}: red, but not for this — wanted {must!r}:\n{out}")
    shutil.rmtree(root)

if problems:
    print("runner-waits-fit-the-host.test: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)
print(f"runner-waits-fit-the-host.test: clean — the real files pass and {len(RED)} changes are each named")
