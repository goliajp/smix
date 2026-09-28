#!/usr/bin/env python3
"""The inline-python gate goes red on the shape that broke a release."""

import importlib.util
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("gate", HERE / "inline-python-compiles.py")
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)

CASES = [
    # (label, script text, number of problems expected)
    ("a snippet indented with its function", "f() {\n  X=\"$(python3 -c 'import socket\n  s = socket.socket()\n  print(1)')\"\n}\n", 1),
    ("the same snippet at column zero", "f() {\n  X=\"$(python3 -c 'import socket\ns = socket.socket()\nprint(1)')\"\n}\n", 0),
    ("a one-line snippet is not this gate's business", "python3 -c 'print(1)'\n", 0),
    ("a snippet that does not parse at all", "python3 -c 'if True\n    print(1)'\n", 1),
    ("a variable spliced into the snippet", "python3 -c '\nimport sys\nif \"'\"$ID\"'\" in sys.argv:\n    print(1)'\n", 0),
    ("a broken snippet with a splice in it", "python3 -c '\nif \"'\"$ID\"'\" in x\n    print(1)'\n", 1),
]

fails = 0
for label, text, want in CASES:
    got = len(gate.problems("case.sh", text))
    if got != want:
        print(f"inline-python-compiles.test: {label} — {got} problem(s), wanted {want}")
        fails += 1
counted = sum(1 for _ in gate.snippets(CASES[0][1]))
if counted != 1:
    print(f"inline-python-compiles.test: the snippet finder saw {counted} snippet(s) in a script with one")
    fails += 1
if fails:
    sys.exit(1)
print(f"inline-python-compiles.test: clean — {len(CASES)} cases and the snippet count")
