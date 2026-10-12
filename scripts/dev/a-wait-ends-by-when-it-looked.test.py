#!/usr/bin/env python3
"""The gate refuses each spelling of "now against the deadline" it exists
for, passes a wait that judges by when its look began, and does not call
an empty tree clean."""

import os
import subprocess
import sys
import tempfile

GATE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "a-wait-ends-by-when-it-looked.py")

CASES = [
    ("checks the clock after the look", "if std::time::Instant::now() >= deadline { return false; }\n", True),
    ("checks the clock before the look", "while Instant::now() < deadline { look(); }\n", True),
    ("the deadline on the left", "if deadline <= Instant::now() { break; }\n", True),
    ("a named deadline", "if Instant::now() >= settle_deadline { break; }\n", True),
    ("judges by when the look began", "let look = deadline.look();\nif deadline.passed_by(look) { break; }\n", False),
    ("a began taken before the look", "let began = Instant::now();\nif began >= deadline { break; }\n", False),
    ("only in a comment", "// once Instant::now() >= deadline the old code gave up\n", False),
]

failures = []
for name, body, must_refuse in CASES:
    with tempfile.TemporaryDirectory() as tmp:
        src = os.path.join(tmp, "crates", "x", "src")
        os.makedirs(src)
        with open(os.path.join(src, "lib.rs"), "w") as fh:
            fh.write(body)
        r = subprocess.run([sys.executable, GATE], env={**os.environ, "SMIX_GATE_ROOT": tmp},
                           capture_output=True, text=True)
        said = r.stdout + r.stderr
        if (r.returncode != 0) != must_refuse or "Traceback" in said:
            failures.append(f"{name}: expected {'a refusal' if must_refuse else 'a pass'}\n{said}")
        else:
            print(f"  {name} → {'refused' if must_refuse else 'allowed'}")

with tempfile.TemporaryDirectory() as tmp:
    r = subprocess.run([sys.executable, GATE], env={**os.environ, "SMIX_GATE_ROOT": tmp},
                       capture_output=True, text=True)
    if r.returncode == 0:
        failures.append(f"a tree with no Rust source passed:\n{r.stdout}")
    else:
        print("  a tree with no Rust source → refused (reading air)")

if failures:
    print("a-wait-ends-by-when-it-looked.test: FAIL")
    for f in failures:
        print(f"  - {f}")
    sys.exit(1)
print(f"a-wait-ends-by-when-it-looked.test: {len(CASES) + 1} cases pass")
