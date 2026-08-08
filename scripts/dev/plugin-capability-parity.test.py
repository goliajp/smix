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

That is one direction, and for a long time it was the only one: a skill
could name nothing false and still be wrong. `drive/SKILL.md` promised
"an iOS Simulator or Android emulator" in its description and then, for
two releases, said nothing about Android anywhere in its body — no
registration, no `--platform android`, nothing. A reader followed the iOS
steps, hit a simctl timeout, and concluded the product had no Android
support. Everything the old check looked at was fine.

So the other direction: a term the description promises has to be taught
in the body. Which terms count is not a list somebody maintains here —
that would go stale the same way, and a hand-picked list of "capabilities
worth checking" is how the thing you forgot stays forgotten. The
vocabulary is read out of the product: a word is a capability term if the
CLI's own help or an MCP tool description uses it.
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


_MCP_TOOLS: list[dict] = []


def tool_descriptions() -> list[str]:
    return [t.get("description", "") for t in _MCP_TOOLS]


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
            _MCP_TOOLS.extend(msg.get("result", {}).get("tools", []))
            return {t["name"] for t in _MCP_TOOLS}
    problems.append("the MCP server did not answer tools/list")
    return set()


STOPWORDS = {
    "a", "an", "and", "any", "anything", "are", "as", "at", "be", "by",
    "can", "check", "do", "for", "from", "in", "into", "is", "it", "its",
    "not", "of", "on", "one", "or", "out", "over", "that", "the", "then",
    "this", "to", "up", "use", "used", "using", "what", "when", "which",
    "with", "you", "your",
}


def normalise(word: str) -> str:
    w = word.lower().strip().strip(".,;:()`\"'")
    return w[:-1] if len(w) > 3 and w.endswith("s") else w


def product_vocabulary() -> set[str]:
    """Terms that name a capability, read out of the product.

    Not every word in the help text — that sweeps up ordinary verbs
    ("decide", "change") and turns the check into noise nobody reads.
    A capability has a name: a subcommand, a value a flag accepts, a
    tool. Those three, at every level, and derived rather than declared
    so that a capability which appears in the product appears here
    without anyone remembering to add it.
    """
    binary = os.environ.get("SMIX_BIN", os.path.join(ROOT, "target", "release", "smix"))
    vocab: set[str] = set()
    for tool in _MCP_TOOLS:
        vocab.update(normalise(part) for part in tool["name"].split("_") if len(part) > 2)
    if not os.access(binary, os.X_OK):
        return vocab

    def help_of(argv: list[str]) -> str:
        out = subprocess.run(
            [binary, *argv, "--help"], capture_output=True, text=True, check=False
        )
        return out.stdout + out.stderr

    def subcommands_in(text: str) -> set[str]:
        body = text.split("Commands:", 1)
        if len(body) < 2:
            return set()
        found = set()
        for line in body[1].splitlines():
            if line.strip() and not line.startswith(" "):
                break
            m = re.match(r"\s{2}(\S+)", line)
            if m and m.group(1) != "help":
                found.add(m.group(1))
        return found

    root_help = help_of([])
    top = subcommands_in(root_help)
    vocab.update(normalise(c) for c in top)
    texts = [root_help]
    for sub in sorted(top):
        text = help_of([sub])
        texts.append(text)
        for nested in sorted(subcommands_in(text)):
            vocab.add(normalise(nested))
            texts.append(help_of([sub, nested]))

    for text in texts:
        for values in re.findall(r"\[possible values: ([^\]]+)\]", text):
            vocab.update(normalise(v) for v in values.split(","))
    vocab.discard("")
    return vocab


files = skill_files()
if not files:
    # An empty skills directory would otherwise pass every check below,
    # which is the same as reporting coverage for nothing.
    print("plugin-capability-parity: FAIL")
    print("  - no SKILL.md found under plugin/skills/")
    sys.exit(1)

subcommands = cli_subcommands()
tools = mcp_tools()
vocabulary = product_vocabulary()

cited_commands = 0
cited_tools = 0

for path in files:
    rel = os.path.relpath(path, ROOT)
    text = open(path).read()

    # What the description promises, the body has to teach.
    front = re.match(r"---\n(.*?)\n---\n(.*)", text, re.S)
    if front:
        described = re.search(r"^description:\s*(.+?)(?=\n\S|\Z)", front.group(1), re.S | re.M)
        body = front.group(2).lower()
        if described:
            promised = {normalise(w) for w in re.findall(r"[A-Za-z][A-Za-z-]{2,}", described.group(1))}
            for term in sorted(promised - STOPWORDS):
                if term in vocabulary and term not in body:
                    problems.append(
                        f"{rel} promises \"{term}\" in its description and never mentions it "
                        f"in the body — a reader who came for that is told nothing"
                    )

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
