#!/usr/bin/env python3
"""What `selector-surface-scan.py` must answer, fed trees rather than this one.

The gate exists because a form can be written in flows and in four SDKs
and be missing from two surfaces for two majors with nothing red. A gate
for that has one failure mode worth guarding above all others: agreeing
that everything is declared because it read nothing. An empty set of
variants makes "every variant is declared" true of a surface that
declares nothing at all.

So every case builds its own tree, and this repository's own state is
checked last and separately — it is one sample, it is green today, and a
harness that only ever ran against it would be green on the day the scan
stopped parsing the enum.

Each red case asserts the exit code is 1 AND that the verdict says the
expected thing. A crash and a judgement leave the same code; the contract
gate paid for that lesson with three branches that were going red by
raising.
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SCAN = os.path.join(ROOT, "scripts", "dev", "selector-surface-scan.py")

problems: list[str] = []

if not os.path.isfile(SCAN):
    print("selector-surface-scan.test: FAIL")
    print(f"  - {os.path.relpath(SCAN, ROOT)} does not exist")
    sys.exit(1)

VARIANTS = [
    "Text", "Id", "Label", "Role", "Focused", "Anchor",
    "LocalizedText", "OcrText", "AnchorRelative", "Point", "Fallback",
]

# Which forms each fake surface claims to support; the rest are signed off.
SUPPORTED = {
    "crates/smix-adapter-maestro/src/parser.rs": ["Text", "Id", "Label", "Role", "Point"],
    "crates/smix-cli/src/act.rs": ["Text", "Id", "Label", "Role", "Point"],
    "crates/smix-mcp/src/selector_params.rs": [
        "Text", "Id", "Label", "Role", "Point", "OcrText",
    ],
}
TOKEN = {
    "Text": "text",
    "Id": "id",
    "Label": "label",
    "Role": "role",
    "Point": "point",
    "OcrText": "ocrText",
}


def write(path: str, body: str) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(body)


def tree(tmp: str, variants: list[str] | None = None) -> None:
    """A tree shaped like this repository's, and declared complete."""
    vs = VARIANTS if variants is None else variants
    # Two variants carry an optional companion field, matching the real
    # enum's shape: a form can be present on a surface and unusable there
    # because the parameter that makes it work cannot be given.
    def body_of(v: str) -> str:
        if v == "Role":
            return "        role: Role,\n        name: Option<Pattern>,\n        modifiers: Modifiers,\n"
        if v == "OcrText":
            return "        ocr_text: String,\n        locales: Vec<String>,\n        modifiers: Modifiers,\n"
        return "        modifiers: Modifiers,\n"

    enum = (
        "pub enum Selector {\n"
        + "".join(f"    {v} {{\n{body_of(v)}    }},\n" for v in vs)
        + "}\n"
    )
    write(os.path.join(tmp, "crates/smix-selector/src/lib.rs"), enum)
    for rel, supported in SUPPORTED.items():
        lines = []
        for v in vs:
            if v in supported:
                lines.append(f"// selector-surface: {v} — the `{TOKEN[v]}` form")
            else:
                lines.append(f"// selector-surface: {v} — none, not wired on this surface")
        body = "\n".join(lines) + "\n\n"
        body += "".join(f'let _ = "{t}";\n' for t in TOKEN.values())
        # A supported form's construction token, so the absence-side check
        # has something true to agree with.
        body += "".join(f"let _ = Selector::{v} {{}};\n" for v in supported)
        # The optional companion fields each surface must speak for.
        for v, f in [("Role", "name"), ("OcrText", "locales")]:
            if v in supported:
                body += f"// selector-surface-field: {v}.{f} — the `{f}` form\n"
        write(os.path.join(tmp, rel), body)


def run(root: str) -> tuple[int, str]:
    out = subprocess.run(
        [sys.executable, SCAN],
        capture_output=True,
        text=True,
        check=False,
        cwd=root,
        env={**os.environ, "PYTHONPATH": ""},
    )
    return out.returncode, out.stdout + out.stderr


def scan_in(tmp: str) -> tuple[int, str]:
    """Run the scan against a fake tree by copying it in beside a shim."""
    shim = os.path.join(tmp, "scripts", "dev", "selector-surface-scan.py")
    os.makedirs(os.path.dirname(shim), exist_ok=True)
    with open(SCAN, encoding="utf-8") as fh:
        src = fh.read()
    write(shim, src)
    out = subprocess.run(
        [sys.executable, shim], capture_output=True, text=True, check=False
    )
    return out.returncode, out.stdout + out.stderr


def expect(label: str, ok: bool, detail: str) -> None:
    if not ok:
        problems.append(f"{label}: {detail}")


def expect_verdict(label: str, code: int, out: str, needle: str) -> None:
    expect(label, code == 1, f"exit {code}, wanted 1:\n{out}")
    expect(f"{label} — a verdict, not a crash", "Traceback" not in out, f"raised:\n{out}")
    expect(f"{label} — says why", needle in out, f"no {needle!r} in:\n{out}")


# 1. The positive control. A gate that is always red is as useless as one
#    that is always green, and only this case can tell them apart.
with tempfile.TemporaryDirectory() as tmp:
    tree(tmp)
    code, out = scan_in(tmp)
    expect("a fully declared tree passes", code == 0, f"exit {code}:\n{out}")

# 2. One form undeclared on one surface — the shape `Point` had.
with tempfile.TemporaryDirectory() as tmp:
    tree(tmp)
    rel = os.path.join(tmp, "crates/smix-cli/src/act.rs")
    body = open(rel, encoding="utf-8").read()
    body = "\n".join(l for l in body.splitlines() if "selector-surface: Point" not in l)
    write(rel, body + "\n")
    code, out = scan_in(tmp)
    expect_verdict("an undeclared form fails", code, out, "Point")

# 3. The declaration says the form is written here and the file has no
#    such word — the line has become prose.
with tempfile.TemporaryDirectory() as tmp:
    tree(tmp)
    rel = os.path.join(tmp, "crates/smix-mcp/src/selector_params.rs")
    body = open(rel, encoding="utf-8").read().replace('let _ = "point";', "")
    write(rel, body)
    code, out = scan_in(tmp)
    expect_verdict("a declaration the file does not back fails", code, out, "prose")

# 4. The enum stops parsing. Every variant is declared, vacuously — this
#    is the case `gate/no-empty-predicate` is named for.
with tempfile.TemporaryDirectory() as tmp:
    tree(tmp)
    write(os.path.join(tmp, "crates/smix-selector/src/lib.rs"), "pub enum Other {}\n")
    code, out = scan_in(tmp)
    expect_verdict("an unparseable enum fails", code, out, "reading air")

# 5. A `none` with no reason behind it.
with tempfile.TemporaryDirectory() as tmp:
    tree(tmp)
    rel = os.path.join(tmp, "crates/smix-cli/src/act.rs")
    body = open(rel, encoding="utf-8").read().replace(
        "// selector-surface: Focused — none, not wired on this surface",
        "// selector-surface: Focused — none",
    )
    write(rel, body)
    code, out = scan_in(tmp)
    expect_verdict("a none with no reason fails", code, out, "no reason")

# 6. A declaration for a variant the enum no longer has.
with tempfile.TemporaryDirectory() as tmp:
    tree(tmp)
    rel = os.path.join(tmp, "crates/smix-mcp/src/selector_params.rs")
    body = open(rel, encoding="utf-8").read()
    write(rel, body + "// selector-surface: Xpath — none, never supported\n")
    code, out = scan_in(tmp)
    expect_verdict("a declaration for a gone variant fails", code, out, "Xpath")

# 7. A surface that signs everything off. Every other check passes —
#    each variant is declared, each `none` has its reason, no declaration
#    is prose — and the surface accepts nothing at all. Without the floor
#    this reads as a fully audited surface, which is how a scan agrees
#    with a file it never really read.
with tempfile.TemporaryDirectory() as tmp:
    tree(tmp)
    rel = os.path.join(tmp, "crates/smix-cli/src/act.rs")
    body = "\n".join(
        f"// selector-surface: {v} — none, signed off for a stated reason"
        for v in VARIANTS
    ) + "\n"
    write(rel, body)
    code, out = scan_in(tmp)
    expect_verdict("a surface that supports nothing fails", code, out, "not really read")

# 8. A `none` that is not true. The gate checked only the claims that
#    said yes for a checkpoint, so wiring `fallback` into MCP and leaving
#    its line at `none` passed clean. The rule this violates was written
#    in this repository before it was applied to this file.
with tempfile.TemporaryDirectory() as tmp:
    tree(tmp)
    rel = os.path.join(tmp, "crates/smix-mcp/src/selector_params.rs")
    body = open(rel, encoding="utf-8").read().replace(
        "// selector-surface: Fallback — none, not wired on this surface",
        "// selector-surface: Fallback — none, not wired on this surface",
    ) + "let _ = Selector::Fallback {};\n"
    write(rel, body)
    code, out = scan_in(tmp)
    expect_verdict("a none the code contradicts fails", code, out, "says it is not")

# 9. No surfaces at all. Both other floors guard the rows of the table;
#    this guards the table. Emptying it makes "every surface declares
#    every form" true of nothing, which is the exact failure this gate
#    exists to catch, turned on itself.
with tempfile.TemporaryDirectory() as tmp:
    tree(tmp)
    shim = os.path.join(tmp, "scripts", "dev", "selector-surface-scan.py")
    os.makedirs(os.path.dirname(shim), exist_ok=True)
    src = open(SCAN, encoding="utf-8").read()
    import re as _re
    emptied = _re.sub(r"SURFACES = \{.*?\n\}", "SURFACES = {}", src, count=1, flags=_re.S)
    assert "SURFACES = {}" in emptied, "the mutation did not take"
    write(shim, emptied)
    out = subprocess.run([sys.executable, shim], capture_output=True, text=True, check=False)
    expect_verdict(
        "no surfaces at all fails", out.returncode, out.stdout + out.stderr, "reading air"
    )

# 10. A form that is present and unusable: the surface takes `ocrText`
#     and says nothing about which languages it can be read in. That is
#     the shape a consumer hit — a Chinese dialog answered with "no
#     matching text" by a recogniser told to read English.
with tempfile.TemporaryDirectory() as tmp:
    tree(tmp)
    rel = os.path.join(tmp, "crates/smix-mcp/src/selector_params.rs")
    body = "\n".join(
        l
        for l in open(rel, encoding="utf-8").read().splitlines()
        if "selector-surface-field: OcrText.locales" not in l
    )
    write(rel, body + "\n")
    code, out = scan_in(tmp)
    expect_verdict("a form with no word for its parameter fails", code, out, "locales")

# 11. And the floor under it: an enum whose optional fields cannot be
#     parsed makes "every parameter is spoken for" true of nothing.
with tempfile.TemporaryDirectory() as tmp:
    tree(tmp)
    write(
        os.path.join(tmp, "crates/smix-selector/src/lib.rs"),
        "pub enum Selector {\n" + "".join(f"    {v} {{}},\n" for v in VARIANTS) + "}\n",
    )
    code, out = scan_in(tmp)
    expect_verdict("an enum with no parseable fields fails", code, out, "reading air")

# 12. This repository. Last, and never the only one.
code, out = run(ROOT)
expect("this repository is fully declared", code == 0, f"exit {code}:\n{out}")

if problems:
    print("selector-surface-scan.test: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print("selector-surface-scan.test: 12 cases pass")
