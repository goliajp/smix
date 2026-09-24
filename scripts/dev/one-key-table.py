#!/usr/bin/env python3
"""A key's name is read in one place: `KeyName::from_name`.

Three hand copies of the key table had drifted apart — the CLI read
`lock`, MCP did not; a flow read `volume up`, the CLI only `volume-up`;
none read `back` — and a consumer looking for back found it in none of
the places they looked. The copies were one table written three times;
the fourth reader, the Node and UniFFI bindings, parsed the wire enum
with serde and took yet another set of spellings.

This finds, in non-test Rust outside `crates/smix-input/src/`, either
shape a second reader takes:

* a string literal mapped to a key: a line starting with `"…"` that has
  `=>` and names `KeyName::` (a `match` arm), or a `("…", KeyName::…)`
  pair (a lookup table);
* a key parsed as a serde wire enum: `KeyName = parse_wire_enum`.

It also requires the table itself — at least 20 `("…", KeyName::…)` rows
in `crates/smix-input/src/lib.rs` — so a walk that reads nothing, or a
table that moved, is red rather than clean.

Usage:
  scripts/dev/one-key-table.py [--root DIR]
"""

from __future__ import annotations

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if len(sys.argv) == 3 and sys.argv[1] == "--root":
    ROOT = sys.argv[2]

TABLE = os.path.join("crates", "smix-input", "src", "lib.rs")
ARM = re.compile(r'^\s*"[^"]*"[^\n]*=>[^\n]*\bKeyName::')
PAIR = re.compile(r'^\s*\("[^"]*",\s*KeyName::')
WIRE = re.compile(r"\bKeyName\s*=\s*parse_wire_enum\b")


def production_lines(text: str) -> list[tuple[int, str]]:
    """Lines outside `#[cfg(test)]` modules and comments."""
    out: list[tuple[int, str]] = []
    skip_depth: int | None = None
    depth = 0
    pending_test = False
    for n, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        if stripped.startswith("#[cfg(test)]"):
            pending_test = True
            continue
        if skip_depth is None and pending_test and "{" in stripped:
            skip_depth = depth
        opens, closes = line.count("{"), line.count("}")
        in_test = skip_depth is not None
        depth += opens - closes
        if skip_depth is not None and depth <= skip_depth:
            skip_depth = None
        if pending_test and (opens or stripped.endswith(";")):
            pending_test = False
        if in_test or stripped.startswith("//"):
            continue
        out.append((n, line))
    return out


def main() -> int:
    crates = os.path.join(ROOT, "crates")
    offenders: list[str] = []
    rows = 0
    files = 0
    for dirpath, dirnames, filenames in os.walk(crates):
        dirnames[:] = [d for d in dirnames if d not in ("target", "tests", "benches", "examples")]
        for name in filenames:
            if not name.endswith(".rs"):
                continue
            path = os.path.join(dirpath, name)
            rel = os.path.relpath(path, ROOT)
            files += 1
            with open(path, encoding="utf-8") as fh:
                lines = production_lines(fh.read())
            if rel == TABLE:
                rows += sum(1 for _, l in lines if PAIR.match(l))
                continue
            for n, line in lines:
                if ARM.match(line) or PAIR.match(line) or WIRE.search(line):
                    offenders.append(f"{rel}:{n}: {line.strip()}")
    problems: list[str] = []
    if files == 0:
        problems.append(f"no Rust source under {crates} — the walk read nothing")
    if rows < 20:
        problems.append(
            f"{TABLE} holds {rows} name→key rows, not the table (≥ 20) this "
            "gate protects — it moved, or the walk missed it"
        )
    if offenders:
        problems.append(
            "a key name is read outside KeyName::from_name:\n    "
            + "\n    ".join(offenders)
        )
    if problems:
        print("one-key-table: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(f"one-key-table: clean — {rows} names in the one table, {files} files read")
    return 0


if __name__ == "__main__":
    sys.exit(main())
