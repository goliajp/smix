#!/usr/bin/env python3
"""The commands gate-port-scan treats as dialling a runner are the CLI's.

gate-port-scan decides which script lines talk to a runner from
`RUNNER_COMMANDS`. A list like that goes stale the day a command is added,
and a script using the new one is then invisible to the scan. So the list
is held against the built binary: every command whose help offers a runner
port is either in it or named as only recording one, and every command in
it that the help does not mention reads the port in code, with the place
written down — so the day its help says so, the excuse has to go.

Needs a built smix (`cargo build -p smix-cli`); a missing one is a
failure, not a skip.

Usage:
  scripts/dev/runner-commands-match-the-cli.py
"""

from __future__ import annotations

import importlib.util
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import _e2e_binary  # noqa: E402

_spec = importlib.util.spec_from_file_location("gate_port_scan", os.path.join(HERE, "gate-port-scan.py"))
scan = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(scan)


def help_commands(binary: str) -> dict[str, str]:
    """Every leaf command path and its help text, asked of the binary."""
    def ask(*path: str) -> str:
        return subprocess.run([binary, *path, "--help"], capture_output=True, text=True).stdout

    def subcommands(text: str) -> list[str]:
        block = text.split("Commands:", 1)[1].split("Options:", 1)[0] if "Commands:" in text else ""
        return [ln.split()[0] for ln in block.splitlines() if ln.strip() and ln.split()[0] != "help"]

    out: dict[str, str] = {}
    for top in subcommands(ask()):
        text = ask(top)
        subs = subcommands(text)
        if not subs:
            out[top] = text
        for sub in subs:
            out[f"{top} {sub}"] = ask(top, sub)
    return out


def main() -> int:
    try:
        binary = _e2e_binary.this_tree_smix()
    except SystemExit as e:
        print(f"runner-commands-match-the-cli: FAIL\n  - no built smix to ask ({e}); "
              f"`cargo build -p smix-cli` first")
        return 1
    commands = help_commands(binary)
    problems: list[str] = []
    if len(commands) < 40:
        problems.append(f"only {len(commands)} commands read from {binary} --help — the help "
                        f"shape changed and this is reading air")
    offers = {c for c, text in commands.items() if "--runner-port" in text or "SMIX_RUNNER_PORT" in text}
    for c in sorted(offers - scan.RUNNER_COMMANDS - set(scan.RECORDS_A_PORT)):
        problems.append(f"`smix {c}` offers a runner port and is not in gate-port-scan's RUNNER_COMMANDS — "
                        f"a script can dial a runner with it and this scan would not see it")
    for c in sorted(scan.RUNNER_COMMANDS - offers):
        if c not in scan.READS_THE_PORT_UNSAID:
            problems.append(f"`smix {c}` is in gate-port-scan's RUNNER_COMMANDS, its help offers no runner port, "
                            f"and nothing says where it reads one")
    for c, where in sorted(scan.READS_THE_PORT_UNSAID.items()):
        if c in offers:
            problems.append(f"`smix {c}` now says so in its help — drop it from "
                            f"gate-port-scan's READS_THE_PORT_UNSAID ({where})")
        if c not in commands:
            problems.append(f"`smix {c}` is excused as reading the port unsaid and is not a command")
    for c in sorted(set(scan.RECORDS_A_PORT) - offers):
        problems.append(f"`smix {c}` is listed as recording a port and its help offers none")
    if problems:
        print("runner-commands-match-the-cli: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(f"runner-commands-match-the-cli: clean — {len(offers)} commands offer a runner port, "
          f"{len(scan.RUNNER_COMMANDS)} dial one, {len(scan.READS_THE_PORT_UNSAID)} without saying so")
    return 0



if __name__ == "__main__":
    sys.exit(main())
