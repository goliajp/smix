#!/usr/bin/env python3
"""A sentence a changed rule retired must not still be on a surface.

On 2026-08-06 smix stopped being simulator-only: physical devices became
a first-class backend behind three guards (registration, per-device
consent, loud errors where a phone has no equivalent verb). Two majors
later `llms.txt` — the file agents read before anything else — still
opened with "smix is a simulator-only UI test runner ... against the
simulator only, never a physical device."

Nothing was wrong by any other gate. fact-scan compares numbers against
source and those numbers were right; hygiene-scan asks whether prose
reads as internal and it did not. Neither can ask this question, because
the answer is not in the source — it is in the day a rule changed.

So each entry below carries the wording, the day it was retired, and what
the rule used to say. A scanner that reports only "forbidden phrase"
invites the reader to argue with the phrase; one that says "this stopped
being true on 2026-08-06, and here is what it used to say" ends it.

Two columns, both by name. GOVERNED is swept; EXEMPT is not, and says
why. A top-level entry in neither fails — silence is how a surface gets
in and stays.
"""

from __future__ import annotations

import argparse
import fnmatch
import os
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
PROSE = (".md", ".mdx", ".txt", ".html", ".ts", ".tsx")


class Retired:
    def __init__(self, on: str, was: str, phrases: list[str]):
        self.on = on
        self.was = was
        self.phrases = phrases


# Each phrasing here is a claim about the WHOLE product. Scoped ones are
# not listed and must not be: `smix capsule` really is iOS-Simulator-only
# and says so beside the two verbs that are not, which is a true sentence
# that a looser pattern would have deleted.
RETIRED = [
    Retired(
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
                            f'{rel}:{n} still says "{phrase}". That stopped being '
                            f"true on {item.on}; until then the rule read: "
                            f"{item.was}. Say what is true now, or move the file "
                            f"to EXEMPT if quoting the old rule is its job."
                        )

    if swept == 0:
        problems.append("no governed prose file was read — this scan is reading air")

    if problems:
        print("retired-claims-scan: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    print(
        f"retired-claims-scan: clean — {swept} governed prose file(s), "
        f"{len(RETIRED)} retired wording(s)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
