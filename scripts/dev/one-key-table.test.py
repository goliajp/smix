#!/usr/bin/env python3
"""What `one-key-table.py` must answer: each shape of a second key
reader is red and named; test code, comments and the table itself are
not; a tree without the table is red."""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
GATE = os.path.join(ROOT, "scripts", "dev", "one-key-table.py")

TABLE = "const NAMES: [(&str, KeyName); 20] = [\n" + "".join(
    f'    ("k{i}", KeyName::Return),\n' for i in range(20)
) + "];\n"

MUST_FLAG = {
    "a match arm": 'fn p(s: &str) -> Option<KeyName> { match s {\n    "home" => Some(KeyName::Home),\n    _ => None } }\n',
    "a lookup pair": 'const T: [(&str, KeyName); 1] = [\n    ("back", KeyName::Back),\n];\n',
    "a serde wire parse": "fn q(s: String) { let key: KeyName = parse_wire_enum(&s, \"key\").unwrap(); }\n",
}
MUST_PASS = {
    "test code": '#[cfg(test)]\nmod t {\n    fn p(s: &str) { match s {\n        "home" => KeyName::Home,\n        _ => KeyName::Back }; }\n}\n',
    "a comment": '// "home" => KeyName::Home,\n',
    "from_name": 'fn r(s: &str) { let _ = KeyName::from_name(s); }\n',
}

problems: list[str] = []


def verdict(body: str, table: str = TABLE) -> tuple[int, str]:
    with tempfile.TemporaryDirectory() as t:
        for krate, text in (("smix-input", table), ("demo", body)):
            src = os.path.join(t, "crates", krate, "src")
            os.makedirs(src)
            with open(os.path.join(src, "lib.rs"), "w") as fh:
                fh.write(text)
        r = subprocess.run([sys.executable, GATE, "--root", t], capture_output=True, text=True)
        return r.returncode, r.stdout + r.stderr


for name, body in MUST_FLAG.items():
    code, out = verdict(body)
    if code == 0:
        problems.append(f"{name}: not flagged\n{out}")
    elif "Traceback" in out:
        problems.append(f"{name}: red by crashing\n{out}")
    elif "demo/src/lib.rs:" not in out:
        problems.append(f"{name}: red without naming the line\n{out}")
for name, body in MUST_PASS.items():
    code, out = verdict(body)
    if code != 0:
        problems.append(f"{name}: flagged, and it is not the shape\n{out}")
code, out = verdict("fn nothing() {}\n", table="pub enum KeyName { Return }\n")
if code == 0:
    problems.append(f"a tree without the table passed:\n{out}")

if problems:
    print("one-key-table.test: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)
print(f"one-key-table.test: {len(MUST_FLAG)} shapes flagged, {len(MUST_PASS)} near-misses passed, a missing table refused")
