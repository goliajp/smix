#!/usr/bin/env python3
"""Scan shipped sources for development-process noise and dead doc pointers.

Exits non-zero when either is found, so it can gate a release.

Scope is "what a reader outside this repo can actually see": the file list
comes from git, so the local-only dev docs are excluded, and a link whose
target is gitignored counts as dead — an outside reader who clones the
repo and follows it lands nowhere.

Noise is version/checkpoint tags, internal plan-section refs, in-house
consumer names, and CJK fragments — none of which mean anything outside.

Everything is matched on prose, with quoted spans removed first.

Two reasons, both about telling data apart from chatter. The codebase
legitimately contains CJK *data*: localizedText fixtures assert on real
Japanese and Chinese strings, and the doc examples for that feature quote
those same strings. And code that *checks* for noise necessarily contains
the noise words — `assert!(!d.contains("insight"))` is the fix, not the
defect. Quoted is data; unquoted is chatter.

Usage:
    python3 scripts/dev/hygiene-scan.py [--verbose] [--noise-only]

`--noise-only` gates on comment noise alone. The two problems are cleaned
on different tracks: comment noise is a source sweep, while dead pointers
live in prose (the changelog and the RFC record cite the in-house
consumer's private feedback docs) and need an editorial pass, not a
find-and-delete.
"""

import collections
import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

# Every shipped file is swept. Skipping one takes an entry here, with a
# reason — the list is printed on every run, so narrowed coverage is stated
# rather than assumed.
#
# The scope used to be an allowlist of directory-and-extension pairs, which
# reported clean while 167 hits sat in plain sight: the whole npm SDK, the
# Gradle scripts and Android manifest beside the Kotlin they configure, the
# FFI interface definition beside the Rust, and a Cargo.toml that ships to
# crates.io. An allowlist has to be told about each new file type; what a
# reader sees does not wait for that.
# Each entry's remaining hits are counted and printed, so an exclusion reads
# as a debt with an owner rather than as ground that was never walked.
EXCLUSIONS = [
    ("docs/roadmap.md", "the version path is its subject"),
    ("docs/plan-hot.md", "checkpoint tags are its subject"),
    ("docs/v2.md", "checkpoint tags are its subject"),
    ("docs/plan-cold/", "checkpoint tags are their subject"),
    ("docs/plan-history/", "archived plans, kept as written"),
    ("docs/dogfood-archive/", "archived consumer correspondence, kept as written"),
    (".claude/rfcs/", "design records written per consumer; editorial pass"),
    (".claude/CLAUDE.md", "the development charter — written in the language "
                          "its readers work in, and its references to "
                          "plan-hot.md are structural (that file exists only "
                          "between checkpoints)"),
    (".claude/rule/", "project rule cards, same charter and same language"),
    (".gitignore", "names the consumer docs it ignores; the rule goes when "
                   "they do"),
]

# git already drops anything gitignored, which is what defines "shipped" —
# notably `.claude/` is ignored EXCEPT `.claude/rfcs/`, the tracked
# design-decision record, so citations of it must still resolve.
SKIP_PREFIXES = ("target/", "build/", ".build/", "node_modules/")

NOISE_PATTERNS = {
    "version-cluster": re.compile(r"v\d+\.\d+(\.\d+)?\s*(c\d|Cluster|Phase)", re.I),
    "phase-tag": re.compile(r"\bPhase [A-Z]\b"),
    "cluster-tag": re.compile(r"\bCluster [A-Z]\b"),
    "cluster-suffix": re.compile(r"\bc5i\b"),
    "consumer-name": re.compile(r"\binsight\b", re.I),
    "round-ref": re.compile(r"\bround-\d"),
    "ask-ref": re.compile(r"\bAsk \d"),
    "plan-ref": re.compile(r"plan-(cold|hot)"),
    "ts-provenance": re.compile(r"now-retired TS|src/core/\S+\.ts|\.ts:\d+"),
}

CJK = re.compile(r"[一-鿿]")
COMMENT_START = re.compile(r"(?<!:)//")
QUOTED_SPAN = re.compile(r"\"[^\"]*\"|'[^']*'|`[^`]*`")

# `[label](some/path.md)` — resolved relative to the citing file.
MD_LINK = re.compile(r"\]\(([^)\s]+\.md)(?:#[^)]*)?\)")
# `SOME_DOC.md` in prose — matched by basename, so the per-crate files
# that legitimately repeat (BUDGETS.md, README.md) stay quiet.
MD_BARE = re.compile(r"`([A-Za-z0-9_.\-/]+\.md)`")

POINTER_EXTS = (".rs", ".swift", ".kt", ".md")

# Planning docs name deliverables that do not exist yet — that is their
# job. An unresolved name there is a plan, not a dead pointer. Archived
# plans under "plan-history/" are read the same way: each was written
# pointing at the hot plan it would become, or the next segment's, and is
# kept as written — the noise scan already excludes it for the same reason.
#
# CLAUDE.md sits one level above those: it is the spec the planning docs
# are written against, so it names `docs/plan-hot.md` (which exists only
# between checkpoints, by design) and `plan-cold/v0.X.md` (a shape, not a
# path). Reading either as a citation gets the direction backwards.
POINTER_SKIP = (
    ".claude/CLAUDE.md",
    "docs/roadmap.md",
    "docs/plan-hot.md",
    "docs/v2.md",
    "docs/plan-cold/",
    "docs/plan-history/",
    "docs/dogfood-archive/",
)


def shipped_files():
    """Every file a reader gets: tracked, plus new ones not gitignored."""
    result = subprocess.run(
        ["git", "ls-files", "-c", "-o", "--exclude-standard"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return [
        rel
        for rel in result.stdout.splitlines()
        if rel and not rel.startswith(SKIP_PREFIXES)
    ]


def comment_text(line):
    """Return the comment portion of a line, or None when there is none.

    The `(?<!:)` guard keeps `https://` from reading as a comment.
    """
    stripped = line.strip()
    if stripped.startswith(("//", "/*", "*", "#")):
        return stripped
    match = COMMENT_START.search(line)
    return line[match.start():] if match else None


def area_for(rel):
    parts = rel.split("/")
    return "crates/" + parts[1] if parts[0] == "crates" else parts[0]


def read_lines(rel):
    try:
        return open(os.path.join(ROOT, rel), encoding="utf-8").read().splitlines()
    except (OSError, UnicodeDecodeError):
        return []


def noise_in(rel):
    """Count each kind of noise in one file."""
    found = collections.Counter()
    for line in read_lines(rel):
        unquoted = QUOTED_SPAN.sub("", line)
        for name, pattern in NOISE_PATTERNS.items():
            found[name] += len(pattern.findall(unquoted))
        comment = comment_text(line)
        if comment and CJK.search(QUOTED_SPAN.sub("", comment)):
            found["cjk-comment"] += 1
    return +found


def excluded_by(rel):
    for prefix, _ in EXCLUSIONS:
        if rel.startswith(prefix):
            return prefix
    return None


def scan_noise(files):
    """Sweep every file, keeping the excluded ones' hits as a stated debt."""
    per_area = collections.defaultdict(collections.Counter)
    per_file = collections.Counter()
    totals = collections.Counter()
    owed = collections.Counter()

    for rel in files:
        found = noise_in(rel)
        if not found:
            continue
        prefix = excluded_by(rel)
        if prefix is not None:
            owed[prefix] += sum(found.values())
            continue
        per_area[area_for(rel)].update(found)
        per_file[rel] += sum(found.values())
        totals.update(found)

    return per_area, per_file, totals, owed


def scan_dead_pointers(files):
    """Find .md citations whose target does not ship."""
    shipped = set(files)
    shipped_names = {os.path.basename(f) for f in files if f.endswith(".md")}

    dead = []
    for rel in files:
        if not rel.endswith(POINTER_EXTS) or rel.startswith(POINTER_SKIP):
            continue
        base_dir = os.path.dirname(rel)
        for lineno, line in enumerate(read_lines(rel), 1):
            for match in MD_LINK.finditer(line):
                target = match.group(1)
                if target.startswith(("http://", "https://")):
                    continue
                resolved = os.path.normpath(os.path.join(base_dir, target))
                if resolved not in shipped:
                    dead.append((rel, lineno, target))
            for match in MD_BARE.finditer(line):
                target = match.group(1)
                if os.path.basename(target) not in shipped_names:
                    dead.append((rel, lineno, target))
    return dead


def main():
    verbose = "--verbose" in sys.argv
    noise_only = "--noise-only" in sys.argv
    files = shipped_files()
    per_area, per_file, totals, owed = scan_noise(files)
    dead = [] if noise_only else scan_dead_pointers(files)
    grand = sum(totals.values())

    for path, reason in EXCLUSIONS:
        print(f"hygiene-scan: {owed[path]:4d} left in {path} — not swept ({reason})")

    if grand == 0 and not dead:
        scope = "no development noise" if noise_only else "no development noise, no dead doc pointers"
        print(f"hygiene-scan: clean — {scope}")
        return 0

    if dead:
        by_file = collections.Counter(rel for rel, _, _ in dead)
        print(f"hygiene-scan: {len(dead)} dead doc pointer(s) in {len(by_file)} file(s)")
        for citing, count in by_file.most_common():
            print(f"  {count:4d}  {citing}")
        if verbose:
            print()
            for rel, lineno, target in dead:
                print(f"  {rel}:{lineno} → {target}")
        print()

    if grand:
        print(f"hygiene-scan: {grand} noise occurrences in {len(per_area)} areas\n")
        for name, count in totals.most_common():
            print(f"  {name:16s} {count}")
        print()
        for area, counts in sorted(per_area.items(), key=lambda kv: -sum(kv[1].values())):
            print(f"  {area:34s} {sum(counts.values()):5d}  {dict(counts)}")
        if verbose:
            print("\nworst files:")
            for rel, count in per_file.most_common(30):
                print(f"  {count:4d}  {rel}")

    return 1


if __name__ == "__main__":
    sys.exit(main())
