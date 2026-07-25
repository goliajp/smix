#!/usr/bin/env python3
"""Structural checks on the smix Claude Code plugin.

`claude plugin validate` is the authority on the manifest schema and the
e2e runs it. What is here is the part it cannot know: that the paths point
at things in *this* repository, that the version tracks the crates, and
that the component layout follows the rule the plugin docs call the common
mistake — everything except `plugin.json` lives at the plugin root, not
inside `.claude-plugin/`.
"""

import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
PLUGIN = os.path.join(ROOT, "plugin")

problems: list[str] = []


def read_json(path: str) -> dict:
    try:
        with open(path) as fh:
            return json.load(fh)
    except FileNotFoundError:
        problems.append(f"missing: {os.path.relpath(path, ROOT)}")
    except json.JSONDecodeError as e:
        problems.append(f"not JSON: {os.path.relpath(path, ROOT)} — {e}")
    return {}


manifest = read_json(os.path.join(PLUGIN, ".claude-plugin", "plugin.json"))
marketplace = read_json(os.path.join(ROOT, ".claude-plugin", "marketplace.json"))
mcp = read_json(os.path.join(PLUGIN, ".mcp.json"))

# --- the manifest names this plugin, and says which smix it expects ------

for field in ("name", "description", "version"):
    if not manifest.get(field):
        problems.append(f"plugin.json has no {field}")

if manifest.get("name") != "smix":
    problems.append(f"plugin name should be smix, is {manifest.get('name')!r}")

# The readiness hook compares an installed binary against this number, so
# a version that drifts from the crates makes it report skew that is not
# there.
meta = subprocess.run(
    ["cargo", "metadata", "--no-deps", "--format-version", "1"],
    cwd=ROOT,
    capture_output=True,
    text=True,
    check=False,
)
if meta.returncode == 0:
    packages = json.loads(meta.stdout)["packages"]
    crate_version = next(p["version"] for p in packages if p["name"] == "smix-cli")
    if manifest.get("version") != crate_version:
        problems.append(
            f"plugin version {manifest.get('version')} != workspace {crate_version}; "
            "the readiness hook compares against it"
        )
else:
    problems.append("cargo metadata failed, so the version could not be checked")

# --- the MCP server is named, not located --------------------------------

servers = mcp.get("mcpServers", {})
if list(servers) != ["smix"]:
    problems.append(f".mcp.json should declare one server named smix, has {list(servers)}")
command = servers.get("smix", {}).get("command")
if command != "smix-mcp":
    problems.append(
        f".mcp.json command should be the bare name smix-mcp, is {command!r} — "
        "where a user installed it is not ours to assume"
    )

# --- component layout ----------------------------------------------------

# The plugin docs single this out: only plugin.json belongs inside
# .claude-plugin/. A hooks/ or skills/ directory placed there is silently
# not loaded, which looks exactly like a plugin that does nothing.
inside = os.path.join(PLUGIN, ".claude-plugin")
if os.path.isdir(inside):
    stray = [e for e in os.listdir(inside) if e != "plugin.json"]
    if stray:
        problems.append(f".claude-plugin/ should hold only plugin.json, also has {stray}")

hooks_path = os.path.join(PLUGIN, "hooks", "hooks.json")
hooks = read_json(hooks_path)
events = hooks.get("hooks", {})
if "SessionStart" not in events:
    problems.append("hooks.json does not wire SessionStart")
else:
    wired = json.dumps(events["SessionStart"])
    if "${CLAUDE_PLUGIN_ROOT}" not in wired:
        problems.append(
            "the SessionStart hook command does not use ${CLAUDE_PLUGIN_ROOT}; "
            "a path relative to the working directory is wrong for an installed plugin"
        )
    if "readiness.sh" not in wired:
        problems.append("the SessionStart hook does not run readiness.sh")

readiness = os.path.join(PLUGIN, "scripts", "readiness.sh")
if not os.access(readiness, os.X_OK):
    problems.append("plugin/scripts/readiness.sh is not executable; hooks are exec'd")

# --- the marketplace points at something real ----------------------------

entries = marketplace.get("plugins", [])
if not entries:
    problems.append("marketplace.json lists no plugins")
for entry in entries:
    source = entry.get("source")
    if not isinstance(source, str):
        problems.append(f"marketplace entry {entry.get('name')!r} has no string source")
        continue
    target = os.path.normpath(os.path.join(ROOT, source))
    if not os.path.isdir(target):
        problems.append(f"marketplace source {source!r} is not a directory in this repo")

if problems:
    print("plugin-structure: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print(f"plugin-structure: clean — {len(entries)} marketplace entry, plugin v{manifest['version']}")
