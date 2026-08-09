#!/usr/bin/env python3
"""A gate that starts a runner does not take the default port.

`smix runner up` defaults to 22087. A gate that takes that default is
red whenever anything else on the machine holds the port — another
checkout, a developer's session, a runner orphaned by a crash. It fails
at startup, before running a single flow, and the failure reads as smix
being broken rather than as two gates wanting the same socket.

That happened on 2026-08-09: the corpus gate exited 3 at `runner up`
against an unrelated runner, in the middle of judging whether the corpus
was deterministic enough to release on. A gate a bystander can turn red
answers no question about the product.

`scripts/lib/gate-port.sh` asks the OS for a free port and exports
`SMIX_RUNNER_PORT`, which `--runner-port` reads via clap's `env`, so one
export reaches startup, every flow, and teardown alike. Sourcing it is
the fix; this scan is what keeps the next gate from forgetting.

Setting `SMIX_RUNNER_PORT` some other way also satisfies this — the
requirement is a port of one's own, not a particular way of getting one.

What this does not see: a `runner up` nested inside an ssh string, as
the federation gates run on their far node. Those ports belong to the
other machine and are not this scan's business, but the exemption is a
consequence of the regex rather than a decision, so it is written down
here instead of being a list that reads like coverage. An earlier draft
did keep such a list; it matched nothing and said so on every run.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SCRIPTS = os.path.join(ROOT, "scripts")

# Executes `runner up` rather than merely mentioning it.
#
# The first version matched the words anywhere on a non-comment line and
# reported four scripts that only talk about the command: a `log "…"`
# telling the reader what to run next, and a `case` pattern in the adb
# guard's own test table. A scan that cannot tell a command from a
# sentence about a command makes work rather than finding it.
STARTS_A_RUNNER = re.compile(
    r"""^\s*
        (?:if\s+!?\s*|\(\s*cd\s[^&]*&&\s*)?     # `if !` / `( cd X &&` prefixes
        "?\$?\{?(?:smix|SMIX_BIN|SMIX)\}?"?     # the binary, however spelled
        (?:/[\w/.\-]*smix)?                     # or a path ending in smix
        \s+runner\ up\b""",
    re.VERBOSE,
)
# Prose about the command, in any of the forms this repo writes it.
MENTIONS_ONLY = re.compile(r"^\s*(?:#|log\b|echo\b|printf\b|\*[\"'])")

problems: list[str] = []
checked = 0
covered = 0

for dirpath, _dirs, files in os.walk(SCRIPTS):
    for name in sorted(files):
        if not name.endswith(".sh"):
            continue
        path = os.path.join(dirpath, name)
        rel = os.path.relpath(path, ROOT)
        try:
            body = open(path, encoding="utf-8").read()
        except (OSError, UnicodeDecodeError):
            continue
        starts = [
            ln
            for ln in body.splitlines()
            if STARTS_A_RUNNER.match(ln) and not MENTIONS_ONLY.match(ln)
        ]
        if not starts:
            continue
        checked += 1
        if "gate-port.sh" in body or "SMIX_RUNNER_PORT=" in body:
            covered += 1
            continue
        problems.append(
            f"{rel} brings a runner up on the default port — one unrelated "
            f"runner anywhere on the machine turns it red before it runs "
            f'anything. Source scripts/lib/gate-port.sh.\n      {starts[0].strip()}'
        )

# A regex that matches nothing agrees with every script there is.
if checked == 0:
    problems.append(
        "no script was found starting a runner — the invocation shape changed "
        "and this scan is now reading air"
    )

if problems:
    print("gate-port-scan: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print(f"gate-port-scan: clean — {covered} runner-starting gates hold a port of their own")
