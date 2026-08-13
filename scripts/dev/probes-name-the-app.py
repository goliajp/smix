#!/usr/bin/env python3
"""Every session probe says which app it is asking about.

C5 made "can this be driven" a question that can be asked. C6 found the
next half of it: asking without naming an app answers about whichever app
the runner happened to be bound to at startup.

Where smix spawned the runner itself those are the same app, so the gap
does not bite today. It bites the first time something probes a runner it
did not start — after a fallback that re-attached, or an `smix_use`
against a session somebody else brought up. The shape it takes then is
the expensive one: the probe says usable, and what is drivable is a
different app than the caller asked about.

So each call site either names an app — `probe_session_for(port, x)` with
`x` something other than the literal `None` — or carries, on the line
above it, a reason it cannot:

    // unnamed-probe: <why this one has no app to name>

Neither is not an option, because silence must never be how a call site
becomes exempt.
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

# Where the two halves of C6 live. Named rather than discovered: a scan
# that finds its subject wherever it happens to be cannot tell "renamed"
# from "gone".
HOME = os.path.join(CRATES, "smix-capsule", "src", "runner.rs")
REQUIRED = ("probe_session_for", "decide_after_timeout")

# Below either of these the scan is describing a codebase that is not this
# one. The floors are small because this axis has few call sites — their
# job is to refuse a cleared-out tree, not to describe a size.
MIN_SITES = 3
MIN_NAMED = 1

CALL = re.compile(r"(?<![A-Za-z0-9_])probe_session(?P<named>_for)?\s*\(")
DEFINITION = re.compile(r"\bfn\s+probe_session(_for)?\s*\(")
DECLARATION = re.compile(r"//\s*unnamed-probe:(?P<why>.*)")
DOC = re.compile(r"^\s*//[/!]")
COMMENT_OR_BLANK = re.compile(r"^\s*(//.*)?$")


def rust_files(root):
    """Product code only, and that is on purpose.

    A test that calls the unnamed face is exercising an API rather than
    deciding anything at runtime, and requiring a reason on each one buys
    a comment nobody reads. Stated here rather than left to be noticed:
    the rule is about call sites that make decisions about a device.
    """
    for base, dirs, names in os.walk(root):
        dirs[:] = [d for d in dirs if d not in ("target", ".git", "tests", "benches")]
        for name in sorted(names):
            if name.endswith(".rs"):
                yield os.path.join(base, name)


def read(path):
    with open(path, encoding="utf-8") as fh:
        return ["" if DOC.match(line) else line for line in fh.read().splitlines()]


def reason_above(lines, index):
    """The declaration in the run of comments directly above `index`."""
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
        return " ".join(rest).strip(" —-")
    return None


def second_argument(text, at):
    """The text between the call's first comma and its closing paren.

    Reads from the call onward rather than to the end of the line: a
    formatter splits a long call across lines, and a one-line reader
    called such a call unnamed — which would have made "wrap it" the way
    to become exempt.
    """
    i = text.index("(", at)
    depth = 0
    args = ""
    for ch in text[i:]:
        if ch == "(":
            depth += 1
            if depth == 1:
                continue
        elif ch == ")":
            depth -= 1
            if depth == 0:
                break
        args += ch
    parts = args.split(",")
    return parts[1].strip() if len(parts) > 1 else ""


problems = []
sites = []

if not os.path.isdir(CRATES):
    print("probes-name-the-app: FAIL")
    print(f"  - no crates/ under {ROOT} — this scan has nothing to read")
    sys.exit(1)

for path in rust_files(CRATES):
    lines = read(path)
    rel = os.path.relpath(path, CRATES)
    for number, line in enumerate(lines):
        found = CALL.search(line)
        if not found or DEFINITION.search(line):
            continue
        rest = "\n".join(lines[number:])
        at = found.start()
        named = bool(found.group("named")) and second_argument(rest, at) not in ("", "None")
        sites.append((rel, number + 1, named, reason_above(lines, number)))

# --- the scan is looking at something ------------------------------------

if len(sites) < MIN_SITES:
    problems.append(
        f"only {len(sites)} session-probe call site(s) under {CRATES} — expected "
        f"at least {MIN_SITES}. Either the probe was renamed and this scan is "
        "reading air, or the floor needs lowering on purpose"
    )

home = "\n".join(read(HOME)) if os.path.isfile(HOME) else ""
for symbol in REQUIRED:
    if f"fn {symbol}" not in home:
        problems.append(
            f"{symbol} is not defined in {os.path.relpath(HOME, CRATES)}. The "
            "rules below are phrased in terms of it, so without it this scan "
            "goes quiet rather than red — which is the failure it exists to "
            "prevent"
        )

# --- every probe names an app, or says why it cannot ----------------------

named_count = 0
excused = 0
for rel, number, named, reason in sites:
    if named:
        named_count += 1
        continue
    if reason is None:
        problems.append(
            f"{rel}:{number} asks whether a session works without saying which "
            "app it means, and does not say why. Either name it —\n"
            "      probe_session_for(port, bundle)\n"
            "    or put the reason on the line above:\n"
            "      // unnamed-probe: <why this one has no app to name>\n"
            "    Silence must never be how a call site becomes exempt"
        )
        continue
    if not reason:
        problems.append(
            f"{rel}:{number} is declared unnamed with nothing after the colon — "
            "the declaration is the reason, not the label"
        )
        continue
    excused += 1

if named_count < MIN_NAMED:
    problems.append(
        f"{named_count} call site(s) name an app — expected at least "
        f"{MIN_NAMED}. If the named form stopped being used, this axis has "
        "gone back to where it started and the gate should go with it"
    )

if problems:
    print("probes-name-the-app: FAIL")
    for problem in problems:
        print(f"  - {problem}")
    sys.exit(1)

print(
    f"probes-name-the-app: clean — {len(sites)} call sites, {named_count} named, "
    f"{excused} unnamed with a reason"
)
