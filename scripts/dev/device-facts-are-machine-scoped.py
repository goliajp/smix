#!/usr/bin/env python3
"""A fact about a device is stored where the device is: on this machine.

A simulator is an operating-system object. Its UDID, its runtime
version, whether it is booted, who booted it, which port a runner holds
on it — none of that changes when you `cd` to another checkout. It used
to be stored per checkout, because the registry path walked up from the
working directory.

Measured 2026-08-11: four checkouts on this machine each kept their own
`.smix/`, and two of them held a lease. The leases were for the same
simulators. One of them read:

    {"deviceId": "FFC57DAE-…", "holder": {"pid": 50057, "cmd": "smix-mcp"},
     "resources": [{"kind":"booted","byUs":true},
                   {"kind":"runner","port":22087}]}

Every field of that is about this machine, and it lived inside a
checkout. The night this scan was written, a runner was found holding
port 22087 with no record of it — the rule says check the owner before
touching a runner, the check came back empty, so it could neither be
confirmed an orphan nor killed. It was on the books the whole time, in
another workspace's books. The rule was right; where the books lived is
what made it unenforceable.

So this scan does two things.

**Every path resolver declares its scope.** MACHINE means it resolves
under `~/.local/share/smix/`; CHECKOUT means the fact genuinely belongs
to one tree — a flow's attempts, a trace, a screenshot; DECIDES_NOTHING
is for the normalisers, which hand back a path a caller already chose
and so cannot claim a scope for it. A resolver in none of the three
fails. Silence is how a device fact ends up in a checkout, and this
cycle watched that shape cost something seven times.

**Every device-fact write goes to a MACHINE resolver.** Declaring the
scope is not the same as honouring it: three write sites here read
`registry_path()` and then fell back to `.smix/sims.json` in the working
directory if it failed, which put the record somewhere no other tree
would ever read.

The first version of this file listed two sets and then never
consulted them — it checked one function body for one substring. It
would have passed the moment that function was renamed, and it could not
see `registry.rs` at all. A gate whose criterion is not its own subject
is the thing the rest of this cycle kept finding.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# The files that decide where device facts are read and written.
SUBJECTS = [
    os.path.join(ROOT, "crates", "smix-cli", "src", "main.rs"),
    os.path.join(ROOT, "crates", "smix-cli", "src", "down.rs"),
    os.path.join(ROOT, "crates", "smix-simctl", "src", "registry.rs"),
]

# The test: does this fact still hold after `cd` to another checkout?
#
# Yes → MACHINE. The device is the same device; nothing about the tree
# you are standing in changes its UDID or who booted it.
MACHINE = {
    "registry_path": "UDID, alias, kind, runtime version, destructive opt-in",
    "machine_dir": "the machine-level root itself",
    "read_paths": "machine registry first, checkout only as a read-only fallback",
    "open_all": "the merged view every reader gets",
    "load_registry": "the CLI's handle on that merged view",
}

# Facts a tree genuinely owns. Listed rather than assumed.
CHECKOUT = {
    "flow_attempts_path": "what this code did when it ran; another tree's runs are not this one's",
    "trace_dir": "the artefacts of one run of one corpus",
    "cells_dir": "same",
    "discover": "finds the checkout's own `.smix`; a fallback source, never a write target",
}

# Neither, and saying so beats filing them under one.
#
# These hand back the path they were given, in a tidier form. A
# normaliser put in the CHECKOUT list would read as a claim that the
# path it returns belongs to a tree, which is not something it knows.
DECIDES_NOTHING = {
    "smix_dir": "normalises a path a caller already chose",
    "store_dir": "same, as the public name for it",
}

# Calls that write a device fact. Each must be handed a path from a
# MACHINE resolver.
WRITERS = (
    "SimRegistry::register(",
    "SimRegistry::allow_destructive(",
    "SimRegistry::unregister(",
    "SimRegistry::migrate(",
    "Self::register(",
)

# Inside `registry.rs` these take a `path` parameter from the caller —
# the crate cannot resolve it, and the CLI sites above are where the
# decision is made. Checked at the call sites instead.
WRITER_EXEMPT_FILES = {"registry.rs"}

problems: list[str] = []


def code_lines(path: str) -> list[tuple[int, str]]:
    """Numbered lines, comments and doc comments dropped.

    Not a nicety: this file's own docstring names `registry_path`, and
    four gates in this cycle were fooled by prose containing the thing
    they check for.
    """
    out = []
    for n, line in enumerate(open(path, encoding="utf-8"), 1):
        if line.lstrip().startswith("//"):
            continue
        out.append((n, line))
    return out


def body_of(lines: list[tuple[int, str]], start: int) -> str:
    """The braced body of the function whose signature starts at `start`."""
    text = "".join(l for n, l in lines if n >= start)
    i = text.find("{")
    if i < 0:
        return ""
    depth, j = 0, i
    while j < len(text):
        if text[j] == "{":
            depth += 1
        elif text[j] == "}":
            depth -= 1
            if depth == 0:
                return text[i : j + 1]
        j += 1
    return text[i:]


missing = [p for p in SUBJECTS if not os.path.isfile(p)]
if missing:
    print("device-facts-are-machine-scoped: FAIL")
    for p in missing:
        print(f"  - no {os.path.relpath(p, ROOT)}")
    sys.exit(1)

# Every function whose name says it produces a place to put something.
# The three lists are matched against these, and anything they do not
# cover fails.
RESOLVER = re.compile(
    r"\bfn\s+(\w*(?:path|paths|dir|open_all|load_registry|discover)\w*)\s*[(<]",
    re.I,
)

declared = 0
for subject in SUBJECTS:
    rel = os.path.relpath(subject, ROOT)
    lines = code_lines(subject)
    for n, line in lines:
        m = RESOLVER.search(line)
        if not m:
            continue
        name = m.group(1)
        lists = [
            label
            for label, table in (
                ("MACHINE", MACHINE),
                ("CHECKOUT", CHECKOUT),
                ("DECIDES_NOTHING", DECIDES_NOTHING),
            )
            if name in table
        ]
        in_machine = "MACHINE" in lists
        if len(lists) > 1:
            problems.append(
                f"{rel}:{n} `{name}` is in {' and '.join(lists)} — pick one"
            )
            continue
        if not lists:
            problems.append(
                f"{rel}:{n} `{name}` resolves a place to put something and is "
                f"in none of the three lists. Add it to MACHINE (the fact "
                f"survives a `cd`), to CHECKOUT with the reason it does not, or "
                f"to DECIDES_NOTHING if it only reshapes a path it was handed. "
                f"Not being listed is how a device fact ends up in a tree."
            )
            continue
        declared += 1
        if not in_machine:
            continue
        body = body_of(lines, n)
        # Reading the working directory is fine when it is handed to a
        # machine-first resolver — that is how the checkout gets its turn
        # as a read-only fallback. What is not fine is deciding from it.
        delegates = any(f"{other}(" in body for other in MACHINE if other != name)
        if "current_dir" in body and not delegates:
            problems.append(
                f"{rel}:{n} `{name}` is declared machine-scoped and resolves from "
                f"the working directory. A simulator's UDID, its runtime version "
                f"and who booted it do not change when you `cd` — storing them per "
                f"checkout is why four workspaces on this machine each held their "
                f"own answer, and why a runner on port 22087 could be on the books "
                f"and invisible at the same time."
            )

# A write that reaches for the checkout when the machine path will not
# resolve has undone the whole thing, quietly.
writes = 0
for subject in SUBJECTS:
    rel = os.path.relpath(subject, ROOT)
    if os.path.basename(subject) in WRITER_EXEMPT_FILES:
        continue
    lines = code_lines(subject)
    joined = "".join(l for _, l in lines)
    for n, line in lines:
        if not any(w in line for w in WRITERS):
            continue
        writes += 1
    for n, line in lines:
        if "registry_path()" not in line:
            continue
        if "unwrap_or_else" in line or ".unwrap_or(" in line:
            problems.append(
                f"{rel}:{n} falls back to a path of its own when the machine "
                f"registry will not resolve. There is no good place to put a "
                f"device record silently — a record in a tree is a record the "
                f"next tree cannot see."
            )
    if '".smix/sims.json"' in joined:
        for n, line in lines:
            if '".smix/sims.json"' in line:
                problems.append(
                    f"{rel}:{n} names a checkout-relative registry path in code. "
                    f"Device records live on the machine; the checkout is a "
                    f"read-only fallback reached through `discover`."
                )

if problems:
    print("device-facts-are-machine-scoped: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

if declared == 0 or writes == 0:
    print("device-facts-are-machine-scoped: FAIL")
    print(
        f"  - parsed {declared} resolver(s) and {writes} write(s) — the shape "
        f"changed and this scan is reading air"
    )
    sys.exit(1)

print(
    f"device-facts-are-machine-scoped: clean — {declared} resolver(s) declared "
    f"({len(MACHINE)} machine-scoped, {len(CHECKOUT)} a checkout owns, "
    f"{len(DECIDES_NOTHING)} deciding nothing), "
    f"{writes} device-fact write(s), none falling back to a tree"
)
