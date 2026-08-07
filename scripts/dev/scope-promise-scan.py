#!/usr/bin/env python3
"""Check that what the scope file promises still matches what exists.

`.claude/docs/v2.md` promises `--stable` under determinism. Nothing implements
it and nothing ever withdrew it. Tracing the promise back found an
exploration note — status "explored", priority 3, aimed at an older
milestone, and requiring cooperation from the app under test that the
scope file dropped. Three further mentions live in a gitignored dossier,
so they do not exist in a clone. Four documents agreed with each other
for seven months while no code agreed with any of them.

The same scope file names eight MCP capabilities; two of them, session
and diagnostic-dump, have no tool.

So each promise carries a citation re-evaluated on every run, and the
load-bearing rule is that a promise marked shipped may NOT cite a file
under docs/. A document is not evidence that something was built. That
one rule is what would have caught this.

`pending` is a watched terminal state, not a parking space: a pending
row must carry decision material AND a `none` citation that still holds.
The moment the thing gets built, that citation fails and the row has to
move. It is the mirror of the audit ledger's asymmetry — that one stops
"fixed but still recorded open", this one stops "built but still
recorded undecided".

WHAT THIS CANNOT SEE:

  * Degree words. "MCP driving surface matured", "full parity" — no
    citation can settle those, and pretending otherwise would make the
    gate an oracle it is not.
  * Every backticked token in the scope text. Checking all of them would
    require a hand-kept exemption table, which is another unmaintained
    list — the thing this exists to prevent. Only flags and the named
    MCP capabilities are reconciled; a sub-deliverable outside those two
    rules can still go missing silently, and is caught only by a person
    splitting the row.
  * How long a row has been pending. Deciding is the user's call, and a
    clock here would just teach people to flip a status to stop the
    noise.

Exit non-zero on any failure.
"""

import datetime
import glob as globlib
import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
EVIDENCE = ".claude/docs/scope-evidence.md"
PENDING = ".claude/docs/scope-decisions-pending.md"
SOURCE = ".claude/docs/v2.md"

IN_SCOPE_START = "## 做什么（in scope）"
IN_SCOPE_END = "## 不做什么"

GATES = (
    "scripts/dev/preflight.sh",
    ".github/workflows/ci.yml",
    "scripts/release/ship.sh",
)

STATUSES = {"shipped", "withdrawn", "pending"}

# Under this, assume the table failed to parse rather than that the
# project promises less.
MIN_ROWS = 7

# Every pending promise needs material, and material means these
# sections in this order. Fixed so "we wrote something" cannot pass for
# "someone can decide from this".
REQUIRED_SECTIONS = [
    "承诺原文与出处",
    "实证现状",
    "若要做:动哪几层",
    "若要撤回:先例与草稿",
    "不做也不撤回的代价",
    "需要你拍的板",
]

AT_CITATION = re.compile(r'^at\s+(\S+):(\d+)\s+"(.+)"$', re.S)
NONE_CITATION = re.compile(r'^none\s+"(.+)"\s+in\s+(\S+)$', re.S)


def read(rel):
    with open(os.path.join(ROOT, rel), encoding="utf-8") as f:
        return f.read()


def tracked_paths():
    out = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "--cached"],
        capture_output=True, text=True, check=True,
    )
    return set(out.stdout.splitlines())


def in_scope_block():
    body = read(SOURCE)
    start = body.index(IN_SCOPE_START)
    end = body.index(IN_SCOPE_END, start)
    return body[start:end]


def scope_item_numbers(block):
    """The `N.` items the scope file lists. Counted, never hardcoded."""
    return {m.group(1) for m in re.finditer(r"^(\d+)\.\s+\*\*", block, re.M)}


def parse_rows(body, failures):
    rows = []
    for line in body.splitlines():
        line = line.strip()
        if not line.startswith("|") or not line.endswith("|"):
            continue
        cells = [c.strip() for c in line.strip("|").split("|")]
        if cells and (cells[0] in ("#", "") or set(cells[0]) <= set("-: ")):
            continue
        if len(cells) != 6:
            failures.append(
                f"{EVIDENCE}: row {cells[0]!r} parsed into {len(cells)} cells, not 6. "
                f"A pipe inside a citation splits the row, and a row the checker "
                f"cannot see is a row nothing checks."
            )
            continue
        rows.append({
            "num": cells[0],
            "promise": cells[1],
            "status": cells[2],
            "citation": cells[3].strip("`"),
            "why": cells[4],
            "checked": cells[5],
        })
    return rows


def check_citation(row, tracked, failures):
    citation, num, status = row["citation"], row["num"], row["status"]

    at = AT_CITATION.match(citation)
    if at:
        path, lineno, literal = at.group(1), int(at.group(2)), at.group(3)
        if status == "shipped" and path.startswith("docs/"):
            failures.append(
                f"{num}: a shipped promise cites {path}, a document. Documents are "
                f"not evidence that something was built — `--stable` stayed "
                f"'promised' across four agreeing documents and zero lines of "
                f"code. Cite the implementation."
            )
            return
        if path not in tracked:
            failures.append(f"{num}: cites {path}, which git does not track.")
            return
        full = os.path.join(ROOT, path)
        if not os.path.isfile(full):
            failures.append(f"{num}: cites {path}, which does not exist.")
            return
        lines = open(full, encoding="utf-8").read().splitlines()
        if not 1 <= lineno <= len(lines):
            failures.append(f"{num}: cites {path}:{lineno}; the file has {len(lines)} lines.")
            return
        if literal not in lines[lineno - 1]:
            elsewhere = [i + 1 for i, l in enumerate(lines) if literal in l]
            failures.append(
                f"{num}: {literal!r} is not on {path}:{lineno}"
                + (f" — it is on {elsewhere}. Update the line number."
                   if elsewhere else
                   f". It is nowhere in the file; this promise's status has "
                   f"probably changed.")
            )
        return

    none = NONE_CITATION.match(citation)
    if none:
        literal, glob = none.group(1), none.group(2)
        hits = []
        for path in globlib.glob(os.path.join(ROOT, glob), recursive=True):
            rel = os.path.relpath(path, ROOT).replace(os.sep, "/")
            if rel not in tracked or not os.path.isfile(path):
                continue
            try:
                if literal in open(path, encoding="utf-8").read():
                    hits.append(rel)
            except (UnicodeDecodeError, OSError):
                continue
        if hits:
            failures.append(
                f"{num}: claims {literal!r} is absent from {glob}, but it is in "
                f"{hits[:5]}. If it was built, this row is no longer {status}."
            )
        return

    failures.append(f"{num}: citation is neither form: {citation!r}")


def check_flag_promises(block, rows, failures):
    """Every `--flag` the scope text names must appear in some row."""
    flags = {m.group(1) for m in re.finditer(r"`(--[a-z][a-z-]*)`", block)}
    covered = " ".join(r["promise"] + r["why"] for r in rows)
    for flag in sorted(flags):
        if flag not in covered:
            failures.append(
                f"{SOURCE} promises {flag} but no row in {EVIDENCE} accounts for "
                f"it. A flag named in scope and tracked nowhere is how `--stable` "
                f"survived unbuilt and unwithdrawn."
            )


def check_mcp_capabilities(block, rows, failures):
    """Capabilities the scope text names for MCP must each have a tool."""
    match = re.search(r"MCP[^\n]*?（([a-z/\-]+)）", block)
    if not match:
        return
    named = [c for c in match.group(1).split("/") if c]
    tools = read("crates/smix-mcp/src/main.rs")
    covered = " ".join(r["promise"] + r["why"] for r in rows)
    for cap in named:
        stem = cap.replace("-", "_")
        if re.search(rf"async fn smix_[a-z_]*{re.escape(stem)}", tools):
            continue
        if cap not in covered:
            failures.append(
                f"{SOURCE} names '{cap}' among the MCP capabilities, no tool "
                f"implements it, and no row in {EVIDENCE} accounts for it."
            )


def main():
    failures = []

    for path in (EVIDENCE, PENDING):
        if not os.path.isfile(os.path.join(ROOT, path)):
            failures.append(
                f"{path} does not exist. With nothing to re-evaluate this scan is "
                f"indistinguishable from a clean bill of health, so it fails."
            )

    rows = []
    if os.path.isfile(os.path.join(ROOT, EVIDENCE)):
        rows = parse_rows(read(EVIDENCE), failures)
        if len(rows) < MIN_ROWS:
            failures.append(
                f"{EVIDENCE}: parsed {len(rows)} rows, expected at least {MIN_ROWS}. "
                f"Treated as a broken parse, not a shorter scope."
            )

    if rows:
        tracked = tracked_paths()
        today = datetime.date.today()
        block = in_scope_block()

        for row in rows:
            num = row["num"]
            if row["status"] not in STATUSES:
                failures.append(
                    f"{num}: status {row['status']!r} is not one of {sorted(STATUSES)}."
                )
            try:
                if datetime.date.fromisoformat(row["checked"]) > today:
                    failures.append(f"{num}: verification date is in the future.")
            except ValueError:
                failures.append(f"{num}: date {row['checked']!r} is not ISO (YYYY-MM-DD).")
            if not row["why"] or row["why"] == "—":
                failures.append(f"{num}: every row must say why it holds that status.")
            check_citation(row, tracked, failures)

        # Coverage: derived from the scope file, never a written-down count.
        listed = scope_item_numbers(block)
        seen = {r["num"].rstrip("abcdefghijklmnopqrstuvwxyz") for r in rows}
        missing = sorted(listed - seen)
        if missing:
            failures.append(
                f"{EVIDENCE} has no row for in-scope item(s) {missing}. The row set "
                f"comes from {SOURCE}, so a new promise makes this red until it is "
                f"accounted for."
            )
        extra = sorted(seen - listed)
        if extra:
            failures.append(f"{EVIDENCE} has rows {extra} that {SOURCE} does not list.")

        check_flag_promises(block, rows, failures)
        check_mcp_capabilities(block, rows, failures)

        # Pending means material exists, in the shape someone can decide from.
        if os.path.isfile(os.path.join(ROOT, PENDING)):
            material = read(PENDING)
            headings = re.findall(r"^### (.+)$", material, re.M)
            entries = re.findall(r"^## (.+)$", material, re.M)
            pending_rows = [r for r in rows if r["status"] == "pending"]
            if pending_rows and not entries:
                failures.append(
                    f"{len(pending_rows)} promise(s) are pending but {PENDING} "
                    f"contains no entries. Pending without material is a decision "
                    f"nobody can make."
                )
            for i in range(len(entries)):
                chunk = headings[i * len(REQUIRED_SECTIONS):(i + 1) * len(REQUIRED_SECTIONS)]
                if chunk != REQUIRED_SECTIONS:
                    failures.append(
                        f"{PENDING}: entry {entries[i]!r} does not carry the six "
                        f"sections in order. Got {chunk}, expected "
                        f"{REQUIRED_SECTIONS}."
                    )

    for gate in GATES:
        if "scope-promise-scan" not in read(gate):
            failures.append(
                f"{gate} does not invoke scope-promise-scan. Three places: the "
                f"local habit, the branch, and the one that reaches users."
            )

    if failures:
        print("scope-promise-scan: FAIL", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    counts = {s: sum(1 for r in rows if r["status"] == s) for s in sorted(STATUSES)}
    print(
        f"scope-promise-scan: clean — {len(rows)} promises "
        f"({counts['shipped']} shipped / {counts['pending']} pending / "
        f"{counts['withdrawn']} withdrawn), {len(rows)} citations re-evaluated"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
