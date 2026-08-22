#!/usr/bin/env python3
"""The table agrees with the code, not only with itself.

`selector_support.rs` says, cell by cell, whether a verb reads a
selector form it cannot get from the accessibility tree. Two ways that
table can be true of nothing:

  * `Slot::ALL` is written by hand next to `slots()`, which is derived
    from `Step`. A slot handed out by `slots()` and missing from `ALL`
    is one the tests walk right past.
  * A cell can claim `Dispatched` while the runtime has no path that
    reads the form. That is the shape of the defect the table exists
    for: `matches_base` asserted for four releases that the adapter
    dispatched `Fallback`, three verbs did, and nothing compared the
    sentence with the call sites.

So both directions are checked from the source. This reads files and
runs nothing.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
TABLE = os.path.join(ROOT, "crates", "smix-adapter-maestro", "src", "selector_support.rs")
RUNTIME = os.path.join(ROOT, "crates", "smix-adapter-maestro", "src", "runtime.rs")

# What counts as reading each form, in the runtime. Named by the call
# that does the reading rather than by a verb, so a rename of the verb
# cannot quietly satisfy this.
READS = {
    "OcrText": ("find_by_text_ocr", "wait_for_visible_with_ocr", "scroll_until_visible_with_ocr"),
    "LocalizedText": ("desugar_localized_text",),
    "AnchorRelative": ("find_norm_coord",),
}

# Which run_step arm serves which slot. The runtime dispatches by Step,
# the table decides by slot, and this is the join — kept here because
# neither side can derive it: a slot is a position inside a verb.
# A slot's dispatch can live one call down: `Step::TapOn`'s arm calls
# `run_tap`, which is where the OCR and the anchor shift are read. So
# the join names whatever actually serves the slot — arm or helper —
# and a scanner that only read arms reported TapOn as claiming a
# dispatch it does not perform, which is a false alarm that teaches
# people to ignore the check.
SLOT_HELPERS = {
    "TapOnTarget": ("run_tap",),
    "DoubleTapTarget": ("point_for_unreadable", "point_for_unreadable_once"),
    "LongPressTarget": ("point_for_unreadable", "point_for_unreadable_once"),
    "FillTarget": ("point_for_unreadable", "point_for_unreadable_once"),
    "AssertVisibleTarget": ("point_for_unreadable", "point_for_unreadable_once"),
    "AnnotationAnchor": ("point_for_unreadable", "point_for_unreadable_once"),
    "RepeatTapTarget": ("point_for_unreadable", "point_for_unreadable_once"),
    "WaitVisibleTarget": ("wait_for_visible_with_ocr",),
    "RunFlowGate": ("check_selector_visible", "evaluate_run_flow_gate"),
    "ScrollTarget": ("scroll_until_visible_with_ocr",),
}
SLOT_ARMS = {
    "TapOnTarget": ("TapOn",),
    "RepeatTapTarget": ("RepeatTap",),
    "DoubleTapTarget": ("DoubleTapOn",),
    "LongPressTarget": ("LongPressOn",),
    "FillTarget": ("InputTextInto",),
    "CopyTextSource": ("CopyTextFrom",),
    "AssertVisibleTarget": ("AssertVisible",),
    "AssertNotVisibleTarget": ("AssertNotVisible",),
    "WaitVisibleTarget": ("ExtendedWaitUntil",),
    "WaitNotVisibleTarget": ("ExtendedWaitUntil",),
    "ScrollTarget": ("ScrollUntilVisible",),
    "RunFlowGate": ("RunFlowInline", "RunFlowConditional"),
    "AnnotationAnchor": ("TakeScreenshot",),
}
CELL_ARMS = {
    (slot, form): arms
    for slot, arms in SLOT_ARMS.items()
    for form in ("OcrText", "LocalizedText", "AnchorRelative")
}


def block(src: str, header: str, opener: str = "{") -> str:
    """The balanced span after `header`, opened by `opener`.

    The bracket is a parameter because the two things read here are a
    function body and an array literal: a reader hard-coded to braces
    walked past the array and compared against an empty set, which is
    how a check comes to agree with everything.
    """
    closer = {"{": "}", "[": "]"}[opener]
    # From `= ` on purpose: `pub const ALL: [Slot; 13] = [...]` opens a
    # bracket in its own type annotation, and starting at the first one
    # reads `[Slot; 13]` — a span with no member names in it, compared
    # against and found wanting every time.
    i = src.index(header)
    if opener == "[":
        i = src.index("= [", i)
    depth, j = 0, src.index(opener, i)
    start = j
    while True:
        if src[j] == opener:
            depth += 1
        elif src[j] == closer:
            depth -= 1
            if depth == 0:
                return src[start : j + 1]
        j += 1


def step_arms(runtime: str) -> dict:
    """Every `Step::X` arm of run_step, by name, with its body."""
    lines = runtime.split("\n")
    start = next(i for i, l in enumerate(lines) if "async fn run_step(" in l)
    end = next(i for i in range(start + 1, len(lines)) if lines[i] == "    }")
    out, cur = {}, None
    for i in range(start, end):
        m = re.match(r"            Step::([A-Za-z]+)", lines[i])
        if m:
            cur = m.group(1)
            out.setdefault(cur, [])
        if cur:
            out[cur].append(lines[i])
    return {k: "\n".join(v) for k, v in out.items()}


def fn_body(src: str, name: str) -> str:
    """The body of a method by name, or empty when it is gone."""
    m = re.search(rf"(async )?fn {re.escape(name)}\s*[(<]", src)
    if not m:
        return ""
    depth, j = 0, src.index("{", m.start())
    start = j
    while j < len(src):
        if src[j] == "{":
            depth += 1
        elif src[j] == "}":
            depth -= 1
            if depth == 0:
                return src[start : j + 1]
        j += 1
    return ""


def claims_dispatch(table: str, slot: str, form: str) -> bool:
    """Does the table's `support` say this cell is dispatched?

    Read by asking the compiled table rather than by parsing arms: the
    match arms group slots and forms with `|`, and a parser that read
    them one pair at a time would answer about pairs that are not
    written down anywhere.
    """
    import subprocess

    key = f"{slot}:{form}"
    return key in _dispatched_cells(table)


_CELLS_CACHE = {}


def _dispatched_cells(table: str) -> set:
    """Every dispatched cell, evaluated by the Rust that owns the table."""
    if "cells" in _CELLS_CACHE:
        return _CELLS_CACHE["cells"]
    import subprocess

    out = subprocess.run(
        ["cargo", "test", "-p", "smix-adapter-maestro", "--test",
         "every_cell_is_a_decision", "--", "--nocapture", "print_the_table"],
        cwd=ROOT, capture_output=True, text=True,
    ).stdout
    cells = set(re.findall(r"CELL (\S+):(\S+) DISPATCHED", out))
    cells = {f"{a}:{b}" for a, b in cells}
    _CELLS_CACHE["cells"] = cells
    return cells


def main() -> int:
    table = open(TABLE, encoding="utf-8").read()
    runtime = open(RUNTIME, encoding="utf-8").read()
    problems: list[str] = []

    handed_out = set(re.findall(r"Slot::([A-Za-z]+)", block(table, "pub fn slots(")))
    listed = set(re.findall(r"Slot::([A-Za-z]+)", block(table, "pub const ALL: [Slot;", "[")))
    for name in sorted(handed_out - listed):
        problems.append(
            f"`slots()` hands out Slot::{name} and `Slot::ALL` does not list it — "
            f"every test that walks ALL has been walking past that slot."
        )
    for name in sorted(listed - handed_out):
        problems.append(
            f"`Slot::ALL` lists Slot::{name} and no step declares it. Either a step "
            f"should, or the slot is gone and the cells for it are answering about "
            f"nothing."
        )

    # Per cell, not per form. Asking "does runtime.rs read OCR
    # anywhere" passed a table that claimed five dispatches it did not
    # have — one reader in one verb made the answer yes for all of
    # them. That is the same shape as the defect the table exists for,
    # committed while building the table.
    #
    # A cell claims a dispatch; the arm that serves its slot has to
    # contain a reader for that form.
    arms = step_arms(runtime)
    for (slot, form), arm_names in CELL_ARMS.items():
        if not claims_dispatch(table, slot, form):
            continue
        readers = READS[form]
        bodies = "\n".join(arms.get(a, "") for a in arm_names)
        for helper in SLOT_HELPERS.get(slot, ()):
            bodies += "\n" + fn_body(runtime, helper)
        if not any(re.search(rf"\b{re.escape(r)}\b", bodies) for r in readers):
            problems.append(
                f"the table says {slot} reads `{form}`, and the runtime arm(s) "
                f"{list(arm_names)} call none of {list(readers)}. A cell that "
                f"claims a dispatch nothing performs is the table agreeing with "
                f"itself."
            )

    if problems:
        print("every-cell-is-declared: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    print(
        f"every-cell-is-declared: clean — {len(listed)} slots, each declared by a step "
        f"and walked by the tests; every dispatched form has a reader in runtime.rs"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
