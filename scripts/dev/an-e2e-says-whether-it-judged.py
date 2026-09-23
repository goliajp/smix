#!/usr/bin/env python3
"""An end-to-end script says, in its exit code, whether it judged anything.

Three answers, and they have to be three:

  0  it drove the thing and the thing was right
  1  it drove the thing and the thing was wrong, or the setup this run
     owns did not come up (a busy port, an apk nobody built, a runner
     that would not start)
  2  it could not judge at all — the subject is not on this machine, or
     somebody else is driving it

The one this file exists against is the fourth answer nobody wrote down:
a script that prints `SKIP` and exits 0. That is indistinguishable, to
everything downstream, from a script that ran and passed. It has already
cost this cycle a red run: C5's first red was a runner of mine holding
the port, the script skipped, exit 0, and the log had one line in it.

`scripts/release/device-e2e-tier.sh` counts what actually drove and
fails when nothing did — but it can only count what the scripts tell it,
and while "skipped" and "passed" share an exit code it was reading the
word SKIP out of their output, which a passing script's own log can
contain.

The second half is the same defect in another coordinate: a script that
names `target/release/smix` while its neighbours build and drive
`target/debug/smix`. C12 hit exactly that — a flag added to the debug
binary, a gate invoking the release one, and the script exiting 1 with
no output at all because `set -e` killed it inside a command
substitution before a single verdict printed.

Standing aside is exit 2, not exit 1. `yield-is-not-failure-scan` says
why — a script that finds a port taken or another batch on the device
has not found a defect, and a gate that goes red about the machine is a
gate people stop reading. Writing this scan, I moved the busy-port
refusals to `fail` and that gate caught it within the minute. The two
are not in tension once there are three answers: standing aside is
neither a failure nor a run that happened, and the third code is what
lets both be true.

Usage:
  scripts/dev/an-e2e-says-whether-it-judged.py [repo-root]
"""

import os
import re
import sys

ROOT = (
    sys.argv[1]
    if len(sys.argv) > 1
    else os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
)
DEV = os.path.join(ROOT, "scripts", "dev")

# How few scripts mean this scan failed to find its subject rather than
# finding it clean. Fifty-four exist today; a glob that suddenly matches
# three has stopped looking at the thing it judges.
MIN_SCRIPTS = 50

# A function that stands in for "I cannot judge this". The names this
# repository has used, and any new one is one word away from being added
# — which is cheaper than a scan that only knows one spelling.
EXCUSE = re.compile(r"^\s*(skip|cannot_judge|yield_to\w*|bail)\s*\(\)\s*\{(.*)$", re.M)

# The binary an e2e drives, chosen by the script rather than asked for.
#
# An ASSIGNMENT, not any mention: the federation lanes invoke
# `target/release/smix` on another host over ssh, having built it there
# and stamped it, and that is a different machine's binary rather than
# this script picking one. What M2 was about is the line that decides
# which binary THIS script will drive.
HARDCODED_BIN = re.compile(
    r"^\s*(?:SMIX|SMIX_BIN|MCP|SMIX_MCP_BIN)=.*target/(?:debug|release)/smix"
)

# Where a script may name a binary path: the one helper that resolves it,
# and prose about it.
BIN_HELPER = "scripts/lib/e2e-binary.sh"


def function_body(lines, start):
    """The lines of a `name() {` function, brace-counted from `start`."""
    depth, body = 0, []
    for ln in lines[start:]:
        depth += ln.count("{") - ln.count("}")
        body.append(ln)
        if depth <= 0:
            break
    return body


def judge(path, text):
    """What is wrong with one script, as sentences."""
    problems = []
    lines = text.split("\n")
    for i, ln in enumerate(lines):
        m = EXCUSE.match(ln)
        if not m:
            continue
        name = m.group(1)
        body = function_body(lines, i)
        if any(re.search(r"\bexit 0\b", b) for b in body):
            problems.append(
                f"{os.path.relpath(path, ROOT)}: `{name}()` exits 0. A leg that "
                f"could not judge and a leg that passed then have the same exit "
                f"code, and the tier that counts what drove cannot tell them "
                f"apart. Exit 2, and say on stderr what is missing."
            )
    # A script that calls one of these and never defines it fails with
    # `command not found` on the one path it was added for — and that
    # path is, by construction, the one nobody runs. Found by writing
    # exactly this defect into three scripts while converting them.
    called = {
        m.group(1)
        for m in re.finditer(r"(?:^|[\s;(])(skip|cannot_judge|bail)\s+\"", text, re.M)
    }
    defined = {m.group(1) for m in EXCUSE.finditer(text)}
    for name in sorted(called - defined):
        problems.append(
            f"{os.path.relpath(path, ROOT)}: calls `{name}` and does not define it. "
            f"The one path it was added for ends in `command not found`, and that "
            f"path is the one nothing exercises."
        )

    for i, ln in enumerate(lines):
        if ln.lstrip().startswith("#"):
            continue
        if HARDCODED_BIN.search(ln) and BIN_HELPER not in ln:
            problems.append(
                f"{os.path.relpath(path, ROOT)}:{i + 1}: picks its own binary "
                f"(`{ln.strip()}`). Two halves of one checkpoint then read two "
                f"binaries — source {BIN_HELPER} and use `$SMIX`."
            )
    return problems


def main():
    scripts = sorted(
        os.path.join(DEV, f) for f in os.listdir(DEV) if f.endswith("-e2e.sh")
    )
    if len(scripts) < MIN_SCRIPTS:
        print("an-e2e-says-whether-it-judged: FAIL")
        print(
            f"  - {len(scripts)} e2e scripts found under {os.path.relpath(DEV, ROOT)}, "
            f"fewer than the {MIN_SCRIPTS} this repository has. A scan that has "
            f"lost its subject reads exactly like a clean one."
        )
        return 1

    problems = []
    for path in scripts:
        with open(path, encoding="utf-8") as fh:
            problems += judge(path, fh.read())

    if problems:
        print("an-e2e-says-whether-it-judged: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    print(
        f"an-e2e-says-whether-it-judged: clean — {len(scripts)} scripts, each "
        f"answering 0/1/2 and each taking its binary from one place"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
