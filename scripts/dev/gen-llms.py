#!/usr/bin/env python3
"""Generate llms.txt + llms-full.txt from source, and gate their freshness.

The AI-facing index is a *projection* of single sources of truth — never a
hand-maintained file that drifts:

  - the verb table is read from `smix_verbs::VERB_TABLE` (the same table a
    reviewer test forces every parser-dispatched verb to appear in),
  - the selector taxonomy is read from the `Selector` enum,
  - the Maven coordinate's version is read from the workspace `Cargo.toml`,
  - `llms-full.txt` concatenates an explicit list of the evergreen guides.

This mirrors `route-conformance.py` (regex over Rust source, no restated
list) and `ffi-bindings-fresh.sh` (regenerate + diff). `--check` regenerates
in memory and diffs against the committed files, exiting non-zero when they
differ so a stale index cannot ship.

If generation reads fewer than the known-minimum rows from a source it FAILS
rather than writing a near-empty index that would pass by knowing nothing.

Usage:
    python3 scripts/dev/gen-llms.py            # write llms.txt + llms-full.txt
    python3 scripts/dev/gen-llms.py --check     # diff committed vs regenerated
"""

import os
import re
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

VERBS_RS = "crates/smix-verbs/src/lib.rs"
SELECTOR_RS = "crates/smix-selector/src/lib.rs"
CARGO_TOML = "Cargo.toml"

LLMS = "llms.txt"
LLMS_FULL = "llms-full.txt"

# Shape guards — if a source is refactored so the regex reads far fewer rows
# than the table actually holds, fail loudly instead of shipping a stub.
MIN_VERBS = 30
MIN_SELECTORS = 8

# The evergreen guides, concatenated into llms-full.txt in this order. An
# EXPLICIT list, deliberately excluding the consumer-correspondence files
# (the `insight-*` / `gol-611-*` notes) that share the docs/ai-guide
# directory — the index is the durable surface, not the dogfood log.
FULL_INCLUDE = [
    "docs/ai-guide/01-quickstart.md",
    "docs/ai-guide/02-yaml-reference.md",
    "docs/ai-guide/03-selectors.md",
    "docs/ai-guide/04-actions.md",
    "docs/ai-guide/05-cli.md",
    "docs/ai-guide/06-fixtures.md",
    "docs/ai-guide/07-errors.md",
    "docs/ai-guide/08-cookbook.md",
    "docs/ai-guide/09-sessions.md",
    "docs/ai-guide/10-ai-assertions.md",
    "docs/ai-guide/11-mcp.md",
    "docs/ai-guide/wire-format.md",
    "docs/ai-guide/abi-stability.md",
    "docs/ai-guide/verb-parity.md",
]

# `v("tapOn", "tap", VerbCategory::Tap, ArgShape::Selector)` — rustfmt wraps
# these across lines, so the pattern spans whitespace. The `const fn v(...)`
# definition never matches: its first argument is `maestro_name`, not a
# quoted literal.
VERB_ROW = re.compile(
    r'v\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*,'
    r'\s*VerbCategory::(\w+)\s*,\s*ArgShape::(\w+)',
)

# A `Name {` variant line at 4-space indent inside the enum body.
SELECTOR_VARIANT = re.compile(r"^    ([A-Z][A-Za-z]*)\s*\{")


def read(rel):
    with open(os.path.join(ROOT, rel), encoding="utf-8") as fh:
        return fh.read()


def workspace_version():
    for line in read(CARGO_TOML).splitlines():
        m = re.match(r'^version\s*=\s*"([^"]+)"', line)
        if m:
            return m.group(1)
    sys.exit("gen-llms: no workspace version in Cargo.toml")


def read_verbs():
    """Every (maestro, smix, category, arg_shape) row in VERB_TABLE order."""
    src = read(VERBS_RS)
    start = src.index("pub static VERB_TABLE")
    end = src.index("];", start)
    rows = VERB_ROW.findall(src[start:end])
    if len(rows) < MIN_VERBS:
        sys.exit(
            f"gen-llms: read only {len(rows)} verbs from VERB_TABLE "
            f"(< {MIN_VERBS}) — the table shape moved; refusing to write a "
            "stub index"
        )
    return rows


def read_selectors():
    """Every Selector variant + its first doc line, in enum order."""
    src = read(SELECTOR_RS)
    start = src.index("pub enum Selector {")
    body = src[start:]
    variants = []
    pending_doc = None
    prev_was_doc = False
    for line in body.splitlines()[1:]:
        stripped = line.strip()
        if stripped.startswith("///"):
            if not prev_was_doc:
                pending_doc = stripped[3:].strip()
            prev_was_doc = True
            continue
        m = SELECTOR_VARIANT.match(line)
        if m:
            variants.append((m.group(1), pending_doc or ""))
            pending_doc = None
            prev_was_doc = False
            continue
        if stripped == "}" and not line.startswith(" "):
            break
        prev_was_doc = False
    if len(variants) < MIN_SELECTORS:
        sys.exit(
            f"gen-llms: read only {len(variants)} Selector variants "
            f"(< {MIN_SELECTORS}) — the enum shape moved; refusing to write "
            "a stub index"
        )
    return variants


def gen_llms():
    version = workspace_version()
    verbs = read_verbs()
    selectors = read_selectors()

    out = []
    out.append("# smix")
    out.append("")
    out.append(
        "> smix is a simulator-only UI test runner for iOS and Android, "
        "built as a Claude Code sub-product. Tests are authored in "
        "maestro-compatible yaml (or the Rust / TypeScript / Swift / Kotlin "
        "SDKs) and driven through an accessibility-first sensing stack — "
        "a11y tree, Vision OCR, then a local-`claude` AI-assertion tier — "
        "against the simulator only, never a physical device. Failures are "
        "AI-readable: every miss carries visible elements and suggested fixes."
    )
    out.append("")
    out.append(
        "This file is generated by `scripts/dev/gen-llms.py` from "
        "`smix_verbs::VERB_TABLE`, the `Selector` enum, and the workspace "
        "version. Do not edit by hand — the ship gate regenerates and diffs it."
    )
    out.append("")

    out.append("## Verbs")
    out.append("")
    out.append(
        f"The canonical yaml verb table ({len(verbs)} entries). Each verb "
        "carries the maestro-surface name test authors know and the "
        "smix-canonical form.")
    out.append("")
    out.append("| maestro | smix | category | arg shape |")
    out.append("| --- | --- | --- | --- |")
    for maestro, smix, category, shape in verbs:
        out.append(f"| `{maestro}` | `{smix}` | {category} | {shape} |")
    out.append("")

    out.append("## Selectors")
    out.append("")
    out.append(
        f"The `Selector` taxonomy ({len(selectors)} variants) — the target "
        "language for every find / tap / assert. The base forms resolve on "
        "the a11y tree; the later layers add locale text, Vision OCR, "
        "anchor-relative coordinates, a raw normalized point, and a "
        "first-hit fallback chain. See `docs/ai-guide/03-selectors.md`.")
    out.append("")
    for name, doc in selectors:
        if doc:
            out.append(f"- **{name}** — {doc}")
        else:
            out.append(f"- **{name}**")
    out.append("")

    out.append("## Install")
    out.append("")
    out.append("```bash")
    out.append("# Rust CLI + SDK")
    out.append("cargo install smix-cli --locked")
    out.append("")
    out.append("# TypeScript / Node / Bun")
    out.append("npm install @goliapkg/smix")
    out.append("")
    out.append("# Swift Package Manager")
    out.append(
        "# add https://github.com/goliajp/smix "
        f'(product: SmixSDK, from: "{version}")')
    out.append("")
    out.append("# Gradle / Maven (Kotlin / Java) — current release:")
    out.append(f'# implementation("jp.golia.smix:smix-sdk:{version}")')
    out.append("```")
    out.append("")
    out.append(
        "Prerequisites: macOS with Xcode + Simulator (iOS); Android SDK with "
        "an emulator image (Android).")
    out.append("")

    out.append("## MCP")
    out.append("")
    out.append(
        "smix exposes the simulator to an agent over MCP — launch, look, tap, "
        "type, assert, with no yaml in between. Bring the runner up first "
        "(`smix runner up <udid> --bundle <id>`); the MCP server talks to the "
        "runner, it does not start it. Full setup + client JSON config: "
        "`docs/ai-guide/11-mcp.md`.")
    out.append("")

    out.append("## Iron rules")
    out.append("")
    out.append(
        "- **Simulator only.** Any physical-device code path is rejected by "
        "construction.")
    out.append(
        "- **Sensing is a11y-first, then OCR, then AI.** No multi-provider "
        "VLM abstraction — the AI tier is a local `claude` CLI.")
    out.append(
        "- **No coordinates or xpath in the selector surface.** The one "
        "escape hatch is `App::tap_at_coord(nx, ny)` — a normalized-0..1 tap "
        "wired directly to the native event chain, not a Selector.")
    out.append("- **No bare `sleep` verb.**")
    out.append(
        "- **Failures must be AI-readable** — every failure carries visible "
        "elements and suggestions.")
    out.append("")

    out.append("## Guides")
    out.append("")
    out.append("- [Quickstart](docs/ai-guide/01-quickstart.md): first flow, first run")
    out.append("- [YAML reference](docs/ai-guide/02-yaml-reference.md): the full verb grammar")
    out.append("- [Selectors](docs/ai-guide/03-selectors.md): the target taxonomy in depth")
    out.append("- [Actions](docs/ai-guide/04-actions.md): tap / fill / scroll / OCR fallback semantics")
    out.append("- [CLI](docs/ai-guide/05-cli.md): `smix run`, `smix runner`, `smix diagnostic`")
    out.append("- [Fixtures](docs/ai-guide/06-fixtures.md): the host-app fixture contract")
    out.append("- [Errors](docs/ai-guide/07-errors.md): the AI-readable failure format")
    out.append("- [Cookbook](docs/ai-guide/08-cookbook.md): recipes for common flows")
    out.append("- [Sessions](docs/ai-guide/09-sessions.md): session lifecycle across the runner")
    out.append("- [AI assertions](docs/ai-guide/10-ai-assertions.md): the fenced `assertWithAI` tier")
    out.append("- [MCP](docs/ai-guide/11-mcp.md): drive the simulator over MCP")
    out.append("- [Wire format](docs/ai-guide/wire-format.md): the runner wire schema")
    out.append("- [ABI stability](docs/ai-guide/abi-stability.md): the FFI + wire compatibility contract")
    out.append("- [Verb parity](docs/ai-guide/verb-parity.md): maestro ↔ smix ↔ platform coverage")
    out.append("")

    return "\n".join(out)


def gen_llms_full():
    out = []
    out.append("# smix — evergreen guides (full text)")
    out.append("")
    out.append(
        "Generated by `scripts/dev/gen-llms.py`. The evergreen AI-facing "
        "guides, concatenated for a single-file read. Consumer-correspondence "
        "notes are deliberately excluded.")
    out.append("")
    for rel in FULL_INCLUDE:
        out.append("")
        out.append(f"<!-- ===== {rel} ===== -->")
        out.append("")
        out.append(read(rel).rstrip("\n"))
        out.append("")
    return "\n".join(out) + "\n"


def write_file(rel, content):
    with open(os.path.join(ROOT, rel), "w", encoding="utf-8") as fh:
        fh.write(content if content.endswith("\n") else content + "\n")


def check_one(rel, generated):
    path = os.path.join(ROOT, rel)
    if not os.path.exists(path):
        print(f"gen-llms: {rel} does not exist — run `gen-llms.py` and commit")
        return False
    want = generated if generated.endswith("\n") else generated + "\n"
    have = open(path, encoding="utf-8").read()
    if have != want:
        print(
            f"gen-llms: {rel} is stale — a source it projects (VERB_TABLE / "
            "Selector / version / a guide) moved. Run `gen-llms.py` and commit."
        )
        return False
    return True


def main():
    check = "--check" in sys.argv
    llms = gen_llms()
    full = gen_llms_full()

    if check:
        ok = check_one(LLMS, llms)
        ok = check_one(LLMS_FULL, full) and ok
        if ok:
            print("gen-llms: fresh — llms.txt + llms-full.txt match source")
            return 0
        return 1

    write_file(LLMS, llms)
    write_file(LLMS_FULL, full)
    verbs = read_verbs()
    selectors = read_selectors()
    print(
        f"gen-llms: wrote {LLMS} ({len(verbs)} verbs, {len(selectors)} "
        f"selectors) + {LLMS_FULL} ({len(FULL_INCLUDE)} guides)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
