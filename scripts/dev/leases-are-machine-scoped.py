#!/usr/bin/env python3
"""One machine root, and the lease ledgers live under it.

A lease says who holds a device, what they opened on it, and on which
port. Every field of that is about this machine. They were written into
whichever `.smix/` was above the working directory, so a machine with
four checkouts had four sets of books about the same simulators — and
on 2026-08-11 a runner holding port 22087 was simultaneously on the
books and invisible, because the books belonged to another workspace.

Two things are checked, and both are lists that participate.

**The machine root has one resolver.** Four separate functions worked
out `$XDG_DATA_HOME/smix` or `~/.local/share/smix` for themselves, and
three more reached for `~/.local/share/smix/<thing>` through
`dirs::home_dir()` — which ignores `XDG_DATA_HOME` entirely, so with
that variable set the runner tree and the press-timing table landed in
different places. A fifth copy is one refactor away from a fifth answer.
Any code line naming those roots outside `MACHINE_ROOT_RESOLVERS` fails.

**Every ledger call is handed the machine lease directory** — and that
half is the compiler's now, not this scan's. The store's functions took
a workspace root and appended `.smix/leases` themselves, so when the
first argument changed meaning, twenty-five call sites went on
compiling and went on writing device facts into trees. `LeaseDir` is a
type for that reason: a workspace root no longer fits. Re-checking it
textually here would only add a second, weaker opinion about something
already settled — and the first draft of this scan did exactly that,
producing four false positives on argument lists it could not parse.

So this checks the part the compiler cannot see: that nobody works out
where the machine's data lives for a second time. `LeaseDir::machine()`
is one call away from `machine_root()`; a private copy of the
environment lookup is not.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CRATES = os.path.join(ROOT, "crates")

# The only functions allowed to work out where this machine keeps smix's
# data. Everything else asks them.
MACHINE_ROOT_RESOLVERS = {
    "machine_root": "crates/smix-lease — the one resolver; everything below asks it",
}

problems: list[str] = []


def rust_files() -> list[str]:
    out = []
    for base, dirs, names in os.walk(CRATES):
        dirs[:] = [d for d in dirs if d != "target"]
        for n in names:
            if n.endswith(".rs"):
                out.append(os.path.join(base, n))
    return sorted(out)


def code_lines(path: str) -> list[tuple[int, str]]:
    """Numbered lines, comments dropped.

    This file's own docstring names both roots, and four gates in this
    cycle were fooled by prose containing the thing they check for.
    """
    out = []
    for n, line in enumerate(open(path, encoding="utf-8"), 1):
        if line.lstrip().startswith("//") or line.lstrip().startswith("#"):
            continue
        out.append((n, line))
    return out


def enclosing_fn(lines: list[tuple[int, str]], target: int) -> str:
    """Name of the function `target` sits inside."""
    name = ""
    for n, line in lines:
        if n > target:
            break
        m = re.search(r"\bfn\s+(\w+)", line)
        if m:
            name = m.group(1)
    return name


# Resolving, not mentioning. An error message that names
# `~/.local/share/smix/runner/` is telling somebody where to look; a
# `join(".local/share")` is deciding where that is. Only the second one
# may have a second copy.
ROOT_LITERAL = re.compile(
    r'var_os\(\s*"XDG_DATA_HOME"|var\(\s*"XDG_DATA_HOME"'
    r'|join\(\s*"\.local/share|from\(\s*"\.local/share'
    r'|home_dir\(\)'
)

resolvers_seen = 0

for path in rust_files():
    rel = os.path.relpath(path, ROOT)
    lines = code_lines(path)

    for n, line in lines:
        if ROOT_LITERAL.search(line):
            fn = enclosing_fn(lines, n)
            if fn in MACHINE_ROOT_RESOLVERS:
                resolvers_seen += 1
                continue
            problems.append(
                f"{rel}:{n} `{fn or '<top level>'}` works out where this "
                f"machine keeps smix's data. There is one resolver for that "
                f"and it is `smix_lease::machine_root()` — a second copy is a "
                f"second answer, and the copies that reach through "
                f"`dirs::home_dir()` do not honour XDG_DATA_HOME at all."
            )

if problems:
    print("leases-are-machine-scoped: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

if resolvers_seen == 0:
    print("leases-are-machine-scoped: FAIL")
    print(
        f"  - parsed {resolvers_seen} machine-root line(s) — the shape changed "
        f"and this scan is reading air"
    )
    sys.exit(1)

print(
    f"leases-are-machine-scoped: clean — {resolvers_seen} machine-root line(s), "
    f"all inside {len(MACHINE_ROOT_RESOLVERS)} resolver(s); which directory a "
    f"ledger call writes to is `LeaseDir`'s job, and the compiler's"
)
