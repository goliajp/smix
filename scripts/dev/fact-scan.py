#!/usr/bin/env python3
"""Check that user-facing surfaces state facts the source agrees with.

hygiene-scan answers "does this read as internal?"; this answers "is it
true?". The gap between the two is how a landing page shipped a tool
count double the real one and three install coordinates nobody could
install: hygiene-scan strips quoted spans before matching, and every
visitor-visible word on a web page is a quoted span, so the whole
marketing surface was structurally exempt — and no gate compared any
number against the code it described.

Checks:
  1. Version coordinates — every version-bearing install coordinate on a
     user-facing surface equals the workspace version.
  2. Tool-count claims — every "N tools" / "N MCP tools" claim equals
     the number of #[tool(...)] registrations in smix-mcp.
  3. User-facing string noise — the hygiene noise patterns, run WITHOUT
     quote-stripping, over the surfaces whose quoted strings are the
     product (web/, dashboard/ sources).
  4. Installability — the DEPLOYED site tells visitors to install the
     workspace version. If no release tag for it exists, the site is
     promising a package the registries do not serve, so the page must
     say so. Checks 1-3 all passed while smix.golia.jp shipped
     "npm install @goliapkg/smix@2.0.0" against a registry whose latest
     was 1.0.27: matching the workspace version is not the same as being
     installable.

Exit non-zero on any mismatch, so it can gate a release.
"""

import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))


def read(rel):
    with open(os.path.join(ROOT, rel), encoding="utf-8") as f:
        return f.read()


def workspace_version():
    m = re.search(r'^version = "([^"]+)"', read("Cargo.toml"), re.M)
    if not m:
        raise SystemExit("fact-scan: no workspace version in Cargo.toml")
    return m.group(1)


# Each entry: (file, regex with ONE capture group = the version it states).
# A file listed here that stops matching is an error, not a skip — a
# coordinate that silently vanishes from the check is how drift starts.
VERSION_COORDINATES = [
    ("README.md", r'jp\.golia\.smix:smix-sdk:([0-9][0-9a-zA-Z.\-]*)'),
    ("README.md", r'from: "([0-9][0-9a-zA-Z.\-]*)"'),
    ("npm/smix-rn/package.json", r'"version":\s*"([^"]+)"'),
    (
        "android-runner/app/src/main/kotlin/dev/smix/runner/SmixRunner.kt",
        r'const val VERSION[^=]*= "([^"]+)"',
    ),
    ("android-runner/sdk/build.gradle.kts", r'val mavenCentralVersion = "([^"]+)"'),
    ("web/src/data/site.ts", r'@goliapkg/smix@([0-9][0-9a-zA-Z.\-]*)'),
    ("web/src/data/site.ts", r'jp\.golia\.smix:smix-sdk:([0-9][0-9a-zA-Z.\-]*)'),
    ("llms.txt", r'jp\.golia\.smix:smix-sdk:([0-9][0-9a-zA-Z.\-]*)'),
]

TOOL_COUNT_CLAIM = re.compile(r"\b(\d+)\s+(?:MCP\s+)?tools\b", re.I)

# Surfaces whose quoted strings are user-visible copy. Checked raw.
STRING_SURFACES = ("web/src", "dashboard/src", "dashboard/index.html")
STRING_EXTS = (".ts", ".tsx", ".mdx", ".html")

NOISE = {
    "consumer-name": re.compile(r"\binsight\b", re.I),
    "consumer-bundle": re.compile(r"focusai", re.I),
    "ticket-ref": re.compile(r"gol-611", re.I),
    "version-cluster": re.compile(r"v\d+\.\d+(\.\d+)?\s*(c\d|Cluster|Phase)", re.I),
    "round-ref": re.compile(r"\bround-\d"),
    "ask-ref": re.compile(r"\bAsk \d"),
    "plan-ref": re.compile(r"plan-(cold|hot)"),
}

# Where tool-count claims are fair to make and must be true.
TOOL_CLAIM_SURFACES = [
    "README.md",
    "llms.txt",
    "crates/smix-mcp/README.md",
]


def release_tag_exists(version):
    """Is there a tag for this version? Tagging is what publishes."""
    result = subprocess.run(
        ["git", "tag", "--list", f"v{version}", f"swift-v{version}"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    return bool(result.stdout.strip())


# Wording that tells a visitor the version is not out yet. The deployed
# site must carry one of these while the tag is missing.
UNRELEASED_MARKERS = ("not published yet", "unreleased", "pre-release")


def mcp_tool_count():
    return read("crates/smix-mcp/src/main.rs").count("#[tool(")


def iter_surface_files():
    for base in STRING_SURFACES:
        full = os.path.join(ROOT, base)
        if os.path.isfile(full):
            yield base
            continue
        for dirpath, _, names in os.walk(full):
            for name in names:
                if name.endswith(STRING_EXTS):
                    rel = os.path.relpath(os.path.join(dirpath, name), ROOT)
                    yield rel


def main():
    failures = []
    version = workspace_version()

    for rel, pattern in VERSION_COORDINATES:
        text = read(rel)
        stated = re.findall(pattern, text)
        if not stated:
            failures.append(
                f"{rel}: expected a version coordinate matching /{pattern}/ "
                f"and found none — the coordinate moved and this check went blind"
            )
            continue
        for v in stated:
            if v != version:
                failures.append(
                    f"{rel}: states version {v}, workspace is {version} (/{pattern}/)"
                )

    tools = mcp_tool_count()
    if tools == 0:
        failures.append("crates/smix-mcp/src/main.rs: zero #[tool(] found — extraction broke")
    for rel in TOOL_CLAIM_SURFACES + [f for f in iter_surface_files()]:
        path = os.path.join(ROOT, rel)
        if not os.path.isfile(path):
            continue
        for lineno, line in enumerate(read(rel).splitlines(), 1):
            for m in TOOL_COUNT_CLAIM.finditer(line):
                if int(m.group(1)) != tools:
                    failures.append(
                        f"{rel}:{lineno}: claims {m.group(1)} tools, smix-mcp registers {tools}"
                    )

    for rel in iter_surface_files():
        for lineno, line in enumerate(read(rel).splitlines(), 1):
            for name, pattern in NOISE.items():
                if pattern.search(line):
                    failures.append(f"{rel}:{lineno}: {name} in user-facing copy")

    if not release_tag_exists(version):
        site = "\n".join(read(rel) for rel in iter_surface_files() if rel.startswith("web/"))
        if not any(marker in site.lower() for marker in UNRELEASED_MARKERS):
            failures.append(
                f"web/: the site states {version} install coordinates, no release "
                f"tag for {version} exists, and no copy says so — visitors are "
                f"told to install a version the registries do not serve"
            )

    if failures:
        print(f"fact-scan: {len(failures)} falsehood(s) on user-facing surfaces")
        for f in failures:
            print(f"  {f}")
        return 1
    print(
        f"fact-scan: clean — coordinates all say {version}, "
        f"tool claims all say {tools}, no noise in user-facing strings"
    )
    return 0



if __name__ == "__main__":
    sys.exit(main())
