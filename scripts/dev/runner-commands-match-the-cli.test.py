#!/usr/bin/env python3
"""Every disagreement between gate-port-scan's command list and the CLI is refused.

A fake smix answers `--help` with texts built from the scan's own lists,
so a CLI that agrees reads clean, and each way of disagreeing is shown to
be named — a new command offering a port, an excuse the help has made
a listed command whose help offers none, a recorder with none, a help
that lists almost nothing, and no binary at all. Each case names the sentence it expects, not only an exit
code.
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "runner-commands-match-the-cli.py")
SCAN = os.path.join(HERE, "gate-port-scan.py")


def judge(name, r, must_refuse, expected):
    """Exit code AND sentence. Either alone can be right by accident."""
    said = (r.stdout + r.stderr).strip()
    refused = r.returncode != 0
    last = said.splitlines()[-1][:100] if said else "(silence)"
    if refused != must_refuse:
        return [f"{name}: expected {'a refusal' if must_refuse else 'a pass'}, got {last}"]
    if expected not in said:
        return [f"{name}: {'refused' if refused else 'passed'} for the wrong reason — "
                f"nothing said {expected!r}, it said {last}"]
    print(f"  {name} → {'refused' if refused else 'allowed'} ({expected!r})")
    return []


# The gate asks a binary for its commands. A fake one answers with
# help texts built here, so each disagreement can be shown to be refused.
FAKE_SMIX = """#!/usr/bin/env python3
import json, sys
helps = json.load(open(__file__ + ".json"))
path = " ".join(a for a in sys.argv[1:] if a != "--help")
print(helps.get(path, ""))
"""


def fake_helps(gate) -> dict[str, str]:
    """A CLI that agrees with the gate: every command it lists, and forty more."""
    offers = gate.RUNNER_COMMANDS | set(gate.RECORDS_A_PORT)
    leaves = {c: ("--runner-port <P>" if c in offers else "no port here")
              for c in gate.RUNNER_COMMANDS | set(gate.RECORDS_A_PORT)}
    leaves.update({f"filler{i}": "no port here" for i in range(40)})
    tops: dict[str, list[str]] = {}
    for c in leaves:
        top, _, sub = c.partition(" ")
        tops.setdefault(top, [])
        if sub:
            tops[top].append(sub)

    def listing(names):
        return "Commands:\n" + "".join(f"  {n}  x\n" for n in names) + "Options:\n"

    helps = {"": listing(tops)}
    for top, subs in tops.items():
        helps[top] = listing(subs) if subs else leaves[top]
        for sub in subs:
            helps[f"{top} {sub}"] = leaves[f"{top} {sub}"]
    return helps


def run_cli(helps: dict[str, str] | None):
    import json
    with tempfile.TemporaryDirectory() as d:
        env = dict(os.environ)
        if helps is None:
            env["SMIX_BIN"] = os.path.join(d, "no-such-smix")
        else:
            fake = os.path.join(d, "smix")
            with open(fake, "w") as fh:
                fh.write(FAKE_SMIX)
            with open(fake + ".json", "w") as fh:
                json.dump(helps, fh)
            os.chmod(fake, 0o755)
            env["SMIX_BIN"] = fake
        return subprocess.run([sys.executable, GATE],
                              capture_output=True, text=True, env=env)


def cli_cases(gate):
    base = fake_helps(gate)
    dialled = sorted(gate.RUNNER_COMMANDS)[0]
    recorder = sorted(gate.RECORDS_A_PORT)[0]
    listed = dict(base)
    listed[""] = base[""].replace("Options:", "  brand-new  x\nOptions:")
    listed["brand-new"] = "--runner-port <P>"
    return [
        ("a CLI that agrees", base, False, "clean"),
        ("a command offering a port the gate does not list", listed, True,
         "`smix brand-new` offers a runner port and is not in gate-port-scan's RUNNER_COMMANDS"),
        ("a listed command whose help offers no port",
         {**base, dialled: "no port here"}, True,
         f"`smix {dialled}` is in gate-port-scan's RUNNER_COMMANDS and its help offers no runner port"),
        ("a recorder whose help offers no port",
         {**base, recorder: "no port here"}, True,
         f"`smix {recorder}` is listed as recording a port and its help offers none"),
        ("no binary", None, True, "no built smix to ask"),
        ("a help that lists almost nothing",
         {"": "Commands:\n  a  x\n  b  x\nOptions:\n", "a": "no port here", "b": "no port here"},
         True, "only 2 commands read from"),
    ]


CLI_CASES: list = []


def against_cli_cases() -> list[str]:
    import importlib.util
    spec = importlib.util.spec_from_file_location("gate_port_scan", SCAN)
    gate = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(gate)
    CLI_CASES.extend(cli_cases(gate))
    out = []
    for name, helps, must_refuse, expected in CLI_CASES:
        out += judge(name, run_cli(helps), must_refuse, expected)
    return out


def main() -> int:
    failures = against_cli_cases()
    if failures:
        print("runner-commands-match-the-cli.test: FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1
    print(f"runner-commands-match-the-cli.test: {len(CLI_CASES)} cases pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
