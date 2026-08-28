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

The script that starts the runner is not always the script that chose
the port. `a-tap-that-cannot-land-says-so.sh` asks the OS and then takes
an argument if it is given one -- and both of its callers gave it a
literal 22091, so a gate that reads correctly on its own ran on a fixed
port anyway, and this scan called it covered. So the second half below
follows the port back through the caller's own variables.

Its boundary, stated rather than left as a consequence of the regex: a
port passed to something that *attaches* to a runner someone else
started (`--port` on the python gates in `v10-exit.sh`) is that
operator's choice of an existing socket, not a port this scan gets to
pick. Only the port a runner is brought *up* on is its business.
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
# A port written as a number: `PORT=28080`, or a default for an override
# (`${SMIX_GATE_RUNNER_PORT:-28080}`). Four or five digits, so a `-p 80`
# or an index is not mistaken for one.
PINS_A_PORT = re.compile(r"[A-Z_]*PORT[A-Z_]*=\s*\"?(?:\$\{[A-Za-z_]+:-)?\s*\d{4,5}\b")

# Prose about the command, in any of the forms this repo writes it.
MENTIONS_ONLY = re.compile(r"^\s*(?:#|log\b|echo\b|printf\b|\*[\"'])")

problems: list[str] = []
checked = 0
covered = 0
bodies: dict[str, str] = {}
runner_scripts: set[str] = set()

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
        bodies[rel] = body
        starts = [
            ln
            for ln in body.splitlines()
            if STARTS_A_RUNNER.match(ln) and not MENTIONS_ONLY.match(ln)
        ]
        if not starts:
            continue
        runner_scripts.add(name)
        checked += 1
        # A literal is not a port of one's own, however it is spelled. The
        # first version of this check looked for the string
        # `SMIX_RUNNER_PORT=` and found it in a teardown line that merely
        # passed a hardcoded 28080 along -- so a gate pinned to a fixed
        # port counted as covered, and died when an unrelated emulator's
        # adb forward held that port during a ship.
        pinned = [
            ln.strip() for ln in body.splitlines()
            if not MENTIONS_ONLY.match(ln) and PINS_A_PORT.search(ln)
        ]
        if pinned:
            problems.append(
                f"{rel} pins a host port to a literal. An adb forward or "
                f"another checkout can hold it, and then this gate is red "
                f"about something else entirely. Ask the OS: source "
                f"scripts/lib/gate-port.sh.\n      {pinned[0]}"
            )
            continue
        if "gate-port.sh" in body or "SMIX_RUNNER_PORT=" in body:
            covered += 1
            continue
        problems.append(
            f"{rel} brings a runner up on the default port — one unrelated "
            f"runner anywhere on the machine turns it red before it runs "
            f'anything. Source scripts/lib/gate-port.sh.\n      {starts[0].strip()}'
        )

# The other half: whoever hands one of those gates a port. A literal
# reaching the gate through the caller's variable is the same fixed
# socket, arrived at by a longer road.
LITERAL = re.compile(r"(?<![\w.])\d{4,5}(?![\w.])")
REFERENCES = re.compile(r"\$\{?([A-Za-z_][A-Za-z_0-9]*)\}?")
callers = 0
for rel, body in sorted(bodies.items()):
    # `\`-continued invocations put the port on the next physical line.
    logical = body.replace("\\\n", " ").splitlines()
    for ln in logical:
        if MENTIONS_ONLY.match(ln):
            continue
        # Not its own name. A gate's usage string carries it, and counting
        # that as a caller would let this half satisfy itself -- the same
        # shape as an assertion that counts its own invocation as evidence.
        others = runner_scripts - {os.path.basename(rel)}
        if not any(script in ln for script in others):
            continue
        callers += 1
        args = ln.split(".sh", 1)[1] if ".sh" in ln else ln
        if LITERAL.search(args):
            problems.append(
                f"{rel} hands a runner-starting gate a literal port. The "
                f"gate asks the OS for one of its own and this overrides "
                f"that choice, so the pin is here even though the gate "
                f"reads correctly.\n      {ln.strip()}"
            )
            continue
        for var in REFERENCES.findall(args):
            for assign in body.splitlines():
                if assign.lstrip().startswith(f"{var}=") and PINS_A_PORT.search(assign):
                    problems.append(
                        f"{rel} hands a runner-starting gate ${var}, which "
                        f"is pinned to a literal. Follow the port back: it "
                        f"is a fixed socket by the time the gate sees it.\n"
                        f"      {assign.strip()}"
                    )

# A regex that matches nothing agrees with every script there is.
if callers == 0:
    problems.append(
        "nothing was found invoking a runner-starting gate — either the "
        "gates lost their callers or the invocation shape changed, and the "
        "caller-side half of this scan is reading air"
    )
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
