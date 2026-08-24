#!/usr/bin/env python3
"""All four layers are present, and "waiting on a decision" is one of them.

Section 0 requires the roadmap, the current major's boundary file, a hot
plan and a cold plan to exist at every moment. Section 6 forbids opening
the next segment before the user says so. Between a checkpoint being
archived and the user speaking, those two cannot both hold — and this
cycle logged two of those windows as carelessness before anyone noticed
the contract made them inevitable (`.claude/docs/v4.md`, 2026-08-12).

So this gate does not ask whether `plan-hot.md` is there. It asks whether
the gap has been CLAIMED: archived, and then either hot again or holding
a note that says whose word it is waiting on. Waiting is a legal state
and has to be writable, because a gate that cannot tell it from a loss
judges the honest case and the careless one identically — which is
exactly how those two windows came to be recorded wrongly.

Layer three is therefore satisfied by one of two files, exactly one:

    `plan-hot.md`           — being worked
    `plan-hot.awaiting.md`  — archived, waiting, four fields saying on what

Everything beside the four layers is claimed by name in BESIDE, with a
reason. Not being listed is never how a file gets in.

It reads `.claude/docs/`, which is development record and by the
2026-07-29 decision is not version-controlled. On a bare checkout it
therefore REFUSES rather than passing: green must never mean "read
nothing". That is `guide-claims-scan`'s precedent, and the reason this
scanner is absent from CI while its harness is not.
"""

from __future__ import annotations

import argparse
import os
import re
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

DOCS = os.path.join(".claude", "docs")
HOT = "plan-hot.md"
AWAITING = "plan-hot.awaiting.md"
ARCHIVE = os.path.join("archive", "plan-history")

# A hot plan states its own segment on line one; every other layer is
# located from it.
HOT_TITLE = re.compile(r"^#\s*plan-hot\s*—\s*v(\d+)\.(\d+)\s*到\s*(\S+?)[：:]")

# The note's four fields, read by line prefix. They are Chinese because
# the document they live in is; the last one carries all the meaning.
AWAITING_FIELDS = ("已归档", "下一段", "自", "等的是")

# Anything beside the four layers, by name and with its reason.
BESIDE = {
    "archive/": "frozen — never revised, never updated",
    "research/": "obtainability research",
    "perf/": "perf decomposition ground truth",
    "audit-ledger.md": "internal defect accounting",
    "scope-evidence.md": "internal scope accounting",
    "guide-executability.md": "what the guides claim versus what runs",
    "open-items.md": "the open ones, grouped by root cause rather than by symptom",
    "scope-decisions-pending.md": "scope calls waiting on the owner; empty is a state, not an absence",
}


def archive_key(name: str) -> tuple:
    """Order archives by version line then checkpoint, for "the newest"."""
    m = re.match(r"v(\d+)\.(\d+)-(?:c(\d+)|(.+?))-hot\.md$", name)
    if not m:
        return (0, 0, 0, name)
    return (int(m.group(1)), int(m.group(2)),
            int(m.group(3)) if m.group(3) else 0, name)


def read(path: str) -> str:
    with open(path, encoding="utf-8") as fh:
        return fh.read()


def awaiting_fields(text: str) -> dict:
    found = {}
    for line in text.splitlines():
        for field in AWAITING_FIELDS:
            prefix = f"- {field}："
            if line.startswith(prefix):
                found[field] = line[len(prefix):].strip()
    return found


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=REPO)
    args = ap.parse_args()
    root = os.path.abspath(args.root)
    docs = os.path.join(root, DOCS)

    if not os.path.isdir(docs):
        print("contract-scan: CANNOT RUN")
        print(f"  - {DOCS} is not in this tree. It is development record and by")
        print("    the 2026-07-29 decision is not version-controlled, so this gate")
        print("    runs where that record lives — green must not mean read nothing.")
        return 2

    problems: list[str] = []

    archive_dir = os.path.join(docs, ARCHIVE)
    archives = sorted(
        (n for n in os.listdir(archive_dir) if n.endswith("-hot.md")),
        key=archive_key,
    ) if os.path.isdir(archive_dir) else []
    latest = archives[-1] if archives else None

    hot = os.path.isfile(os.path.join(docs, HOT))
    awaiting = os.path.isfile(os.path.join(docs, AWAITING))
    held = ""
    segment = None

    # Every branch has to be total. The first draft was not: disable any
    # one rule and control fell into a branch that read a file which was
    # not there, or used a None match — so it went red by RAISING. An
    # exception and a verdict leave the same exit code, and three red
    # cases were passing on the traceback. The harness now demands that
    # red be a sentence.
    if hot and awaiting:
        problems.append(
            f"layer three has both {HOT} and {AWAITING}. It is one position and "
            f"holds EXACTLY ONE — a segment cannot be worked and waited on at "
            f"the same time. Archiving deletes the hot plan; going hot deletes "
            f"the note."
        )
    elif not hot and not awaiting:
        at = f", the newest being {ARCHIVE}/{latest}" if latest else ""
        problems.append(
            f"layer three is empty, and the gap is neither hot nor waiting on "
            f"anyone{at}. Section 0 wants all four layers at every moment and "
            f"section 6 forbids going hot before the user speaks — to keep both, "
            f"write {DOCS}/{AWAITING} and say in four lines whose word this is "
            f"waiting on. Waiting is legal; saying nothing is not."
        )
    elif awaiting:
        fields = awaiting_fields(read(os.path.join(docs, AWAITING)))
        for field in AWAITING_FIELDS:
            if field not in fields:
                problems.append(f"{AWAITING} has no 「{field}」 line")
            elif not fields[field]:
                problems.append(
                    f"{AWAITING} leaves 「{field}」 empty — a note that does not "
                    f"say what it waits for is the same as no note"
                )
        held = fields.get("等的是", "")
    elif hot:
        title = read(os.path.join(docs, HOT)).splitlines()[:1]
        m = HOT_TITLE.match(title[0]) if title else None
        if m is None:
            problems.append(
                f"{HOT} does not say on line one which segment it is. The form is "
                f"`# plan-hot — v{{X.Y}} 到 {{CP}}：…` — every other layer is "
                f"located from that line, so a plan that will not declare itself "
                f"takes the whole check down quietly."
            )
        else:
            segment = (m.group(1), m.group(2), m.group(3))

    # The segment in flight is read off layer three, so a broken layer
    # three cannot locate the other three. Say what is already known.
    inflight = segment[:2] if segment else None
    if awaiting and not problems:
        fields = awaiting_fields(read(os.path.join(docs, AWAITING)))
        m = re.match(r"v(\d+)\.(\d+)", fields.get("下一段", ""))
        if m is None:
            problems.append(
                f"{AWAITING} 「下一段」 does not state a version "
                f"(read {fields.get('下一段', '')!r}). The form is `v{{X.Y}} C{{N}}` "
                f"— layer four is located from it."
            )
        else:
            inflight = (m.group(1), m.group(2))
            named = fields.get("已归档", "").strip("`").split("/")[-1]
            line = [a for a in archives
                    if archive_key(a)[:2] == (int(m.group(1)), int(m.group(2)))]
            if named not in archives:
                problems.append(
                    f"{AWAITING} 「已归档」 points at {named} and {ARCHIVE} has no "
                    f"such file"
                )
            elif line and named != line[-1]:
                problems.append(
                    f"{AWAITING} is stuck at {named} while the newest on that "
                    f"version line is {line[-1]} — the archive moved on and the "
                    f"note did not. The staleness test is mechanical (it must be "
                    f"the newest) rather than a date threshold, because a "
                    f"threshold only teaches people to touch up the date."
                )

    if inflight and not problems:
        ver = f"v{inflight[0]}.{inflight[1]}"

        if ver not in read(os.path.join(docs, "roadmap.md")):
            problems.append(
                f"layer one, roadmap.md, does not carry {ver}, which is what is "
                f"in flight — the path is missing the step being taken"
            )

        cargo = os.path.join(root, "Cargo.toml")
        m = re.search(r'^version = "(\d+)\.', read(cargo), re.M) \
            if os.path.isfile(cargo) else None
        if m is None:
            problems.append(
                "cannot read the workspace version out of Cargo.toml — layer two "
                "is derived from it"
            )
        else:
            major = m.group(1)
            if not os.path.isfile(os.path.join(docs, f"v{major}.md")):
                problems.append(
                    f"layer two, v{major}.md, is absent. The workspace major is "
                    f"{major} and that major's decisions have nowhere to go — "
                    f"v4's decisions spent a while wedged into v2.md for exactly "
                    f"this reason"
                )
            if major != inflight[0]:
                problems.append(
                    f"{ver} is in flight while the workspace major is {major} — "
                    f"layer three and layer two disagree"
                )

        cold = os.path.join(docs, "plan-cold")
        hits = [n for n in os.listdir(cold) if n.startswith(ver)] \
            if os.path.isdir(cold) else []
        if not hits:
            problems.append(
                f"layer four has no cold plan starting {ver} — that version is "
                f"being worked and its shape was never drawn"
            )
        elif len(hits) > 1:
            problems.append(
                f"layer four has {len(hits)} cold plans for {ver}: "
                f"{', '.join(sorted(hits))}"
            )

    # Every archived checkpoint has a line in its cold plan, and the two
    # say the same thing.
    #
    # The cold plan is the version's shape and the archive is what was
    # actually done; when they part, the shape is fiction. v4.3's cold
    # plan described runner-attach work from its title to its checkpoint
    # list while two checkpoints of selector-surface work were archived
    # against it, and the previous checkpoint had put "rearrange the cold
    # plan" in its own closing actions — where it was skipped, because a
    # documentation edit has nothing that goes red.
    if inflight and not problems:
        ver = f"v{inflight[0]}.{inflight[1]}"
        cold = os.path.join(docs, "plan-cold")
        cold_file = next(
            (n for n in sorted(os.listdir(cold)) if n.startswith(ver)), None
        ) if os.path.isdir(cold) else None
        if cold_file:
            cold_text = read(os.path.join(cold, cold_file))
            lines = re.findall(r"^- (C\d+)[：:]\s*(.+)$", cold_text, re.M)
            # A cold plan with no checkpoint list agrees with any archive.
            if not lines:
                problems.append(
                    f"plan-cold/{cold_file} lists no `- C{{N}}：` checkpoints — "
                    f"this check is reading air, and a version with no shape "
                    f"written down cannot disagree with what was built"
                )
            listed = dict(lines)
            for name in archives:
                m = re.match(rf"{re.escape(ver)}-c(\d+)-hot\.md$", name)
                if not m:
                    continue
                cp = f"C{int(m.group(1))}"
                if cp not in listed:
                    problems.append(
                        f"{ARCHIVE}/{name} was archived and plan-cold/{cold_file} "
                        f"has no `- {cp}：` line. The cold plan is the version's "
                        f"shape; an archive it does not know about means the "
                        f"shape is describing a different version"
                    )

    # Everything else beside the layers, claimed by name.
    for name in sorted(os.listdir(docs)):
        rel = name + "/" if os.path.isdir(os.path.join(docs, name)) else name
        if rel in BESIDE or rel in (HOT, AWAITING, "roadmap.md", "plan-cold/"):
            continue
        if re.fullmatch(r"v\d+\.md", rel):
            continue
        problems.append(
            f"{DOCS}/{rel} is claimed by nothing. It is either one of the four "
            f"layers or it belongs in BESIDE with a line saying what it is — "
            f"going unlisted must never be how a file stays"
        )

    for entry in list(BESIDE):
        if not os.path.exists(os.path.join(docs, entry.rstrip("/"))):
            problems.append(f"BESIDE lists {entry} and it is gone — drop the line")

    # The rule and the gate have to agree on what layer three is.
    rules = os.path.join(root, ".claude", "CLAUDE.md")
    rule_note = ""
    if not os.path.isfile(rules):
        rule_note = " — CLAUDE.md is not in this tree, so section 0 was not reconciled"
    else:
        text = read(rules)
        for token in (HOT, AWAITING):
            if token not in text:
                problems.append(
                    f"CLAUDE.md never names {token}, and this gate judges layer "
                    f"three by it — one of the two moved first, and they have to "
                    f"be brought back level"
                )

    if problems:
        print("contract-scan: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    if segment:
        where = f"hot (v{segment[0]}.{segment[1]} {segment[2]})"
    elif awaiting:
        where = f"awaiting ({held})"
    else:
        print("contract-scan: FAIL")
        print("  - reached a state where no layer was read at all — this gate is broken")
        return 1

    print(
        f"contract-scan: clean — layer three is {where}, all four present, "
        f"{len(archives)} archived{rule_note}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
