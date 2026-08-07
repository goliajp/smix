#!/usr/bin/env python3
"""Does the guide-executability list agree with the probes and the ledger?

The list records, claim by claim, whether what the guides promise actually
runs — and for the ones that do not, which layer is at fault and which
audit-ledger row is tracking it. It is only worth what its bookkeeping is
worth: a row naming a probe that no longer exists, or a probe running with
no row, is drift in the one table that is supposed to be the account.

This was the tail of `crates/smix-cli/src/guide_gate.rs`, which reached it
by compiling `.claude/docs/guide-executability.md` and
`.claude/docs/audit-ledger.md` into the crate. Both are development record
and neither is version-controlled, so the crate's test build depended on
files a checkout does not have. Reconciling three documents is a scanner's
job. The behavioural half — actually running each documented example —
stays in the crate, where the parser and the driver are.

Three ways drift shows up, and this checks all three:

  1. A row's columns disagree with each other. A claim marked `runs` that
     still names a broken layer, a `unjudged` row carrying a probe, a
     review date in the future.
  2. A row names a probe that is not a test in guide_gate.rs.
  3. A probe runs and no row claims it. Without this, deleting a row would
     leave its probe running and unaccounted for — the half of the drift
     the row-side check cannot see.
"""

import datetime
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

LIST = ".claude/docs/guide-executability.md"
LEDGER = ".claude/docs/audit-ledger.md"
GATE = "crates/smix-cli/src/guide_gate.rs"

# Columns, by position in the 11-cell row.
ID, STATUS, PROBE, LAYER, LEDGER_REF, REVIEWED = 0, 3, 4, 6, 7, 8

STATUSES = ("runs", "broken", "unjudged")
MIN_ROWS = 8

# Probes written by hand rather than generated from the corpus. Each must
# exist in the gate and be claimed by a row; the list is here rather than
# derived so that deleting a probe *and* its row still fails.
HAND_WRITTEN_PROBES = [
    "every_runner_dialling_command_can_reach_the_registry",
    "a_configured_launch_activity_reaches_the_device",
    "the_default_tap_takes_the_route_its_page_names",
    "the_daemon_proxy_id_example_is_admissible",
    "the_bare_string_form_matches_a_real_tree",
    "the_documented_regex_examples_are_patterns",
    "every_documented_key_name_parses",
]

problems = []
missing = []


def read(rel):
    try:
        with open(os.path.join(ROOT, rel), encoding="utf-8") as fh:
            return fh.read()
    except OSError:
        missing.append(rel)
        return None


def rows(text):
    """Read the table.

    A row whose cell count is wrong is an error, not a skip. The audit
    ledger learned this the expensive way: one citation contained an
    unescaped `|`, the row split into eleven cells, the scan skipped it as
    unparseable, and reported clean — a row nothing checked, in a table
    whose entire purpose is that every row is checked.
    """
    out = []
    for line in text.splitlines():
        line = line.strip()
        if not line.startswith("| "):
            continue
        cells = [c.strip().strip("`") for c in line.strip("|").split(" | ")]
        first = cells[0] if cells else ""
        if first == "id" or first.startswith("---"):
            continue
        if len(cells) != 11:
            problems.append(
                f"row `{first}` has {len(cells)} cells, not 11 — escape any "
                f"`|` inside a cell as `\\|`. A row this reader cannot split "
                f"is a row nothing checks"
            )
            continue
        out.append(cells)
    return out


def main():
    listing = read(LIST)
    ledger = read(LEDGER)
    gate = read(GATE)

    if missing:
        print("guide-claims: CANNOT RUN — these inputs are absent:")
        for m in missing:
            print(f"  - {m}")
        if any(m.startswith(".claude/") for m in missing):
            print(
                "\n  `.claude/` is the development record and is deliberately "
                "not version-controlled.\n  This gate reconciles it against "
                "the probes, so it runs where that record lives."
            )
        return 2

    tests = set(re.findall(r"^fn (\w+)\(", gate, re.M))
    table = rows(listing)
    if len(table) < MIN_ROWS:
        problems.append(
            f"only {len(table)} rows parsed out of the list — the table shape "
            f"changed and this check would pass by knowing nothing"
        )

    today = datetime.date.today().isoformat()
    for cells in table:
        rid, status = cells[ID], cells[STATUS]
        if status not in STATUSES:
            problems.append(
                f"{rid}: status `{status}` is outside the vocabulary — an open "
                f"vocabulary drifts this column back into prose"
            )
        if cells[REVIEWED] > today:
            problems.append(f"{rid}: reviewed {cells[REVIEWED]} is in the future")
        if status == "runs":
            if cells[LAYER] != "—":
                problems.append(f"{rid}: a claim that runs has no layer to fix")
        elif cells[LAYER] == "—":
            problems.append(f"{rid}: says what is broken, not where")
        if status == "unjudged":
            if cells[PROBE] != "—":
                problems.append(f"{rid}: unjudged rows have no probe")
        elif cells[PROBE] not in tests:
            problems.append(
                f"{rid}: names probe `{cells[PROBE]}`, which is not a test in "
                f"{GATE}"
            )
        if cells[LEDGER_REF] != "—" and cells[LEDGER_REF] not in ledger:
            problems.append(
                f"{rid}: cites ledger row {cells[LEDGER_REF]}, which does not "
                f"appear in {LEDGER}"
            )

    claimed = {c[PROBE] for c in table}
    for name in HAND_WRITTEN_PROBES:
        if name not in tests:
            problems.append(f"probe `{name}` is named here but no longer exists")
        if name not in claimed:
            problems.append(f"probe `{name}` runs and no row in the list claims it")

    if problems:
        print(f"guide-claims: FAIL — {len(problems)} problems")
        for p in problems:
            print(f"  - {p}")
        return 1

    counts = {s: sum(1 for c in table if c[STATUS] == s) for s in STATUSES}
    print(
        f"guide-claims: {len(table)} claims "
        f"({counts['runs']} runs / {counts['broken']} broken / "
        f"{counts['unjudged']} unjudged), every probe accounted for"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
