#!/usr/bin/env python3
"""Every `health_ok` call site says whether it is making a decision.

`/health` answers whether the runner's HTTP server is answering. It is a
handler closed over a boot date: it never touches the app, the
`XCUIApplication` or a session table, so it says 200 for as long as the
socket is being read. A runner whose app was reinstalled underneath it
answers `/health` 200 and `/tree` 500 in the same second.

Two places read the first of those and decided from it. `smix runner up`
printed "runner already up" and returned success; `smix_use` reported it
was already driving. A consumer hit that three times in one day, and the
shape is what makes it worth a gate rather than two fixes: the only
command that could recover the runner was the one refusing to, because
the question it asked was not the question it needed answered.

So every call site is in one of two lists. A `health-decider` concludes
something from the answer, and the block it guards must also ask whether
the session works. A `health-not-a-decider` reports it, waits on it, or
watches it go away — those are fine, and naming them is what keeps the
first list honest. A site in neither list fails this scan, because "not
listed" must never be the way something becomes exempt.

Declare it on the comment lines immediately above the call:

    // health-decider: <what it concludes>
    // health-decider: <what it concludes> — deferred: <why it cannot ask yet>
    // health-not-a-decider: <what it does with the answer>

Adjacency rather than a `file:function` list, which was the first
design: `up_on` and the Android `up` each read `/health` twice, once to
short-circuit and once to wait, and those two want opposite answers. A
list keyed by function cannot say that, and would have let the
short-circuit inherit the wait's probe.
"""

import os
import re
import sys

ROOT = (
    sys.argv[1]
    if len(sys.argv) > 1
    else os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
)
CRATES = os.path.join(ROOT, "crates")

# The questions that do answer "can this be driven". Named rather than
# discovered: iOS asks the tree, Android asks the window list, and a
# scan that accepted any nearby call would accept `println!`.
#
# Matched as a prefix, so `probe_session_for(...)` counts as asking. It
# did not, once: the check built `f"{probe}("` and the named variant
# stopped matching the moment it was introduced. C6's plan had recorded
# the opposite as checked — it had read this tuple and not the line that
# uses it.
PROBES = ("probe_session", "automation_sees_an_app")
PROBE_CALL = re.compile(r"\b(?:" + "|".join(PROBES) + r")\w*\s*\(")
# Where the iOS one lives. A renamed probe has to make this go red, not
# quiet.
PROBE_HOME = os.path.join(CRATES, "smix-capsule", "src", "runner.rs")

# Below either of these the scan is describing a codebase that is not
# this one, and a clean verdict would mean nothing.
MIN_SITES = 8
MIN_DECIDERS = 2

# `\b` does not match inside `record_health_ok(`, which is a different
# thing in a different crate; `::health_ok(` and ` health_ok(` do.
CALL = re.compile(r"\bhealth_ok\s*\(")
DEFINITION = re.compile(r"\bfn\s+health_ok\s*\(")
DECLARATION = re.compile(r"//\s*health-(?P<kind>decider|not-a-decider):(?P<why>.*)")
# Doc comments go first: `///` prose about a call site is prose, and
# prose that reads as a declaration would let a sentence stand in for
# the thing it describes.
DOC = re.compile(r"^\s*//[/!]")
COMMENT_OR_BLANK = re.compile(r"^\s*(//.*)?$")


def rust_files(root):
    for base, dirs, names in os.walk(root):
        dirs[:] = [d for d in dirs if d not in ("target", ".git")]
        for name in sorted(names):
            if name.endswith(".rs"):
                yield os.path.join(base, name)


def read(path):
    with open(path, encoding="utf-8") as fh:
        return ["" if DOC.match(line) else line for line in fh.read().splitlines()]


def declaration_above(lines, index):
    """The declaration in the run of comments directly above `index`.

    A declaration runs to the end of that comment block, not to the end
    of its own line. The Android deferral's reason is four lines long,
    and a one-line reader called it empty — which would have made the
    only way to record a reason "write a shorter one".
    """
    start = index
    while start > 0 and COMMENT_OR_BLANK.match(lines[start - 1]):
        start -= 1
    for i in range(start, index):
        found = DECLARATION.search(lines[i])
        if not found:
            continue
        rest = [found.group("why")] + [
            re.sub(r"^\s*//", "", line) for line in lines[i + 1 : index]
        ]
        return found.group("kind"), " ".join(rest).strip(" —-")
    return None


def guarded_block(lines, index):
    """The text of the block this call gates, by brace matching."""
    depth = 0
    started = False
    out = []
    for line in lines[index:]:
        out.append(line)
        for ch in line:
            if ch == "{":
                depth += 1
                started = True
            elif ch == "}":
                depth -= 1
        if started and depth <= 0:
            break
    return "\n".join(out)


problems = []
sites = []
probe_users = set()

if not os.path.isdir(CRATES):
    print("health-is-not-a-session-check: FAIL")
    print(f"  - no crates/ under {ROOT} — this scan has nothing to read")
    sys.exit(1)

for path in rust_files(CRATES):
    lines = read(path)
    rel = os.path.relpath(path, CRATES)
    text = "\n".join(lines)
    if PROBE_CALL.search(text):
        probe_users.add(rel)
    for number, line in enumerate(lines):
        if CALL.search(line) and not DEFINITION.search(line):
            sites.append((rel, number + 1, declaration_above(lines, number), guarded_block(lines, number)))

# --- the scan is looking at something ------------------------------------

if len(sites) < MIN_SITES:
    problems.append(
        f"only {len(sites)} health_ok call site(s) under {CRATES} — expected at "
        f"least {MIN_SITES}. Either the function was renamed and this scan is "
        "reading air, or the floor needs lowering on purpose"
    )

home = "\n".join(read(PROBE_HOME)) if os.path.isfile(PROBE_HOME) else ""
if "fn probe_session" not in home:
    problems.append(
        f"probe_session is not defined in {os.path.relpath(PROBE_HOME, CRATES)}. "
        "Every rule below is phrased in terms of asking the session, so without "
        "it this scan goes quiet rather than red — which is the failure it "
        "exists to prevent"
    )
if len(probe_users) < 2:
    problems.append(
        f"the session probes are called from {len(probe_users)} file(s). They "
        "were written because two separate places concluded a device was "
        "drivable from /health alone; one caller means one of them went back"
    )

# --- every site is claimed, and every decider asks ------------------------

deciders = deferred = non_deciders = 0
for rel, number, declaration, block in sites:
    if declaration is None:
        problems.append(
            f"{rel}:{number} reads /health and says nothing about what it does "
            "with the answer. Put one of these on the line above it:\n"
            "      // health-decider: <what it concludes>\n"
            "      // health-not-a-decider: <what it does with the answer>\n"
            "    Not listed must never be the way something becomes exempt"
        )
        continue
    kind, why = declaration
    if not why:
        problems.append(
            f"{rel}:{number} is declared with nothing after the colon — the "
            "declaration is the reason, not the label"
        )
        continue
    if kind == "not-a-decider":
        non_deciders += 1
        continue
    deciders += 1
    if "deferred:" in why:
        if not why.split("deferred:", 1)[1].strip():
            problems.append(
                f"{rel}:{number} defers asking the session and does not say why. "
                "A deferral with no reason cannot be told from an oversight"
            )
        else:
            deferred += 1
        continue
    if not PROBE_CALL.search(block):
        problems.append(
            f"{rel}:{number} concludes from /health and the block it guards "
            f"never asks {' or '.join(PROBES)}. /health says the server is "
            "answering; it cannot see whether the session behind it still "
            "works, and concluding from it is how `runner up` came to refuse "
            "the only command that could have recovered the runner"
        )

if deciders < MIN_DECIDERS:
    problems.append(
        f"only {deciders} declared decider(s) — expected at least "
        f"{MIN_DECIDERS}. If one of them stopped concluding anything, say so "
        "here rather than letting the count drop quietly"
    )

if problems:
    print("health-is-not-a-session-check: FAIL")
    for problem in problems:
        print(f"  - {problem}")
    sys.exit(1)

print(
    f"health-is-not-a-session-check: clean — {len(sites)} call sites, "
    f"{deciders} deciders of which {deciders - deferred} ask the session, "
    f"{deferred} deferred with a reason, {non_deciders} non-deciders"
)
