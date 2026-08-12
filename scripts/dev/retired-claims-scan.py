#!/usr/bin/env python3
"""A sentence an invariant retired must not still be on a surface.

On 2026-08-06 invariant §9 #1 stopped saying "only simulators; any PR
introducing a physical-device code path is rejected outright" and started
saying physical devices are a first-class backend behind three named
guards. The code followed: registration, per-device consent, loud errors
where a phone has no equivalent verb. Two majors later `llms.txt` — the
file agents read before anything else — still opened with "smix is a
simulator-only UI test runner ... against the simulator only, never a
physical device."

Nothing was wrong. Every gate was green. fact-scan compares numbers
against source and those numbers were right; hygiene-scan asks whether
prose reads as internal and it did not. Neither of them can ask the
question this asks, because the answer does not live in the source at
all — it lives in the day a rule changed, and only the rule remembers.

So each entry below carries four things, and the fourth is not decoration:
the wording, the invariant that retired it, the day, and what the rule
used to say. A scanner that reports only "forbidden phrase" invites the
reader to argue with the phrase. One that says "§9 #1 stopped saying this
on 2026-08-06, and here is what it used to say" ends the argument.

Two columns, both by name. GOVERNED is swept; EXEMPT is not, and says
why. A top-level entry in neither fails — silence is how a surface gets
in and stays, which is the same hole that let a Maven coordinate sit
three minor versions stale in a README nobody had added to a list.

`.claude/` is in neither column on purpose. The sweep enumerates tracked
files and the development record is deliberately not version-controlled
(workflow-scan enforces that in both directions), so it is not a surface
this can reach. Exempting it would put a line here that reads as dead on
every machine but the author's.
"""

from __future__ import annotations

import argparse
import fnmatch
import os
import re
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# The register that decides what is retired, and the section in it.
REGISTER = os.path.join(".claude", "CLAUDE.md")
SECTION = "9"

PROSE = (".md", ".mdx", ".txt", ".html", ".ts", ".tsx")


class Retired:
    def __init__(self, invariant: str, number: str, on: str, was: str, phrases: list[str]):
        self.invariant = invariant
        self.number = number
        self.on = on
        self.was = was
        self.phrases = phrases


# Each phrasing here is a claim about the WHOLE product. Scoped ones are
# not listed and must not be: `smix capsule` really is iOS-Simulator-only
# and says so beside the two verbs that are not, which is a true sentence
# that a looser pattern would have deleted.
RETIRED = [
    Retired(
        invariant="§9 #1",
        number="1",
        on="2026-08-06",
        was=(
            "only simulators are supported; any PR introducing a physical-device "
            "code path is rejected outright"
        ),
        phrases=[
            "never a physical device",
            "never a real device",
            "simulator-only ui test runner",
            "against the simulator only",
            "smix never touches a phone",
        ],
    ),
]

# Top-level entries whose prose a reader outside this repository reaches.
# A trailing slash means a directory.
GOVERNED = {
    "docs/": "the published guides",
    "web/": "the site",
    "dashboard/": "the shipped dashboard's copy",
    "plugin/": "skills an agent reads as instructions",
    ".claude-plugin/": "the plugin and marketplace manifests' prose",
    "npm/": "package READMEs, rendered on npmjs.com",
    "crates/": "crate docs, rendered on docs.rs and crates.io",
    "android-runner/": "the SDK README, rendered on Maven Central",
    "swift-bridge/": "the bridge's README",
    "examples/": "the golden-path samples and their prose",
    "test-fixtures/": "fixture apps ship in the checkout and carry prose",
    "README.md": "the front door",
    "CONTRIBUTING.md": "read before anyone changes anything",
    "llms.txt": "what an agent reads first",
    "llms-full.txt": "the same, unabridged",
}

# Not swept, and why. Quoting a retired sentence is the job of some of
# these; the rest have no reader-facing prose at all.
EXEMPT = {
    "CHANGELOG.md": "every release describes the behaviour of its day, correctly",
    "docs/migrating-to-*.md": "a migration guide must say what the old behaviour was",
    "scripts/": "the gates, this scanner among them, name the wording they forbid",
    ".github/": "CI configuration; it invokes the scanners rather than describing smix",
    ".devops/": "deployment plumbing, no prose about behaviour",
    "Cargo.toml": "manifest",
    "Cargo.lock": "lockfile",
    "bun.lock": "lockfile",
    "package.json": "manifest",
    "Package.swift": "manifest",
    "Package.resolved": "lockfile",
    "rust-toolchain.toml": "toolchain pin",
    "deny.toml": "licence policy",
    ".gitignore": "ignore rules",
    "LICENSE-APACHE": "licence text, verbatim and not ours to edit",
    "LICENSE-MIT": "the same",
}


def tracked(root: str) -> list[str]:
    out = subprocess.run(
        ["git", "-C", root, "ls-files"], capture_output=True, text=True, check=False
    )
    if out.returncode != 0:
        return []
    return [p for p in out.stdout.splitlines() if p]


def claimed_by(rel: str, column: dict) -> str | None:
    for entry in column:
        if entry.endswith("/"):
            if rel == entry.rstrip("/") or rel.startswith(entry):
                return entry
        elif "*" in entry:
            if fnmatch.fnmatch(rel, entry):
                return entry
        elif rel == entry:
            return entry
    return None


def register_body(root: str) -> str | None:
    """Section 9 of the invariant register, if this tree has one."""
    path = os.path.join(root, REGISTER)
    if not os.path.isfile(path):
        return None
    text = open(path, encoding="utf-8").read()
    m = re.search(
        rf"^## {SECTION}\..*?(?=^## (?!{SECTION}\.)\d)", text, re.M | re.S
    )
    return m.group(0) if m else ""


def invariant_item(body: str, number: str) -> str | None:
    """One numbered item of the register section, body included."""
    m = re.search(rf"^{number}\. (.*?)(?=^\d+\. |\Z)", body, re.M | re.S)
    return m.group(1) if m else None


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=REPO)
    args = ap.parse_args()
    root = os.path.abspath(args.root)

    problems: list[str] = []
    files = tracked(root)
    if not files:
        print("retired-claims-scan: FAIL")
        print(f"  - no tracked files under {root} — this scan is reading air")
        return 1

    # Every top-level entry is claimed by exactly one column.
    for top in sorted({p.split("/")[0] for p in files}):
        rel = top
        governed = claimed_by(rel, GOVERNED) or claimed_by(rel + "/", GOVERNED)
        exempt = claimed_by(rel, EXEMPT) or claimed_by(rel + "/", EXEMPT)
        if governed and exempt:
            problems.append(
                f"{top} is in both columns — it is either swept or it is not"
            )
        elif not governed and not exempt:
            problems.append(
                f"{top} is claimed by no column. Add it to GOVERNED so its prose "
                f"is swept, or to EXEMPT with the reason it is not — not being "
                f"listed must never be how a surface gets in"
            )

    # A column entry that no longer matches anything is describing a tree
    # that used to be here.
    for column, name in ((GOVERNED, "GOVERNED"), (EXEMPT, "EXEMPT")):
        for entry in column:
            pattern = entry.rstrip("/")
            hit = any(
                p == pattern or p.startswith(pattern + "/") or fnmatch.fnmatch(p, entry)
                for p in files
            )
            if not hit:
                problems.append(
                    f"{name} lists {entry} and nothing in this tree matches it — drop it"
                )

    # The sweep.
    swept = 0
    for rel in files:
        if not rel.endswith(PROSE):
            continue
        if claimed_by(rel, EXEMPT):
            continue
        if not claimed_by(rel, GOVERNED):
            continue
        try:
            lines = open(os.path.join(root, rel), encoding="utf-8").read().splitlines()
        except (OSError, UnicodeDecodeError):
            continue
        swept += 1
        for n, line in enumerate(lines, 1):
            low = line.lower()
            for item in RETIRED:
                for phrase in item.phrases:
                    if phrase in low:
                        problems.append(
                            f'{rel}:{n} still says "{phrase}". {item.invariant} '
                            f"retired that on {item.on}; until then it read: "
                            f"{item.was}. Say what is true now, or move the file "
                            f"to EXEMPT if quoting the old rule is its job."
                        )

    if swept == 0:
        problems.append("no governed prose file was read — this scan is reading air")

    # Each entry is tied to a register that has to still say it. Where the
    # register is not in the tree, that half does not run, and says so.
    body = register_body(root)
    note = ""
    if body is None:
        note = (
            " — the invariant register is not in this checkout, so the entries "
            "below were not reconciled against it"
        )
    else:
        for item in RETIRED:
            text = invariant_item(body, item.number)
            if text is None:
                problems.append(
                    f"{item.invariant} cites §{SECTION} item {item.number} and the "
                    f"register has no such item — the numbering moved under this list"
                )
            elif item.on not in text:
                problems.append(
                    f"{item.invariant} says it was retired on {item.on} and §{SECTION} "
                    f"item {item.number} no longer records that day — one of the two "
                    f"changed without the other"
                )

    if problems:
        print("retired-claims-scan: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    print(
        f"retired-claims-scan: clean — {swept} governed prose file(s), "
        f"{len(RETIRED)} retired wording(s){note}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
