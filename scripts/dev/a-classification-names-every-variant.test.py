#!/usr/bin/env python3
"""What `a-classification-names-every-variant.py` must answer.

Its subject: `matches!(x, E::A | E::B)` on one of this repository's own
enums, used to sort values into a class. A variant added later falls on
the `false` side without anyone deciding it — the same silence `_ =>`
gives, and the shape that broke `sim boot` → `runner up` → `run` on
Android when a ledger row kind was added (C4's `is_service`).
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
GATE = os.path.join(ROOT, "scripts", "dev", "a-classification-names-every-variant.py")

ENUM = "pub enum Kind { A, B { x: u8 }, C }\npub enum Other { P, Q }\n"

MUST_FLAG = {
    "two variants of an own enum": "fn is_ab(k: &Kind) -> bool { matches!(k, Kind::A | Kind::B { .. }) }\n",
    "three variants, split over lines": (
        "fn f(k: &Kind) -> bool {\n    matches!(\n        k,\n        Kind::A\n            | Kind::C\n    )\n}\n"
    ),
    "Some() around own variants": "fn g(k: Option<&Kind>) -> bool { matches!(k, Some(Kind::A) | Some(Kind::C)) }\n",
}

MUST_PASS = {
    "a single variant": "fn is_a(k: &Kind) -> bool { matches!(k, Kind::A) }\n",
    "an enum defined elsewhere": "fn h(o: std::cmp::Ordering) -> bool { matches!(o, std::cmp::Ordering::Less | std::cmp::Ordering::Equal) }\n",
    "| inside one variant's field pattern": "fn i(k: &Kind) -> bool { matches!(k, Kind::B { x: 1 | 2 }) }\n",
    "an exhaustive match": "fn j(k: &Kind) -> bool { match k { Kind::A | Kind::B { .. } => true, Kind::C => false } }\n",
    "test code": "#[cfg(test)]\nmod t { use super::*; fn k(k: &Kind) -> bool { matches!(k, Kind::A | Kind::C) } }\n",
    "a comment": "// matches!(k, Kind::A | Kind::C)\n",
}

problems: list[str] = []


def verdict(body: str) -> tuple[int, str]:
    with tempfile.TemporaryDirectory() as t:
        src = os.path.join(t, "crates", "demo", "src")
        os.makedirs(src)
        with open(os.path.join(src, "lib.rs"), "w") as fh:
            fh.write(ENUM + body)
        r = subprocess.run([sys.executable, GATE, "--root", t], capture_output=True, text=True)
        return r.returncode, r.stdout + r.stderr


for name, body in MUST_FLAG.items():
    code, out = verdict(body)
    if code == 0:
        problems.append(f"{name}: not flagged\n{out}")
    elif "Traceback" in out:
        problems.append(f"{name}: red by crashing\n{out}")
    elif "lib.rs:" not in out:
        problems.append(f"{name}: red without naming the line\n{out}")

for name, body in MUST_PASS.items():
    code, out = verdict(body)
    if code != 0:
        problems.append(f"{name}: flagged, and it is not the shape\n{out}")

# A file that is itself a test module, declared `#[cfg(test)] mod x;` from
# its parent, carries no `#[cfg(test)]` of its own. It is test code.
with tempfile.TemporaryDirectory() as t:
    src = os.path.join(t, "crates", "demo", "src")
    os.makedirs(src)
    with open(os.path.join(src, "lib.rs"), "w") as fh:
        fh.write(ENUM + "#[cfg(test)]\nmod harness;\n")
    with open(os.path.join(src, "harness.rs"), "w") as fh:
        fh.write("use super::*;\nfn k(k: &Kind) -> bool { matches!(k, Kind::A | Kind::C) }\n")
    r = subprocess.run([sys.executable, GATE, "--root", t], capture_output=True, text=True)
    if r.returncode != 0:
        problems.append(f"a file declared as a test module was flagged:\n{r.stdout}")

with tempfile.TemporaryDirectory() as t:
    os.makedirs(os.path.join(t, "crates"))
    r = subprocess.run([sys.executable, GATE, "--root", t], capture_output=True, text=True)
    if r.returncode == 0:
        problems.append(f"a tree with no enums passed:\n{r.stdout}")

if problems:
    print("a-classification-names-every-variant.test: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)
print(
    f"a-classification-names-every-variant.test: {len(MUST_FLAG)} shapes flagged, "
    f"{len(MUST_PASS) + 1} near-misses passed, an empty tree refused"
)
