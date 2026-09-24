#!/usr/bin/env python3
"""A class drawn over one of our own enums names every variant.

    fn is_service(&self) -> bool { matches!(self, Resource::Runner { .. } | Resource::Recording { .. }) }

When a variant is added, this answers `false` for it without anyone having
decided so. That is the silence `_ =>` gives, and `type/no-catchall-match`
forbids `_ =>` on our own enums for exactly that reason; `matches!` with an
alternation is the same thing in a different spelling. It broke Android's
most ordinary path once: a new ledger row kind was filed as "not a
service", and `run` was refused naming a recording that was not there.

Flagged: in production code (above `#[cfg(test)]`), a `matches!` whose
pattern has a top-level `|` between two or more variants of an enum this
repository defines. The fix is an exhaustive `match`, one arm per variant,
so the next variant is a compile error at the place the class is drawn.

Not flagged: one variant (an identity question stays right when variants
are added), `|` inside a single variant's field pattern, enums from other
crates, test code, comments.
"""

from __future__ import annotations

import argparse
import os
import re
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
ENUM_DEF = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?enum\s+(\w+)", re.M)


def sources(root: str) -> list[str]:
    out = []
    for d, _, files in os.walk(os.path.join(root, "crates")):
        if f"{os.sep}src" not in d and not d.endswith("src"):
            continue
        out += [os.path.join(d, f) for f in files if f.endswith(".rs")]
    return sorted(out)


def strip_comments(src: str) -> str:
    src = re.sub(r"/\*.*?\*/", lambda m: re.sub(r"[^\n]", " ", m.group(0)), src, flags=re.S)
    return re.sub(r"//[^\n]*", "", src)


def balanced(src: str, open_at: int) -> int:
    """Index just past the `)` closing the `(` at `open_at`."""
    depth, i, sq, dq = 0, open_at, False, False
    while i < len(src):
        c = src[i]
        if dq:
            if c == "\\":
                i += 2
                continue
            if c == '"':
                dq = False
        elif c == '"':
            dq = True
        elif c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return i


def top_level_split(pat: str, sep: str) -> list[str]:
    parts, cur, depth = [], "", 0
    for c in pat:
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        if c == sep and depth == 0:
            parts.append(cur)
            cur = ""
            continue
        cur += c
    parts.append(cur)
    return parts


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=REPO)
    root = os.path.abspath(ap.parse_args().root)
    files = sources(root)
    own: set[str] = set()
    for f in files:
        own |= set(ENUM_DEF.findall(open(f, encoding="utf-8").read()))
    if not own:
        print("a-classification-names-every-variant: FAIL")
        print("  - no enum defined under crates/*/src — the scan read nothing, which is not clean")
        return 1

    # A file that is a test module declared from its parent carries no
    # `#[cfg(test)]` of its own, so it is found from the declaration.
    test_modules: set[str] = set()
    for f in files:
        for name in re.findall(r"#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;",
                               open(f, encoding="utf-8").read()):
            d = os.path.dirname(f)
            test_modules |= {os.path.join(d, f"{name}.rs"), os.path.join(d, name, "mod.rs")}

    problems: list[str] = []
    for f in files:
        if f in test_modules:
            continue
        prod = strip_comments(open(f, encoding="utf-8").read()).split("#[cfg(test)]", 1)[0]
        for m in re.finditer(r"\bmatches!\s*\(", prod):
            end = balanced(prod, m.end() - 1)
            body = prod[m.end() : end - 1]
            args = top_level_split(body, ",")
            if len(args) < 2:
                continue
            pattern = ",".join(args[1:]).split(" if ", 1)[0]
            alternatives = [a for a in top_level_split(pattern, "|") if a.strip()]
            if len(alternatives) < 2:
                continue
            named = [re.search(r"\b([A-Z]\w*)::[A-Z]\w*", a) for a in alternatives]
            enums = {n.group(1) for n in named if n}
            if len(enums) == 1 and all(named) and enums <= own:
                line = prod[: m.start()].count("\n") + 1
                (enum,) = enums
                problems.append(
                    f"{os.path.relpath(f, root)}:{line}: `matches!` draws a class over "
                    f"{len(alternatives)} variants of `{enum}` — a variant added later lands "
                    f"on the `false` side undecided; write an exhaustive `match`"
                )
    if problems:
        print(f"a-classification-names-every-variant: FAIL — {len(problems)} class(es) that a new variant would join silently")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(
        f"a-classification-names-every-variant: clean — {len(files)} files, "
        f"{len(own)} own enums, no class drawn with `matches!` over several of their variants"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
