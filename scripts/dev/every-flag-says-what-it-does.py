#!/usr/bin/env python3
"""Every flag on the CLI's surface says what it does.

A consumer read `smix tap --help` and found `--ocr-locale` described with
`--port`'s sentence while `--port` itself was blank. It was not a typo:
`--port`'s doc comment sat above the field *before* it, because somebody
inserted a new field between the comment and the field it described, and
clap read the comment as the new field's. The same shape was on fifteen
fields; the consumer saw the four on commands they happened to run.

A flag with no description on the surface is a sentence nobody wrote, and
nothing was looking at that axis.

Hidden flags are exempt by name: `#[arg(hide = true)]` keeps something off
the surface deliberately, and there is nothing for a reader to read. So is
`#[command(subcommand)]`, which is a dispatch point rather than something
typed — clap renders no description for it.

**Positionals count.** The first draft of this scan looked for `#[arg(`,
which is how flags are written and how positionals usually are not: it
called a file clean while `smix fill --help` rendered `<SELECTOR>` with
nothing after it. Thirty-nine fields were undescribed once it asked about
every field that reaches the surface rather than every field with an
attribute — and the axis this gate exists for is exactly the one where a
reader sees a blank.
"""

import os
import re
import sys

ROOT = (
    sys.argv[1]
    if len(sys.argv) > 1
    else os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
)
SOURCE = os.path.join(ROOT, "crates", "smix-cli", "src", "main.rs")

# Below this the scan is describing a codebase that is not this one.
MIN_FIELDS = 40

ATTR = re.compile(r"^\s*#\[")
DOC = re.compile(r"^\s*///")
# A field of a clap variant: eight spaces in, `name: Type`.
#
# Indentation alone is not the predicate. A struct literal inside a
# function body is indented the same way, and the first draft judged six
# of them — `RunLease { leases, device_id: … }` is not a command-line
# argument and has no surface to be blank on. What decides is which
# definition the line is inside, so the scan tracks that and this pattern
# only shapes what a field looks like once we are in one.
FIELD = re.compile(r"^        ([a-z_][a-z0-9_]*)\s*:\s*[A-Za-z]")
# Where clap's surface is declared. Anything outside these is code.
DERIVE = re.compile(r"^#\[derive\([^)]*\b(Parser|Subcommand|Args)\b")

problems = []
flags = 0
hidden = 0

if not os.path.isfile(SOURCE):
    print("every-flag-says-what-it-does: FAIL")
    print(f"  - {SOURCE} is absent — this scan has nothing to read")
    sys.exit(1)

lines = open(SOURCE, encoding="utf-8").read().splitlines()

# Which line ranges are clap definitions: from a qualifying `#[derive(...)]`
# to the closing brace at column 0.
regions = []
for i, line in enumerate(lines):
    if not DERIVE.match(line):
        continue
    j = i + 1
    while j < len(lines) and not lines[j].startswith("}"):
        j += 1
    regions.append((i, j))

if not regions:
    problems.append(
        f"no clap definitions found in {SOURCE} — the derive pattern stopped "
        "matching and this scan would pass by reading nothing"
    )

def in_clap(n):
    return any(a <= n <= b for a, b in regions)

for i, line in enumerate(lines):
    m = FIELD.match(line)
    if not m or not in_clap(i):
        continue
    field = m.group(1)
    # Walk up past this field's own attribute lines, however many, to
    # whatever precedes them.
    k = i - 1
    while k >= 0 and ATTR.match(lines[k]):
        k -= 1
    attrs = "\n".join(lines[k + 1 : i])
    if "hide = true" in attrs or "command(subcommand)" in attrs:
        hidden += 1
        continue
    flags += 1
    if k < 0 or not DOC.match(lines[k]):
        problems.append(
            f"main.rs:{i + 1} `{field}` reaches the surface with no "
            "description. clap reads the `///` directly above the field's "
            "attributes; a field with none renders blank, and a field "
            "inserted between a comment and what it described takes the "
            "comment with it and leaves the next one bare"
        )

if flags < MIN_FIELDS:
    problems.append(
        f"only {flags} flag(s) found in {SOURCE} — expected at least "
        f"{MIN_FIELDS}. Either the CLI shrank or this scan stopped matching, "
        "and a clean verdict would mean nothing"
    )

if problems:
    print("every-flag-says-what-it-does: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print(
    f"every-flag-says-what-it-does: clean — {flags} flags, all described, "
    f"{hidden} hidden"
)
