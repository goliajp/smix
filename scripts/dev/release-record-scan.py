#!/usr/bin/env python3
"""Do the release record's several lists agree with each other?

This was `crates/smix-cli/src/release_record.rs`, a `#[cfg(test)]` module
that `include_str!`ed the version boundary file, the CHANGELOG and the
guide-executability list into the crate. That made the crate's test build
depend on documents — and once the development record moved out of version
control, on files a checkout does not even have. Reconciling two documents
is a scanner's job, not a crate's; the crate now contains only code.

It checks four things, and the reasons each exists are real failures:

* **The two breaking-change lists agree.** On 2026-07-22 they held six
  entries and eight. Two of the CHANGELOG's entries had never been written
  back into the boundary table, which the charter (§10) requires — a change
  to what the version *is* belongs in the version's boundary file.
* **Every behaviour change reached the release notes.** Six of the eight
  guide-executability rows changed what smix does and none had reached the
  notes: the port ladder, the tap routes learning id and label, the
  expression grammar, the explicit regex form. A user upgrading would have
  met all of them undocumented.
* **The publish DAG covers everything that ships, in an order that works.**
  The list was written by hand and the workspace grew past it: `smix-store`
  was absent while `smix-cli` and `smix-simctl` both depended on it, so
  `cargo publish -p smix-simctl` would have been refused by the registry —
  seventeen crates into a DAG whose earlier steps cannot be taken back.
* **The semver gate still tolerates a crate it cannot check.** The script's
  own comment had said the tool was "blind to brand-new crates", a sentence
  nobody had run.

WHAT THIS CANNOT SEE

* **Whether an entry belongs on a list at all.** Whether adding a `pub`
  field breaks a caller depends on `non_exhaustive` and on whether anyone
  constructs the struct with a literal. That is a judgement; a gate that
  made it would be answering with something it invented. What goes on the
  list is decided by a person. Once it is on one list, this requires it on
  both.
* **Whether `### Added` and `### Fixed` cover what shipped.** Those sections
  have no stable shape to check against, and no second list to check them
  with.
* **Whether a migration note is workable.** It reads that one is present,
  not that following it succeeds.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# The boundary file and the executability list are development record: they
# live under `.claude/`, which is not version-controlled. A checkout without
# them cannot run this gate, and saying so is the honest outcome — quietly
# passing would report agreement between lists it never read.
CLAIMS = ".claude/docs/guide-executability.md"
CHANGELOG = "CHANGELOG.md"
SHIP = "scripts/release/ship.sh"
WORKSPACE = "Cargo.toml"


def _major():
    """The workspace major, which is which boundary this gate is about.

    It used to be written here as `v2.md` and `## [2.0.0]`, with a comment
    explaining that v2's table is about what v2 *is* — true, and it stopped
    being about the current version the moment the workspace moved on. By
    v9 this was reconciling a new boundary file against v2's phrases and
    reporting them as disagreements.

    Third time this shape has cost something in one cycle: the charter
    named `v2.md` for the decision log, the site check compared a version
    coordinate and called that current, and this. A gate that names a
    version is a gate that goes stale on a schedule nobody set.
    """
    try:
        text = open(os.path.join(ROOT, WORKSPACE), encoding="utf-8").read()
    except OSError:
        return "2"
    m = re.search(r'^version\s*=\s*"(\d+)\.', text, re.M)
    return m.group(1) if m else "2"


BOUNDARY = f".claude/docs/v{_major()}.md"

# The breaking changes belong to the major's FIRST release: later patches
# add their own headings, and the boundary table is about what the major
# *is*.
BREAKING_RELEASE = f"## [{_major()}.0.0]"


problems = []
missing = []


def read(rel):
    path = os.path.join(ROOT, rel)
    try:
        with open(path, encoding="utf-8") as fh:
            return fh.read()
    except OSError:
        missing.append(rel)
        return None


def workspace_major():
    """The major the workspace is on, for the release being made."""
    text = read(WORKSPACE) or ""
    found = re.search(r'^version\s*=\s*"(\d+)\.', text, re.M)
    return found.group(1) if found else None


def newest_release_heading(changelog):
    found = re.search(r"^## \[(\d+\.\d+\.\d+)\]", changelog, re.M)
    return f"## [{found.group(1)}]" if found else None


def cells_of(line):
    """Split one markdown table row.

    Identifiers in these tables are written as code because the tables are
    read by people first; the backticks are presentation. A phrase containing
    a backtick is written in double backticks with padding spaces — the
    markdown spelling for "this code span contains a backtick" — so the
    padding comes off too and the phrase inside is the join key.
    """
    return [c.strip().strip("`").strip() for c in line.strip().strip("|").split(" | ")]


def is_separator(first):
    return first in ("#", "id") or first.startswith("---")


def bold_phrases(section):
    """The bold phrase opening each `- **…**` entry, in order.

    Taken verbatim between the `**` pairs — one entry's phrase carries
    backticks and an underscore, and regularising it would make the join key
    something neither file actually contains.
    """
    out = []
    for line in section.splitlines():
        line = line.strip()
        if not line.startswith("- **"):
            continue
        rest = line[len("- **"):]
        end = rest.find("**")
        if end != -1:
            out.append(rest[:end])
    return out


def breaking_section(changelog, release):
    """The `### Breaking` block inside one release's entry.

    Inside, not "the first one in the file". The comment above
    BREAKING_RELEASE says this side is pinned too, and for four years it
    read the same thing either way because no later release had a
    Breaking section. 5.0.0 is the first that does, and the unpinned read
    silently compared v2's boundary table against v5's entry — the
    reader agreeing with the writer by accident.
    """
    parts = changelog.split(release)
    if len(parts) < 2:
        return None
    entry = parts[1].split("\n## [")[0]
    if "### Breaking" not in entry:
        return None
    return entry.split("### Breaking")[1].split("\n### ")[0]


def changelog_breaking_phrases(changelog):
    section = breaking_section(changelog, BREAKING_RELEASE)
    if section is None:
        problems.append(
            f"CHANGELOG's {BREAKING_RELEASE} entry has no `### Breaking` section"
        )
        return []
    return bold_phrases(section)


def changelog_release_phrases(changelog):
    """Every bold phrase anywhere in the changelog.

    All subsections and all releases, because an executability row cites the
    entry that introduced the behaviour — which is whichever release that
    was, not the current major's first one. Reading only one release's
    section made every row written before the last major look like a
    citation to nothing.
    """
    return bold_phrases(changelog)


def boundary_rows_of(text, rel):
    """`boundary_rows`, for a boundary file other than the pinned one."""
    parts = text.split("## 破坏性变更")
    if len(parts) < 2:
        problems.append(
            f"{rel} has no breaking-change table, and the release being made "
            "has breaking entries to record in it"
        )
        return []
    table = parts[1].split("\n## ")[0]
    out = []
    for line in table.splitlines():
        line = line.strip()
        if not line.startswith("| "):
            continue
        cells = cells_of(line)
        if not cells or is_separator(cells[0]):
            continue
        if len(cells) != 4:
            problems.append(
                f"{rel}: breaking-change row `{cells[0]}` has {len(cells)} "
                "cells, not 4 — a row this reader cannot split is a row "
                "nothing checks"
            )
            continue
        out.append((cells[0], cells[3]))
    return out


def boundary_rows(boundary):
    """Each breaking-change row: its number and the CHANGELOG phrase it claims."""
    parts = boundary.split("## 破坏性变更")
    if len(parts) < 2:
        problems.append(f"{BOUNDARY} has no breaking-change table")
        return []
    table = parts[1].split("\n## ")[0]
    out = []
    for line in table.splitlines():
        line = line.strip()
        if not line.startswith("| "):
            continue
        cells = cells_of(line)
        first = cells[0] if cells else ""
        if is_separator(first):
            continue
        if len(cells) != 4:
            problems.append(
                f"breaking-change row `{first}` has {len(cells)} cells, not 4 — "
                f"escape any `|` inside a cell as `\\|`. A row this reader "
                f"cannot split is a row nothing checks"
            )
            continue
        out.append((first, cells[3]))
    return out


def claim_rows(claims):
    out = []
    for line in claims.splitlines():
        line = line.strip()
        if not line.startswith("| "):
            continue
        cells = cells_of(line)
        first = cells[0] if cells else ""
        if is_separator(first):
            continue
        if len(cells) != 11:
            problems.append(
                f"claim row `{first}` has {len(cells)} cells, not 11 — "
                f"escape any `|` inside a cell as `\\|`"
            )
            continue
        out.append(cells)
    return out


def check_release_being_made(changelog):
    """The same rule, applied to the release actually being made.

    The v2 check above is about what v2 *is* and stays pinned there. It
    therefore says nothing about the entry at the top of the file — and
    for four years that was harmless, because no later release had a
    Breaking section. The first one that did (5.0.0) went entirely
    unchecked by the gate whose whole subject is "once it is on one list,
    it must be on both".

    Silent when the newest entry has no Breaking section: most releases
    do not, and a rule that demanded one would be asking for a table
    about nothing.
    """
    heading = newest_release_heading(changelog)
    major = workspace_major()
    if heading is None or major is None:
        problems.append(
            "cannot read the newest release heading or the workspace major — "
            "this half of the gate would be checking one list against nothing"
        )
        return 0
    section = breaking_section(changelog, heading)
    if section is None:
        return 0
    phrases = bold_phrases(section)
    rel = f".claude/docs/v{major}.md"
    text = read(rel)
    if text is None:
        problems.append(
            f"{heading} has a Breaking section and {rel} is absent — a change "
            "to what the version is belongs in the version's boundary file (§10)"
        )
        return 0
    claimed = [c for _, c in boundary_rows_of(text, rel)]
    for phrase in phrases:
        if phrase not in claimed:
            problems.append(
                f"{heading} lists `{phrase}` and no row of {rel}'s "
                "breaking-change table claims it"
            )
    for c in claimed:
        if c not in phrases:
            problems.append(
                f"{rel} claims `{c}`, which {heading} does not open an entry with"
            )
    return len(phrases)


def check_breaking_lists(boundary, changelog):
    rows = boundary_rows(boundary)
    phrases = changelog_breaking_phrases(changelog)
    # How many breaking changes a release has is a fact about that release,
    # so a fixed floor here would be a copy of one release's world going
    # stale beside the thing it counted. What does not go stale: two
    # independent readers of the same set must both find something, and the
    # matching below then makes them agree item by item.
    if not rows:
        problems.append(
            "no rows read out of the boundary table — the shape changed and "
            "the matching below would pass by comparing nothing"
        )
    if not phrases:
        problems.append(
            "no bold phrases read out of the CHANGELOG's Breaking section — "
            "same"
        )
    for n, phrase in rows:
        if phrase not in phrases:
            problems.append(
                f"row {n} claims `{phrase}`, which the CHANGELOG does not "
                f"open an entry with"
            )
    for p in phrases:
        if not any(phrase == p for _, phrase in rows):
            problems.append(
                f"the CHANGELOG lists `{p}` and no row of the boundary table "
                f"claims it"
            )
    return len(rows)


def check_behaviour_changes(claims, changelog):
    """Every change that altered behaviour is in the release notes.

    The `kind` column is filled by a person. Whether a change is visible to a
    user is a judgement, the same kind this gate already refuses to make about
    breaking changes; what is checkable is that the column and the citation
    agree, and that a `behaviour` row names an entry that exists.
    """
    phrases = changelog_release_phrases(changelog)
    if not phrases:
        problems.append(
            f"no bold phrases read out of the {BREAKING_RELEASE} section — "
            f"the shape changed and every citation below would red for the "
            f"wrong reason"
        )
    behaviour = 0
    rows = claim_rows(claims)
    if not rows:
        problems.append(
            "no rows read out of the claims table — the shape changed and "
            "the kind column below would be judged on nothing"
        )
    for cells in rows:
        cid, kind, citation = cells[0], cells[9], cells[10]
        if kind == "docs":
            if citation != "—":
                problems.append(f"{cid} is marked docs-only and cites `{citation}`")
        elif kind == "behaviour":
            behaviour += 1
            if citation not in phrases:
                problems.append(
                    f"{cid} changed behaviour and cites `{citation}`, which "
                    f"opens no entry under {BREAKING_RELEASE}"
                )
        else:
            problems.append(
                f"{cid} has kind `{kind}` — the vocabulary is docs / behaviour"
            )
    return behaviour


def publish_list(ship):
    parts = ship.split("CRATES=(")
    if len(parts) < 2:
        problems.append("ship.sh no longer declares a publish DAG")
        return []
    return parts[1].split(")")[0].split()


def members(workspace):
    """Workspace members, and whether each opts out of publishing.

    The manifest says `members = ["crates/*"]`, so the list is the directory.
    Reading the glob rather than a names list means a crate added tomorrow is
    covered without touching this.
    """
    if 'members = ["crates/*"]' not in workspace:
        problems.append(
            "the workspace stopped globbing crates/ — this reader assumed "
            "that shape and would now report members it invented"
        )
        return []
    out = []
    crates_dir = os.path.join(ROOT, "crates")
    for name in sorted(os.listdir(crates_dir)):
        manifest_path = os.path.join(crates_dir, name, "Cargo.toml")
        try:
            with open(manifest_path, encoding="utf-8") as fh:
                manifest = fh.read()
        except OSError:
            continue
        opted_out = any(
            line.strip().startswith("publish") and "false" in line
            for line in manifest.splitlines()
        )
        # Which sibling crates it needs before it can be published.
        # Dev-dependencies do not count: the registry does not require them
        # to exist when publishing.
        head = manifest.split("[dev-dependencies]")[0]
        deps = []
        for line in head.splitlines():
            line = line.strip()
            first = line.split()[0] if line.split() else ""
            if first.startswith("smix-") and 'path = "../' in line:
                deps.append(first)
        out.append((name, opted_out, deps))
    return out


def check_publish_dag(ship, workspace):
    listed = publish_list(ship)
    mem = members(workspace)
    if len(mem) < 25:
        problems.append(
            f"only {len(mem)} workspace members read — the manifest's shape "
            f"changed and this would pass by knowing nothing"
        )
    names = {m for m, _, _ in mem}
    for name, opted_out, _ in mem:
        if name not in listed and not opted_out:
            problems.append(
                f"{name} is a workspace member that does not opt out of "
                f"publishing and is not in the DAG"
            )
        if name in listed and opted_out:
            problems.append(
                f"{name} declares `publish = false` and is in the DAG anyway"
            )
    for name in listed:
        if name not in names:
            problems.append(f"the DAG names {name}, which is not a member")

    # Topological: a crate's siblings must already have been published.
    by_name = {m: deps for m, _, deps in mem}
    published = []
    for name in listed:
        for dep in by_name.get(name, []):
            if dep in listed and dep not in published:
                problems.append(f"{name} is published before {dep}, which it depends on")
        published.append(name)
    return listed


def check_semver_gate(ship):
    """The semver gate does not abort on a crate it cannot check.

    `cargo semver-checks --workspace` stops the whole run when a crate has no
    published baseline or a baseline with no library target. Checked as text,
    not behaviour: running it needs the network and a registry baseline.
    """
    for needle in (
        "SEMVER_EXCLUDE",
        "failed to build rustdoc for crate",
        "not found in registry",
        "of $SEMVER_TOTAL crates checked",
    ):
        if needle not in ship:
            problems.append(
                f"ship.sh no longer contains {needle!r} — the semver step "
                f"stopped tolerating crates the tool refuses, or stopped "
                f"saying how many it really checked"
            )


def check_this_gate_runs(ship):
    """This gate is only worth having where it actually runs.

    Its predecessor lived inside the crate and had to assert that preflight
    mapped a changed document back to the crates whose tests read it —
    otherwise editing only the CHANGELOG reached nothing. A scanner has no
    such indirection: it either appears in the runners or it does not.
    """
    preflight = read("scripts/dev/preflight.sh")
    if preflight is None:
        return
    me = "release-record-scan"
    if me not in preflight:
        problems.append(
            "preflight.sh no longer runs release-record-scan — edit only the "
            "CHANGELOG and nothing checks it against the boundary file"
        )
    if me not in ship:
        problems.append(
            "ship.sh no longer runs release-record-scan — the release's lists "
            "would be reconciled nowhere on the path that publishes them"
        )


def main():
    boundary = read(BOUNDARY)
    claims = read(CLAIMS)
    changelog = read(CHANGELOG)
    ship = read(SHIP)
    workspace = read(WORKSPACE)

    if missing:
        print("release-record: CANNOT RUN — these inputs are absent:")
        for m in missing:
            print(f"  - {m}")
        if any(m.startswith(".claude/") for m in missing):
            print(
                "\n  `.claude/` is the development record and is deliberately "
                "not version-controlled.\n  This gate reconciles the release "
                "notes against it, so it runs where that record lives —\n  the "
                "authoring machine — and not in a bare checkout."
            )
        return 2

    n_breaking = check_breaking_lists(boundary, changelog)
    n_now = check_release_being_made(changelog)
    n_behaviour = check_behaviour_changes(claims, changelog)
    listed = check_publish_dag(ship, workspace)
    check_semver_gate(ship)
    check_this_gate_runs(ship)

    if problems:
        print(f"release-record: FAIL — {len(problems)} disagreements")
        for p in problems:
            print(f"  - {p}")
        return 1

    print(
        f"release-record: {n_breaking} breaking changes in {BOUNDARY} and "
        f"{n_now} in the "
        f"release being made, both lists agree · "
        f"{n_behaviour} behaviour changes in the release notes · publish list "
        f"{len(listed)} crates, topological"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
