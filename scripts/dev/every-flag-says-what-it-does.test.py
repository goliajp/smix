#!/usr/bin/env python3
"""The flag-description gate can still go red.

Six fixture files, each one thing away from a complete one. Three have to
be refused and three have to pass — a gate that cannot be made to fail is
indistinguishable from one that always passes, and one that refuses
correct code is worse than none.

Case 2 is the shape this gate was written for and the one a reader would
never guess from the symptom: the description is present, correct, and
attached to the wrong flag, because a field was inserted between a `///`
and the field it described. On the surface that reads as one flag
carrying another's sentence and the second carrying nothing.

Case 5 was added after removing the upward walk left this harness green.
A rule with no case is not a rule, and that one guards the direction that
would make the gate refuse code that is fine.
"""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "every-flag-says-what-it-does.py")

# Enough fields to clear MIN_FIELDS, written the way main.rs writes them.
FILLER = "\n".join(
    f"""        /// What flag{i} does.
        #[arg(long)]
        flag{i}: Option<String>,"""
    for i in range(45)
)

MAIN_RS = f"""\
#[derive(Subcommand, Debug)]
enum Cmd {{
    /// Tap an element.
    Tap {{
        /// Selector in `<kind>:<value>` shorthand.
        selector: String,
        /// Which language to read an `ocrText:` selector in.
        #[arg(long = "ocr-locale")]
        ocr_locale: Vec<String>,
        /// Runner port override.
        #[arg(long)]
        port: Option<u16>,
        /// An internal switch nobody should reach for.
        #[arg(long, hide = true)]
        secret: bool,
{FILLER}
    }},
}}
"""


def run(edits):
    with tempfile.TemporaryDirectory() as root:
        body = MAIN_RS
        for before, after in edits:
            assert before in body, f"fixture edit no longer applies: {before!r}"
            body = body.replace(before, after, 1)
        path = os.path.join(root, "crates", "smix-cli", "src", "main.rs")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(body)
        return subprocess.run(
            [sys.executable, GATE, root], capture_output=True, text=True
        )


CASES = [
    (
        "a flag with no description at all",
        [("        /// Runner port override.\n", "")],
        False,
        "`port` reaches the surface with no description",
    ),
    (
        "the description sits on the field before it",
        [
            (
                """        /// Which language to read an `ocrText:` selector in.
        #[arg(long = "ocr-locale")]
        ocr_locale: Vec<String>,
        /// Runner port override.
        #[arg(long)]
        port: Option<u16>,""",
                """        /// Runner port override.
        /// Which language to read an `ocrText:` selector in.
        #[arg(long = "ocr-locale")]
        ocr_locale: Vec<String>,
        #[arg(long)]
        port: Option<u16>,""",
            )
        ],
        False,
        "`port` reaches the surface with no description",
    ),
    (
        "a hidden flag needs no description",
        [
            (
                """        /// An internal switch nobody should reach for.
        #[arg(long, hide = true)]""",
                """        #[arg(long, hide = true)]""",
            )
        ],
        True,
        "1 hidden",
    ),
    (
        "a file with almost no flags in it",
        [(FILLER, "")],
        False,
        "expected at least",
    ),
    (
        "a flag whose attribute spans several lines",
        [
            (
                """        /// Runner port override.
        #[arg(long)]
        port: Option<u16>,""",
                """        /// Runner port override.
        #[arg(long)]
        #[arg(value_name = "PORT")]
        port: Option<u16>,""",
            )
        ],
        # Clean: the doc is there, two attribute lines away. Without the
        # upward walk the reader stops at the second `#[arg]` and calls a
        # described flag undescribed — which is how a gate starts refusing
        # correct code, and why removing that rule has to break something.
        True,
        "all described",
    ),
    (
        "a positional with no description",
        [("        /// Selector in `<kind>:<value>` shorthand.\n", "")],
        False,
        "`selector` reaches the surface with no description",
    ),
    (
        "a struct literal in a function body is not a surface",
        [
            (
                "}\n",
                """}

fn build() -> RunLease {
    Ok(Some(RunLease {
        leases,
        device_id: udid.to_string(),
    }))
}
""",
            )
        ],
        # Clean: those are locals in a function, not command-line
        # arguments, and the first draft judged six of them because it
        # matched on indentation alone.
        True,
        "all described",
    ),
    (
        "a file with no clap definition in it",
        [("#[derive(Subcommand, Debug)]\n", "")],
        # Without the derive there is no surface, and every field falls
        # outside every region — so the per-field rules all pass
        # vacuously. Only the "did I find any definitions at all" floor
        # can refuse this, which is why removing it has to break here.
        False,
        "reading nothing",
    ),
    ("the complete file", [], True, "all described"),
]

failures = []
for name, edits, want_clean, marker in CASES:
    result = run(edits)
    clean = result.returncode == 0
    output = result.stdout + result.stderr
    if clean != want_clean:
        failures.append(
            f"{name}: expected {'clean' if want_clean else 'FAIL'}, got "
            f"{'clean' if clean else 'FAIL'}\n{output}"
        )
    elif marker not in output:
        failures.append(f"{name}: verdict is right and says nothing about why\n{output}")
    if "Traceback" in output:
        failures.append(f"{name}: the gate crashed rather than judging\n{output}")

if failures:
    print("every-flag-says-what-it-does.test: FAILED")
    for failure in failures:
        print(f"  - {failure}")
    sys.exit(1)

print(f"every-flag-says-what-it-does.test: {len(CASES)} cases pass")
