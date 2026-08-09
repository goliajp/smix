#!/usr/bin/env python3
"""Every MCP tool names the CLI command that does the same thing.

The plugin adds initiative, not capability: it must not be able to do
something `smix` on its own cannot. `plugin-capability-parity` checks
that rule in one direction — a skill may not name a command the CLI
lacks. Nothing checked the other, and `smix_swipe` sat in the tool list
for the whole life of the MCP server with no `smix swipe` behind it.
`smix_assert_not_visible` was the same: the CLI could wait for an
element to appear and had no way to wait for one to go.

The mapping cannot be derived. Nine of sixteen tools have no
same-named subcommand and seven of those nine are only renames —
`smix_devices` is `smix sim list`, `smix_use` is `smix runner up`. A
scan that diffs names reports those seven as missing capabilities, and a
gate that cries wolf seven times teaches people to skip it.

So each tool declares its own counterpart, in a `/// CLI:` line directly
above it, and this asks the binary whether that command exists. The
declaration lives with the tool on purpose: a central table would go
stale exactly the way the route list, the fast-path modifier list and
three copies of "are these modifiers empty" all did.

A tool with no counterpart writes `/// CLI: none — <reason>`. Saying
nothing is not an option; that is how the two above stayed missing.
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
MCP = os.path.join(ROOT, "crates", "smix-mcp", "src", "main.rs")

problems: list[str] = []


def smix_binary() -> str | None:
    """The most recently built CLI, whichever profile that is.

    An earlier draft preferred release unconditionally and reported two
    commands as missing minutes after they were added — the release
    build predated them. A stale binary answers about a version that is
    not the one under test, and which profile it came from says nothing
    about how stale it is.
    """
    built = [
        p
        for p in (os.path.join(ROOT, r) for r in ("target/release/smix", "target/debug/smix"))
        if os.path.isfile(p) and os.access(p, os.X_OK)
    ]
    return max(built, key=os.path.getmtime) if built else None


def declarations() -> dict[str, str]:
    """Each `async fn smix_*` and the `/// CLI:` line above it."""
    if not os.path.isfile(MCP):
        problems.append(f"no MCP server source at {MCP}")
        return {}
    out: dict[str, str] = {}
    pending: str | None = None
    for line in open(MCP, encoding="utf-8"):
        m = re.match(r"\s*///\s*CLI:\s*(.+?)\s*$", line)
        if m:
            pending = m.group(1)
            continue
        fn = re.match(r"\s*async fn (smix_[a-z_0-9]+)\s*\(", line)
        if fn:
            out[fn.group(1)] = pending or ""
            pending = None
    return out


tools = declarations()

# A parser that finds nothing agrees with any server at all.
if tools and len(tools) < 10:
    problems.append(
        f"only {len(tools)} MCP tools parsed — the tool shape changed and this "
        "scan is now reading air"
    )

binary = smix_binary()
if tools and binary is None:
    problems.append(
        "no built smix binary — this scan asks the CLI itself whether a "
        "command exists, and cannot take the source's word for it. "
        "`cargo build -p smix-cli` first."
    )

checked = 0
excused = 0
for tool, decl in sorted(tools.items()):
    if not decl:
        problems.append(
            f"{tool} does not say which CLI command does the same thing. Add "
            f"`/// CLI: smix <command>` above it, or `/// CLI: none — <reason>` "
            f"if there genuinely is none — silence is how smix_swipe stayed "
            f"missing for a whole major version"
        )
        continue
    if decl.startswith("none"):
        if not re.match(r"none\s*[—-]\s*\S", decl):
            problems.append(
                f"{tool} declares `none` with no reason. Say why the CLI cannot "
                f"do this, so the next reader can tell a decision from an omission"
            )
        else:
            excused += 1
        continue
    words = decl.split()
    if not words or words[0] != "smix":
        problems.append(
            f"{tool} declares {decl!r}, which does not start with `smix`. The "
            f"declaration is a command someone can type"
        )
        continue
    if binary is None:
        continue
    argv = [binary, *words[1:], "--help"]
    result = subprocess.run(argv, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        problems.append(
            f"{tool} declares `{decl}` and that command does not exist — "
            f"`{' '.join(words)} --help` exited {result.returncode}. Either the "
            f"CLI is missing a capability the MCP server has, or the "
            f"declaration is out of date"
        )
    else:
        checked += 1

if problems:
    print("mcp-cli-parity-scan: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print(
    f"mcp-cli-parity-scan: clean — {checked} MCP tools name a CLI command that "
    f"exists, {excused} declare none with a reason"
)
