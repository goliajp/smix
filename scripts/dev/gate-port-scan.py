#!/usr/bin/env python3
"""A script that talks to a runner does it on a port of its own.

`smix runner up` defaults to 22087, and so does every command that dials a
runner. A gate that takes that default is red whenever anything else on
the machine holds the port — another checkout, a developer's session, a
runner orphaned by a crash — and the failure reads as smix being broken
rather than as two gates wanting the same socket (the corpus gate,
2026-08-09). Worse, it can reach somebody else's runner: a test's teardown
that ran `runner down` with no port stopped whatever runner the machine's
ledger had on 22087, and a consumer's run found its runner's record beside
one of ours on that port and refused to start (2026-09-26).

`scripts/lib/gate-port.sh` asks the OS for a free port and exports
`SMIX_RUNNER_PORT`, which every runner command reads, so one export
reaches startup, every flow, an MCP session started from the script, and
teardown alike. Setting `SMIX_RUNNER_PORT` some other way also satisfies
this — the requirement is a port of one's own, not a way of getting one.

Three rules:

1. A script that runs a runner command here, or starts an MCP session
   (which dials the runner the environment names), holds a port of its
   own — or names, in every such command, the port its caller gave it.
   An Android `runner down --device` is not a runner command for this
   purpose: it is scoped by device and never reads the port.
2. A runner command sent over ssh names its port in that line. An export
   does not cross ssh, and the far machine's default is somebody's too.
3. No port is pinned to a literal, here or by whoever calls the script.
4. A Python gate takes its port from its caller or from the OS: an
   argparse port option, or an environment fallback, has no literal
   default, and a `runner up` argv names its port. The four v10 gates
   defaulted to 22095 and attach to a runner somebody else brought up, so
   the port is the caller's to give — one asked of the OS is by
   construction one nobody is listening on.

Which commands dial a runner is the CLI's to say: `RUNNER_COMMANDS` is
held against the built binary by runner-commands-match-the-cli.py.

A port passed to something that *attaches* to a runner someone else
started (`--port` on the python gates in `v10-exit.sh`) is that operator's
choice of an existing socket, not a port this scan gets to pick.

Usage:
  scripts/dev/gate-port-scan.py
"""

from __future__ import annotations

import ast
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from _shell_lines import code_lines  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(HERE))
SCRIPTS = os.path.join(ROOT, "scripts")

# Commands that dial a runner. Each says so in its help.
RUNNER_COMMANDS = {
    "runner up", "runner down", "runner supervise", "runner cycle",
    "runner list-sessions", "capsule up", "capsule down", "down", "doctor", "run",
    "run-script", "tap", "find", "wait-for", "fill", "press-key", "swipe",
    "scroll", "hide-keyboard", "tree", "describe", "system-popups",
    "system-popup-action", "authoring suggest", "authoring capture-tree",
    "authoring diff-tree", "authoring record", "diagnostic dump",
    "sim screenshot",
}
# Offer a runner port only to write it into the registry; they dial nothing.
RECORDS_A_PORT = {
    "sim register": "stores the sim's runner port in the registry",
}

# A bare `smix` right after a quote or `*` is inside a pattern or a string
# (`*"smix runner up"*)` in a case table), not a command.
_BINARY = r"""(?:"?\$\{?(?:SMIX|SMIX_BIN)\}?"?|(?<![\w./*'"`-])smix|\S*/smix)"""
_VERBS = "|".join(sorted((re.escape(c).replace(r"\ ", r"\s+") for c in RUNNER_COMMANDS), key=len, reverse=True))
RUNS_A_COMMAND = re.compile(rf"{_BINARY}\s+(?:{_VERBS})(?![\w-])")
RUNS_A_FLOW = re.compile(r'"?\$\{?SMIX_RUN\}?"?\s')
# A session that loads the plugin, or the server run directly. The word
# `smix-mcp` in an install or a path starts nothing.
MCP_SESSION = re.compile(r"\bclaude\b.*--plugin-dir\b|^\s*(?:\(\s*)?\"?\$\{?SMIX_MCP\}?\"?\s")
ANDROID_DOWN = re.compile(r"\brunner\s+down\b(?=.*--platform\s+\"?android)(?=.*--device\b)")
OVER_SSH = re.compile(r"\brssh\b|\bssh\s")
NAMES_A_PORT = re.compile(r"--runner-port\s+\"?\$|SMIX_RUNNER_PORT=\"?\$")
# Prose about a command, in the forms this repository writes it.
SPEECH = re.compile(r"^\s*(?:log|echo|printf|step|fail|bad|ok|cannot_judge|note|say|die|warn)\b")
# A port written as a number: `PORT=28080`, or a default for an override
# (`${SMIX_C5_ANDROID_PORT:-22097}`). Four or five digits, so a `-p 80` or
# an index is not mistaken for one. Override names take digits: three gates
# escaped through `SMIX_C5_…`-style names when they could not.
PINS_A_PORT = re.compile(
    r"[A-Z_]*PORT[A-Z_]*=\s*\"?(?:\$\{[A-Za-z_][A-Za-z_0-9]*:-)?\s*\d{4,5}\b"
)
OWN_PORT = re.compile(r"\bgate-port\.sh\b|SMIX_RUNNER_PORT=")
LITERAL = re.compile(r"(?<![\w.])\d{4,5}(?![\w.])")
REFERENCES = re.compile(r"\$\{?([A-Za-z_][A-Za-z_0-9]*)\}?")


def logical(lines: list[str], n: int) -> str:
    """Line n (1-based) with the lines its trailing backslashes continue onto."""
    out = []
    for ln in lines[n - 1:]:
        out.append(ln.rstrip())
        if not ln.rstrip().endswith("\\"):
            break
    return " ".join(x.rstrip("\\") for x in out)


PORT_OPTION = re.compile(r"port|listen|forward", re.I)
LITERAL_PORT = re.compile(r"^\d{4,5}$")


def _const(node: ast.AST | None) -> str | None:
    if isinstance(node, ast.Constant) and isinstance(node.value, (str, int)):
        return str(node.value)
    return None


def python_problems(body: str, rel: str) -> tuple[list[str], bool]:
    """Literal port defaults and runner starts without a port, in Python code.

    Returns the problems and whether the script takes a port from its caller
    (an argparse port option with no default), which makes it one whose
    callers are checked for handing it a literal.
    """
    try:
        tree = ast.parse(body)
    except SyntaxError:
        return [], False
    problems: list[str] = []
    takes_one = False
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        fn = node.func
        name = fn.attr if isinstance(fn, ast.Attribute) else getattr(fn, "id", "")
        if name == "add_argument":
            opts = [_const(a) or "" for a in node.args]
            if not any(o.startswith("-") and PORT_OPTION.search(o) for o in opts):
                continue
            kw = {k.arg: k.value for k in node.keywords}
            default = _const(kw.get("default"))
            if default is not None and LITERAL_PORT.match(default):
                problems.append(
                    f"{rel}:{node.lineno}: {'/'.join(opts)} defaults to {default} — a literal "
                    f"port is a socket somebody else can hold; take it from the caller "
                    f"(required=True) or ask the OS"
                )
            elif "default" not in kw:
                takes_one = True
        elif name in ("get", "getenv") and len(node.args) >= 2:
            key, fallback = _const(node.args[0]) or "", _const(node.args[1])
            if "PORT" in key and fallback is not None and LITERAL_PORT.match(fallback):
                problems.append(
                    f"{rel}:{node.lineno}: falls back to port {fallback} when {key} is unset — "
                    f"the machine's default is whoever else runs smix; require it or ask the OS"
                )
        for arg in node.args:
            if isinstance(arg, (ast.List, ast.Tuple)):
                words = [_const(e) for e in arg.elts]
                starts = any(words[i:i + 2] == ["runner", "up"] for i in range(len(words)))
                if starts and "--runner-port" not in words:
                    problems.append(
                        f"{rel}:{node.lineno}: starts a runner without --runner-port — it lands "
                        f"on the machine's default port"
                    )
    return problems, takes_one


def dials(line: str) -> bool:
    if MCP_SESSION.search(line) or RUNS_A_FLOW.search(line):
        return True
    m = RUNS_A_COMMAND.search(line)
    return bool(m) and not ANDROID_DOWN.search(line[m.start():])


def scan_scripts() -> int:
    problems: list[str] = []
    bodies: dict[str, str] = {}
    runner_scripts: set[str] = set()
    python_takers: set[str] = set()
    covered = remote = 0
    for dirpath, _dirs, files in os.walk(SCRIPTS):
        for name in sorted(files):
            if not name.endswith((".sh", ".py")):
                continue
            path = os.path.join(dirpath, name)
            rel = os.path.relpath(path, ROOT)
            try:
                body = open(path, encoding="utf-8").read()
            except (OSError, UnicodeDecodeError):
                continue
            if name.endswith(".py"):
                found, takes_one = python_problems(body, rel)
                problems += found
                if takes_one:
                    python_takers.add(name)
                continue
            bodies[rel] = body
            physical = body.splitlines()
            local: list[tuple[int, str]] = []
            for n, ln in code_lines(body):
                if SPEECH.match(ln) or not dials(ln):
                    continue
                whole = logical(physical, n)
                if OVER_SSH.search(whole):
                    remote += 1
                    if not NAMES_A_PORT.search(whole):
                        problems.append(
                            f"{rel}:{n}: sends a runner command over ssh without naming its "
                            f"port — an export does not cross ssh, and the far machine's "
                            f"default port is somebody's too. Put --runner-port \"$PORT\" in "
                            f"the remote command.\n      {ln.strip()}"
                        )
                else:
                    local.append((n, whole))
            if not local:
                continue
            runner_scripts.add(name)
            pinned = [
                ln.strip() for _, ln in code_lines(body)
                if not SPEECH.match(ln) and PINS_A_PORT.search(ln)
            ]
            if pinned:
                problems.append(
                    f"{rel} pins a host port to a literal. An adb forward or another "
                    f"checkout can hold it, and then this gate is red about something "
                    f"else entirely. Ask the OS: source scripts/lib/gate-port.sh.\n"
                    f"      {pinned[0]}"
                )
            elif OWN_PORT.search(body) or all(NAMES_A_PORT.search(w) for _, w in local):
                # A port of its own, or every runner command names the one
                # its caller gave it (attaching to a runner somebody started).
                covered += 1
            else:
                n, ln = local[0]
                problems.append(
                    f"{rel}:{n}: talks to a runner on the machine's default port (22087, "
                    f"every smix's) — it collides with, or acts on, whoever else is there. "
                    f"Source scripts/lib/gate-port.sh.\n      {ln.strip()}"
                )

    # Whoever hands one of those scripts a port: a literal reaching it through
    # the caller's variable is the same fixed socket, one step further away.
    callers = 0
    for rel, body in sorted(bodies.items()):
        others = (runner_scripts | python_takers) - {os.path.basename(rel)}
        for ln in body.replace("\\\n", " ").splitlines():
            if SPEECH.match(ln) or ln.lstrip().startswith("#"):
                continue
            if not any(script in ln for script in others):
                continue
            callers += 1
            args = next((ln.split(ext, 1)[1] for ext in (".sh", ".py") if ext in ln), ln)
            if LITERAL.search(args):
                problems.append(
                    f"{rel} hands a runner-driving script a literal port, overriding the "
                    f"one it would ask the OS for.\n      {ln.strip()}"
                )
                continue
            for var in REFERENCES.findall(args):
                for assign in body.splitlines():
                    if assign.lstrip().startswith(f"{var}=") and PINS_A_PORT.search(assign):
                        problems.append(
                            f"{rel} hands a runner-driving script ${var}, which is pinned "
                            f"to a literal.\n      {assign.strip()}"
                        )

    # A pattern that matches nothing agrees with every script there is.
    if not runner_scripts:
        problems.append("no script was found talking to a runner — the invocation shape "
                        "changed and this scan is reading air")
    if not python_takers:
        problems.append("no Python gate was found taking its port from its caller — the "
                        "argparse shape changed and the Python half is reading air")
    if callers == 0:
        problems.append("nothing was found invoking a runner-driving script — the "
                        "caller-side half of this scan is reading air")
    if problems:
        print("gate-port-scan: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(
        f"gate-port-scan: clean — {covered} script(s) talking to a runner hold a port of "
        f"their own; {remote} runner command(s) over ssh name theirs; {len(python_takers)} "
        f"Python gate(s) take theirs from the caller"
    )
    return 0


if __name__ == "__main__":
    sys.exit(scan_scripts())
