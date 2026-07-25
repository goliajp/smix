#!/usr/bin/env python3
"""The plugin teaches nothing smix cannot do on its own.

The invariant, stated once: a plugin adds initiative, not capability.
Someone who uninstalls it loses convenience, not power. That is the same
rule §12.1 applies to drivers — a consumer must not become where a
capability lives — with the plugin as the consumer.

Prose cannot hold that. What can: every `smix <subcommand>` a skill names
has to be a real subcommand, every `smix_<tool>` a real tool, and no skill
may teach the raw device commands the plugin's own guards refuse. A skill
that drifted ahead of the CLI would be documenting a capability that only
exists inside Claude Code, which is the failure this checks for.
"""

import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SKILLS = os.path.join(ROOT, "plugin", "skills")

problems: list[str] = []


def skill_files() -> list[str]:
    found = []
    for base, _dirs, files in os.walk(SKILLS):
        for name in files:
            if name == "SKILL.md":
                found.append(os.path.join(base, name))
    return sorted(found)


def cli_subcommands() -> set[str]:
    """Top-level subcommands, read from the CLI itself."""
    binary = os.environ.get("SMIX_BIN", os.path.join(ROOT, "target", "release", "smix"))
    if not os.access(binary, os.X_OK):
        problems.append(f"no smix binary at {binary} (cargo build -p smix-cli --release)")
        return set()
    out = subprocess.run([binary, "--help"], capture_output=True, text=True, check=False)
    text = out.stdout + out.stderr
    body = text.split("Commands:", 1)
    if len(body) < 2:
        problems.append("could not read a command list out of `smix --help`")
        return set()
    names = set()
    for line in body[1].splitlines():
        m = re.match(r"\s{2}(\S+)", line)
        if m and m.group(1) not in {"help"}:
            names.add(m.group(1))
    return names


def mcp_tools() -> set[str]:
    """Tool names the MCP server offers, asked of the server."""
    binary = os.environ.get(
        "SMIX_MCP_BIN", os.path.join(ROOT, "target", "release", "smix-mcp")
    )
    if not os.access(binary, os.X_OK):
        problems.append(f"no smix-mcp binary at {binary}")
        return set()
    requests = (
        json.dumps(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "parity", "version": "0"},
                },
            }
        )
        + "\n"
        + json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"})
        + "\n"
        + json.dumps({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
        + "\n"
    )
    out = subprocess.run(
        [binary], input=requests, capture_output=True, text=True, check=False, timeout=60
    )
    for line in out.stdout.splitlines():
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        if msg.get("id") == 2:
            return {t["name"] for t in msg.get("result", {}).get("tools", [])}
    problems.append("the MCP server did not answer tools/list")
    return set()


files = skill_files()
if not files:
    # An empty skills directory would otherwise pass every check below,
    # which is the same as reporting coverage for nothing.
    print("plugin-capability-parity: FAIL")
    print("  - no SKILL.md found under plugin/skills/")
    sys.exit(1)

subcommands = cli_subcommands()
tools = mcp_tools()

cited_commands = 0
cited_tools = 0

for path in files:
    rel = os.path.relpath(path, ROOT)
    text = open(path).read()

    for m in re.finditer(r"`smix ([a-z][a-z0-9-]*)", text):
        cited_commands += 1
        name = m.group(1)
        if subcommands and name not in subcommands:
            problems.append(f"{rel} names `smix {name}`, which the CLI does not offer")

    for m in re.finditer(r"`?(smix_[a-z_]+)", text):
        cited_tools += 1
        name = m.group(1)
        if tools and name not in tools:
            problems.append(f"{rel} names {name}, which the MCP server does not offer")

    # Teaching the raw commands the plugin's own guards refuse would hand
    # someone the workaround along with the rule.
    for pattern, what in (
        (r"xcrun simctl (boot|shutdown|erase|delete|install|launch)", "a mutating simctl verb"),
        (r"\badb (install|uninstall|push|shell pm)", "a mutating adb verb"),
    ):
        if re.search(pattern, text):
            problems.append(f"{rel} teaches {what}; the plugin's guards refuse those")

if cited_commands == 0 and cited_tools == 0:
    problems.append("the skills name no smix command or tool, so nothing was checked")

if problems:
    print("plugin-capability-parity: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print(
    f"plugin-capability-parity: clean — {len(files)} skill(s), "
    f"{cited_commands} command citation(s), {cited_tools} tool citation(s), all real"
)
