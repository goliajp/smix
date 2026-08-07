#!/usr/bin/env python3
"""Re-evaluate the open-defect ledger instead of trusting what it says.

On 2026-07-22 five of the fourteen entries recorded as unfixed were
sampled. Three had been fixed long before and one no longer described
anything in the tree. The list is the main input to "what is left before
release", and it had drifted in both directions with nothing watching.

That is this week's recurring defect, one layer up: true when written,
false later, unguarded in between.

So every row here carries a citation the machine re-evaluates on each
run, and the citations are deliberately asymmetric in what they pin:

  * a `present` row cites the DEFECTIVE code. Fix the defect and the
    citation stops matching — the gate goes red and makes you update the
    status. That is exactly the rot that happened.
  * a `fixed` row cites the FIXING code. Revert or refactor it away and
    the same thing happens from the other side.

WHAT THIS CANNOT SEE, stated so nobody reads it as omniscient:

  * It checks that a citation still holds, NOT that the citation means
    what the row claims. A row can be internally consistent and still
    describe reality wrongly; only a person can judge that.
  * It deliberately does NOT check whether the cited file was committed
    after the verification date. That would catch one narrow case — an
    edit that changes a line's meaning without moving it — at the cost
    of turning every unrelated commit into a red table. People would
    learn to bump the date without looking, which is the reflex this
    whole exercise exists to remove.

Exit non-zero on any failure.
"""

import datetime
import glob as globlib
import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
LEDGER = ".claude/docs/audit-ledger.md"
SOURCE = ".claude/docs/v2.md"

# The one line in v2.md that defines which entries must exist. Located by
# anchor rather than by line number, and required to be unique: the
# circled numerals appear on ~18 other lines (other numbered lists), so a
# whole-file scan would silently pick up the wrong set.
ANCHOR = "待办(按严重度,未修)"

GATES = (
    "scripts/dev/preflight.sh",
    ".github/workflows/ci.yml",
    "scripts/release/ship.sh",
)

STATUSES = {"fixed", "present", "moot"}

# Where a fix would have to land. Fixed vocabulary so the column stays
# sortable for planning; it is not an estimate of effort.
LAYERS = {
    "parser", "rust-client", "driver", "sdk", "mcp", "cli",
    "swift-runner", "kotlin-runner", "docs",
}

# Below this, assume the table failed to parse rather than that the list
# shrank. A regex that reads zero rows would let every later check pass
# vacuously — the floor android-gate-scan carries for the same reason.
MIN_ROWS = 10

CIRCLED = "①②③④⑤⑥⑦⑧⑨⑩⑪⑫⑬⑭⑮⑯⑰⑱⑲⑳"

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


def parse_rows(body, failures):
    """Rows of the ledger's single table, as dicts.

    A line that looks like a table row but does not have seven cells is
    an error, not something to skip. The first version skipped silently,
    and a citation containing `||` (a Rust short-circuit) split into nine
    cells — that row vanished from the table while the gate reported
    clean. A row the checker cannot see is a row nothing is checking,
    which is the whole failure mode being guarded against here.
    """
    rows = []
    for line in body.splitlines():
        line = line.strip()
        if not line.startswith("|") or not line.endswith("|"):
            continue
        cells = [c.strip() for c in line.strip("|").split("|")]
        if cells and (cells[0] in ("#", "") or set(cells[0]) <= set("-: ")):
            continue  # header or separator
        if len(cells) != 7:
            failures.append(
                f"row {cells[0]!r} parsed into {len(cells)} cells, not 7. A pipe "
                f"inside a citation (Rust `||`, a table in prose) splits the row; "
                f"escape it as \\| so the cell survives. Skipping it would hide "
                f"the row from every check below."
            )
            continue
        rows.append({
            "num": cells[0],
            "defect": cells[1],
            "status": cells[2],
            "citation": cells[3].strip("`"),
            "why": cells[4],
            "layer": cells[5],
            "checked": cells[6],
        })
    return rows


def required_numbers(failures):
    """The circled numerals from the one anchored line in v2.md."""
    lines = [l for l in read(SOURCE).splitlines() if ANCHOR in l]
    if len(lines) != 1:
        failures.append(
            f"{SOURCE}: the anchor {ANCHOR!r} matches {len(lines)} lines, expected "
            f"exactly 1 — the ledger's row set is defined by that line, so this "
            f"scan cannot tell which entries are required."
        )
        return None
    return {c for c in lines[0] if c in CIRCLED}


def check_citation(row, tracked, failures):
    citation = row["citation"]
    num = row["num"]

    at = AT_CITATION.match(citation)
    if at:
        path, lineno, literal = at.group(1), int(at.group(2)), at.group(3)
        if path not in tracked:
            failures.append(
                f"{num}: cites {path}, which git does not track. Derived copies "
                f"(e.g. ~/.local/share/smix/runner/, written by `smix runner "
                f"install`) do not exist in a clone and can disagree with the "
                f"tree. Cite the source under swift-bridge/ or crates/."
            )
            return
        full = os.path.join(ROOT, path)
        if not os.path.isfile(full):
            failures.append(f"{num}: cites {path}, which does not exist.")
            return
        lines = open(full, encoding="utf-8").read().splitlines()
        if lineno < 1 or lineno > len(lines):
            failures.append(
                f"{num}: cites {path}:{lineno} but the file has {len(lines)} lines."
            )
            return
        target = lines[lineno - 1]
        if literal not in target:
            elsewhere = [i + 1 for i, l in enumerate(lines) if literal in l]
            if elsewhere:
                failures.append(
                    f"{num}: {literal!r} is not on {path}:{lineno} — it is on "
                    f"{elsewhere}. Update the line number."
                )
            else:
                failures.append(
                    f"{num}: {literal!r} is nowhere in {path}. The status of this "
                    f"row has probably changed — re-verify it and set today's "
                    f"date in the last column."
                )
            return
        stripped = target.strip()
        # `#[` is a Rust attribute, not a comment. Row ① cites exactly
        # that shape. Stated here so nobody "corrects" the exception away.
        comment_starts = ("//", "/*", "* ", "<!--", "# ")
        if stripped.startswith(comment_starts) and not stripped.startswith("#["):
            failures.append(
                f"{num}: cites a comment line ({path}:{lineno}). Comments are "
                f"claims; cite the code that makes the claim true."
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
                f"{num}: claims {literal!r} is gone from {glob}, but it is in "
                f"{hits[:5]}. Either it came back or the row is wrong."
            )
        return

    failures.append(
        f"{num}: citation is neither form. Use "
        f'`at <path>:<line> "<literal>"` or `none "<literal>" in <glob>`, '
        f"got: {citation!r}"
    )


def main():
    failures = []

    ledger_path = os.path.join(ROOT, LEDGER)
    if not os.path.isfile(ledger_path):
        failures.append(
            f"{LEDGER} does not exist. This scan has nothing to re-evaluate, "
            f"which is indistinguishable from a clean bill of health — so it "
            f"fails instead."
        )
        rows = []
    else:
        rows = parse_rows(read(LEDGER), failures)
        if len(rows) < MIN_ROWS:
            failures.append(
                f"{LEDGER}: parsed {len(rows)} rows, expected at least {MIN_ROWS}. "
                f"A parse that reads too few rows lets every later check pass "
                f"vacuously, so this is treated as a broken table rather than a "
                f"short one."
            )

    if rows:
        tracked = tracked_paths()
        today = datetime.date.today()
        seen = set()

        for row in rows:
            num = row["num"]
            if row["status"] not in STATUSES:
                failures.append(
                    f"{num}: status {row['status']!r} is not one of "
                    f"{sorted(STATUSES)}. An open vocabulary here lets the column "
                    f"drift back into prose."
                )
            try:
                checked = datetime.date.fromisoformat(row["checked"])
                if checked > today:
                    failures.append(f"{num}: verification date {row['checked']} is in the future.")
            except ValueError:
                failures.append(f"{num}: verification date {row['checked']!r} is not ISO (YYYY-MM-DD).")

            if row["status"] == "present":
                if not row["why"] or row["why"] == "—":
                    failures.append(
                        f"{num}: a present row must say how the defect is still "
                        f"reachable, entry point first."
                    )
                layers = [l.strip() for l in row["layer"].split("+") if l.strip()]
                if not layers or row["layer"] == "—":
                    failures.append(f"{num}: a present row must name the layers a fix would touch.")
                else:
                    unknown = [l for l in layers if l not in LAYERS]
                    if unknown:
                        failures.append(
                            f"{num}: unknown layer(s) {unknown}; the vocabulary is {sorted(LAYERS)}."
                        )
            else:
                if not row["why"] or row["why"] == "—":
                    failures.append(f"{num}: a {row['status']} row must say why.")
                if row["layer"] != "—":
                    failures.append(
                        f"{num}: layer must be — on a {row['status']} row (nothing to fix)."
                    )

            check_citation(row, tracked, failures)
            seen.add(num.rstrip("abcdefghijklmnopqrstuvwxyz"))

        required = required_numbers(failures)
        if required is not None:
            missing = sorted(required - seen, key=CIRCLED.index)
            extra = sorted(seen - required, key=lambda c: CIRCLED.index(c) if c in CIRCLED else 99)
            if missing:
                failures.append(
                    f"{LEDGER} has no row for {''.join(missing)}, which {SOURCE} lists "
                    f"as open. The row set comes from that line, not from a number "
                    f"written here."
                )
            if extra:
                failures.append(
                    f"{LEDGER} has rows {''.join(extra)} that {SOURCE} does not list."
                )

    for gate in GATES:
        if "audit-ledger-scan" not in read(gate):
            failures.append(
                f"{gate} does not invoke audit-ledger-scan. Three places, not one: "
                f"preflight is the local habit, CI is the branch, ship is the "
                f"release — and missing ship is missing the path to users. "
                f"adb-guard shipped with its script committed and the line that "
                f"runs it left behind."
            )

    if failures:
        print("audit-ledger-scan: FAIL", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    counts = {s: sum(1 for r in rows if r["status"] == s) for s in sorted(STATUSES)}
    print(
        f"audit-ledger-scan: clean — {len(rows)} rows "
        f"({counts['fixed']} fixed / {counts['present']} present / {counts['moot']} moot), "
        f"{len(rows)} citations re-evaluated"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
