#!/usr/bin/env python3
"""A checkout's ledgers are read. They are never written, and they never
decide.

The ledgers moved to `~/.local/share/smix/leases` on 2026-08-11. What
did not move is the copy of smix already installed on this machine:
`~/.local/bin/smix` is 3.0.0 and still writes into whichever `.smix/`
sits above the working directory. Ninety-one minutes after the
migration, one checkout's `.smix/leases/FFC57DAE-….json` had been
rewritten — recording a runner at pid 28529 that was alive and serving
on port 22087, while the machine ledger for the same device still named
a pid that had exited and read `abandoned — 2 close(s) owed`. A
`lease reconcile` from any tree, in that window, would have shut the
simulator down and killed the runner. The window closed when that
session ended; it was not hypothetical while it was open.

So the two directories are different things and the code has to say
which one it means:

* `LeaseDir` is where this machine keeps its ledgers. It is the only
  thing the writing half of `store` accepts.
* `CheckoutLedgers` is a tree's old book. It can be read and nothing
  else — a divergence found there stops a decision; it never makes one.

That half is the compiler's. This scan checks the half it cannot see:
**who is allowed to name either directory in the first place.** Two
lists, both consulted, both failing in either direction — a caller that
appears without being listed, and a listed caller that has quietly
stopped existing, are the same defect seen from two sides.

The sibling scan `leases-are-machine-scoped.py` records why the other
half is not here: when the first argument of every ledger function
changed meaning, twenty-five call sites went on compiling and went on
writing device facts into trees. A type fixed that. A second textual
opinion about it would only be weaker.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CRATES = os.path.join(ROOT, "crates")

# Who may ask where this machine keeps its ledgers.
#
# Not a long list on purpose: every extra caller is another place that
# could be handed the wrong directory. `smix-cli` reaches it through
# `smix_capsule::runner::machine_leases()`, which is why it is not here.
MACHINE_LEDGER_CALLERS = {
    "crates/smix-capsule/src/runner.rs": "the one helper the CLI and the capsule both call",
    "crates/smix-mcp/src/main.rs": "the server takes a lease per session; it has no capsule",
}

# Who may name a tree's old book.
#
# One caller, and it reads: `lease migrate` opens the source side. If a
# second appears, the question to ask is whether it is about to make a
# decision from what a 3.0.0 binary wrote.
CHECKOUT_LEDGER_READERS = {
    "crates/smix-cli/src/lease_cmd.rs": "`lease migrate` and `lease list` read the source side",
    # Added deliberately: `runner list` reads a tree's book so it can say
    # which checkout has a record of a runner this machine's ledgers do
    # not — the question that could not be answered about port 22087.
    # It never classifies a row from it; the tree's path is printed
    # alongside, labelled as evidence. This list fails in both
    # directions, so an entry is an act rather than a relaxation.
    "crates/smix-cli/src/runner_list.rs": "`runner list` names the tree that knows a runner",
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


def shipping_code(path: str) -> list[tuple[int, str]]:
    """Numbered lines, minus comments and `#[cfg(test)]` blocks.

    A test hands a tempdir to `LeaseDir::at` on purpose — a throwaway
    directory *is* a whole machine for the length of one test, and
    saying so is how the tests stopped reading this machine's real
    state. What must not happen is a shipping code path doing it, so
    the test blocks come out by brace matching rather than by filename:
    `leased.rs` holds twenty of them inside `#[cfg(test)] mod tests`,
    and a path-based rule would have called that file clean while
    telling us nothing about the twenty-first.
    """
    out: list[tuple[int, str]] = []
    depth = 0
    in_test = False
    test_depth = 0
    pending_cfg_test = False
    for n, line in enumerate(open(path, encoding="utf-8"), 1):
        stripped = line.lstrip()
        opens, closes = line.count("{"), line.count("}")
        if in_test:
            depth += opens - closes
            if depth <= test_depth:
                in_test = False
            continue
        if stripped.startswith("//"):
            depth += 0
            continue
        if re.match(r"#\[cfg\(test\)\]", stripped) or re.match(
            r"#\[cfg\(all\(test", stripped
        ):
            pending_cfg_test = True
            depth += opens - closes
            continue
        if pending_cfg_test and "{" in line:
            pending_cfg_test = False
            in_test = True
            test_depth = depth
            depth += opens - closes
            continue
        if pending_cfg_test and stripped and not stripped.startswith("#"):
            # An attribute on something that is not a block — a single
            # test fn with no body on this line. Keep looking.
            pass
        out.append((n, line))
        depth += opens - closes
    return out


MACHINE_CALL = re.compile(r"LeaseDir::machine\(")
CHECKOUT_NAME = re.compile(r"CheckoutLedgers::|checkout_lease_dir\b")
ESCAPE_HATCH = re.compile(r"LeaseDir::at\(")

machine_found: set[str] = set()
checkout_found: set[str] = set()
hatch_sites: list[str] = []

for path in rust_files():
    rel = os.path.relpath(path, ROOT).replace(os.sep, "/")
    if "/tests/" in rel:
        continue
    lines = shipping_code(path)
    for n, line in lines:
        if MACHINE_CALL.search(line):
            machine_found.add(rel)
        if CHECKOUT_NAME.search(line):
            checkout_found.add(rel)
        if ESCAPE_HATCH.search(line):
            hatch_sites.append(f"{rel}:{n}")

for extra in sorted(machine_found - set(MACHINE_LEDGER_CALLERS)):
    problems.append(
        f"{extra} asks where this machine keeps its ledgers and is not listed. "
        f"Add it with the reason it needs its own answer, or route it through "
        f"`smix_capsule::runner::machine_leases()` like the CLI does."
    )
for missing in sorted(set(MACHINE_LEDGER_CALLERS) - machine_found):
    problems.append(
        f"{missing} is listed as asking for the machine ledger directory and no "
        f"longer does. Either it stopped needing one — take it off the list — or "
        f"it now gets one from somewhere this scan cannot see."
    )

for extra in sorted(checkout_found - set(CHECKOUT_LEDGER_READERS)):
    problems.append(
        f"{extra} names a checkout's old ledger book and is not listed. A tree's "
        f"book is written by whatever smix that tree last ran — 3.0.0 was still "
        f"writing to one ninety-one minutes after these moved — so reading it is "
        f"a decision somebody has to have made on purpose."
    )
for missing in sorted(set(CHECKOUT_LEDGER_READERS) - checkout_found):
    problems.append(
        f"{missing} is listed as reading a checkout's book and no longer does. "
        f"If migration stopped reading the source side, it stopped migrating."
    )

for site in hatch_sites:
    problems.append(
        f"{site} builds a `LeaseDir` from a path in shipping code. That "
        f"constructor exists for tests, where a tempdir stands in for a whole "
        f"machine. In shipping code it is a way to point the writing half of "
        f"`store` at a directory nobody agreed on."
    )

if problems:
    print("no-second-ledger-path: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

# Both lists empty would agree with any codebase at all.
if not machine_found or not checkout_found:
    print("no-second-ledger-path: FAIL")
    print(
        f"  - found {len(machine_found)} machine-ledger caller(s) and "
        f"{len(checkout_found)} checkout reader(s) — the shape changed and this "
        f"scan is reading air"
    )
    sys.exit(1)

print(
    f"no-second-ledger-path: clean — {len(machine_found)} file(s) ask for this "
    f"machine's ledgers, {len(checkout_found)} read a tree's old book, and no "
    f"shipping code builds a ledger directory of its own"
)
