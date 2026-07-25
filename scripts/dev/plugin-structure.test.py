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

# --- the guards are shipped, and are the ones this repo runs -------------

# Two copies of a guard drift, and the one that drifts is the copy nobody
# runs the tests against. So the plugin holds the implementation and this
# repo's own hook config points at it: a regression reaches us before it
# reaches anyone who installed the plugin.
for script in ("sim-guard.sh", "adb-guard.sh", "hook-command.py"):
    shipped = os.path.join(PLUGIN, "scripts", script)
    if not os.access(shipped, os.X_OK) and script.endswith(".sh"):
        problems.append(f"plugin/scripts/{script} is missing or not executable")
    elif not os.path.exists(shipped):
        problems.append(f"plugin/scripts/{script} is missing")
    stale = os.path.join(ROOT, "scripts", "dev", script)
    if os.path.exists(stale):
        problems.append(
            f"scripts/dev/{script} still exists alongside the plugin copy — "
            "two of these drift, and the tests only cover one"
        )

settings_path = os.path.join(ROOT, ".claude", "settings.json")
settings = read_json(settings_path)
wired = json.dumps(settings)
for script in ("sim-guard.sh", "adb-guard.sh"):
    if f"plugin/scripts/{script}" not in wired:
        problems.append(
            f"this repo's own hooks do not run plugin/scripts/{script}; "
            "we would be shipping a guard we do not use"
        )

pre = hooks.get("hooks", {}).get("PreToolUse", [])
pre_json = json.dumps(pre)
if not pre:
    problems.append("hooks.json does not wire PreToolUse, so the guards ship inert")
else:
    if '"matcher": "Bash"' not in pre_json.replace('"matcher":"Bash"', '"matcher": "Bash"'):
        problems.append("the PreToolUse hook does not match Bash")
    for script in ("sim-guard.sh", "adb-guard.sh"):
        if script not in pre_json:
            problems.append(f"PreToolUse does not run {script}")
    if "${CLAUDE_PLUGIN_ROOT}" not in pre_json:
        problems.append("the PreToolUse commands do not use ${CLAUDE_PLUGIN_ROOT}")

# --- skills and the monitor ----------------------------------------------

skills_dir = os.path.join(PLUGIN, "skills")
skills = sorted(
    d for d in os.listdir(skills_dir) if os.path.isdir(os.path.join(skills_dir, d))
) if os.path.isdir(skills_dir) else []
if not skills:
    problems.append("plugin/skills/ has no skills")
for name in skills:
    doc = os.path.join(skills_dir, name, "SKILL.md")
    if not os.path.exists(doc):
        problems.append(f"skills/{name}/ has no SKILL.md")
        continue
    head = open(doc).read().split("---")
    # Frontmatter is how Claude Code decides when to invoke a skill; without
    # a description it loads and is never chosen.
    if len(head) < 3 or "description:" not in head[1]:
        problems.append(f"skills/{name}/SKILL.md has no frontmatter description")

monitors_path = os.path.join(PLUGIN, "monitors", "monitors.json")
try:
    monitors = json.load(open(monitors_path))
except FileNotFoundError:
    monitors = []
    problems.append("missing: plugin/monitors/monitors.json")
except json.JSONDecodeError as e:
    monitors = []
    problems.append(f"monitors.json is not JSON — {e}")
for entry in monitors:
    for field in ("name", "command", "description"):
        if not entry.get(field):
            problems.append(f"a monitor entry has no {field}")
    if "${CLAUDE_PLUGIN_ROOT}" not in entry.get("command", ""):
        problems.append(f"monitor {entry.get('name')!r} does not use ${{CLAUDE_PLUGIN_ROOT}}")
    # Unscoped, the watch runs for every session the plugin is enabled in,
    # including ones that never touch a device.
    if not entry.get("when"):
        problems.append(f"monitor {entry.get('name')!r} has no `when`, so it runs always")

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
