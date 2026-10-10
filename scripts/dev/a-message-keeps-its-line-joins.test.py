#!/usr/bin/env python3
"""The line-joins gate can still go red, and only on a sentence it breaks."""

import os
import subprocess
import sys
import tempfile

GATE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "a-message-keeps-its-line-joins.py")

FILLER = "\n".join(f'fn f{i}() -> &\'static str {{ "line {i}" }}' for i in range(12))


def tree(extra: str) -> str:
    root = tempfile.mkdtemp()
    for i in range(120):
        d = os.path.join(root, "crates", f"c{i}", "src")
        os.makedirs(d)
        with open(os.path.join(d, "lib.rs"), "w") as fh:
            fh.write(FILLER + ("\n" + extra if i == 0 else ""))
    return root


def run(extra: str) -> tuple[int, str]:
    out = subprocess.run([sys.executable, GATE, "--root", tree(extra)], capture_output=True, text=True)
    return out.returncode, out.stdout


CASES = [
    ("a sentence broken without a backslash is red",
     'fn e() -> String { "cannot tell which device this is — it is not in the\n             registry".to_string() }', 1),
    ("the same sentence joined with a backslash is clean",
     'fn e() -> String { "cannot tell which device this is — it is not in the \\\n             registry".to_string() }', 0),
    ("a list laid out under a colon is clean",
     'fn h() -> &\'static str { "Options:\n    --device  the device\n    --port    the port" }', 0),
    ("a raw string is clean",
     'fn r() -> &\'static str { r"one\n         two" }', 0),
    ("a test module is not read",
     '#[cfg(test)]\nmod tests { fn t() -> &\'static str { "broken in the\n        middle" } }', 0),
]

failed = 0
for name, src, want in CASES:
    got, out = run(src)
    ok = got == want
    failed += not ok
    print(f"{'ok  ' if ok else 'FAIL'} {name}: exit {got}, wanted {want}")
    if not ok:
        print("     " + out.strip().replace("\n", "\n     "))
if failed:
    print(f"line-joins.test: FAIL — {failed} case(s)")
    sys.exit(1)
print(f"line-joins.test: clean — {len(CASES)} cases")
