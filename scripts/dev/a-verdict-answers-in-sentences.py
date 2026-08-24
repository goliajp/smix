#!/usr/bin/env python3
"""Every verdict answers in a sentence, including about rubbish.

The 7.0.0 ship reached A4 at thirty-one minutes and died with

    TypeError: '<' not supported between instances of 'NoneType' and 'str'

A red that arrives as a traceback says the gate broke, not what the gate
found, and those want opposite responses. A4's crash also ate the finding
underneath it — the payload had a window with a null package, which was
the very thing it was written to report.

Eight verdict scripts exist and one had a self-test. Writing seven more
by hand would test seven behaviours and miss the property they share, so
this drives all of them against inputs designed to break a parser:
nothing, empty, the right shape with nothing in it, and nulls where
strings are expected.

What is asserted is deliberately narrow — the shared property, not each
script's judgement:

  * it does not traceback;
  * it does not answer 0, because none of these inputs is evidence of
    anything and a verdict that passes on rubbish would pass on silence;
  * it says something on stderr, because an exit code with no sentence is
    the same failure one layer down.

The judgement half — "does A5 correctly read a fill's result label" —
belongs in per-script tests, and those go in beside this as they are
written. This is the floor, and it is the floor that was missing.

Usage:  a-verdict-answers-in-sentences.py [repo-root]
"""

import glob
import json
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.abspath(
    sys.argv[1]
    if len(sys.argv) > 1
    else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
)

# Inputs a parser can trip over, each a file the script is handed as its
# first argument. Named so a failure says which one did it.
DEGENERATE = {
    "an empty file": "",
    "not json at all": "<html>504 Gateway Timeout</html>",
    "an empty object": "{}",
    "the shape with nothing in it": '{"windows": [], "tree": {}, "children": []}',
    "nulls where strings go": json.dumps(
        {
            "windows": [{"package": None, "rootReadable": None, "type": None}],
            "tree": {"identifier": None, "role": None, "children": [{"value": None}]},
            "children": [{"identifier": None, "value": None}],
        }
    ),
}

def arg_shapes(path: str) -> list:
    """What each argument is, read off the script itself.

    Derived rather than listed, because a table of names and arities
    beside the scripts is a second copy of something already written
    down and the second copy is the one that goes stale.

    It also has to be right, or this sweep reports its own mistakes as
    the subject's: the first pass handed `android-a8-verdict.py` an app
    id where it wanted a second file and called the resulting
    FileNotFoundError a crash in the verdict. Two of the eight "findings"
    were the instrument.
    """
    src = open(path, encoding="utf-8").read()
    idx = [int(m) for m in re.findall(r"sys\.argv\[(\d+)\]", src)]
    n = max(idx) if idx else 1
    shapes = []
    for i in range(1, n + 1):
        a = f"sys.argv[{i}]"
        if re.search(r"int\(\s*" + re.escape(a), src):
            shapes.append("int")
        else:
            shapes.append("str")
    return shapes


def main() -> int:
    # A verdict is something you run. `verdict_io.py` beside them is the
    # shared loader they all import — a library, with no entry point, and
    # sweeping it reported it for "answering 0" when it had simply been
    # imported by a subprocess and done nothing. The instrument counting
    # its own parts as subjects is the second time this sweep did that.
    candidates = sorted(
        glob.glob(os.path.join(ROOT, "scripts", "release", "*verdict*.py"))
    )
    verdicts = [
        c
        for c in candidates
        if '__name__ == "__main__"' in open(c, encoding="utf-8").read()
    ]
    if not verdicts:
        # A sweep that found nothing agrees with every tree there is.
        print("verdict-sentences: CANNOT RUN — no verdict scripts under scripts/release/")
        return 1

    problems = []
    for script in verdicts:
        name = os.path.basename(script)
        shapes = arg_shapes(script)
        for label, body in DEGENERATE.items():
            with tempfile.NamedTemporaryFile(
                "w", suffix=".json", delete=False
            ) as f:
                f.write(body)
                path = f.name
            extra, spares = [], []
            for shape in shapes[1:]:
                if shape == "int":
                    extra.append("1")
                else:
                    # Everything else gets a real temp file holding the
                    # same rubbish. A path IS a string, so a script
                    # wanting a marker gets an odd marker and should
                    # judge it rather than crash — while a script wanting
                    # a second file gets one. Guessing which from the
                    # source failed: `tree_path, marker = sys.argv[1],
                    # sys.argv[2]` hides the `open` behind an assignment,
                    # and the sweep then reported its own
                    # FileNotFoundError as the verdict crashing.
                    with tempfile.NamedTemporaryFile(
                        "w", suffix=".json", delete=False
                    ) as g:
                        g.write(body)
                        spares.append(g.name)
                    extra.append(g.name)
            try:
                done = subprocess.run(
                    [sys.executable, script, path] + extra,
                    capture_output=True,
                    text=True,
                    timeout=30,
                )
            finally:
                os.unlink(path)
                for sp in spares:
                    os.unlink(sp)

            out = done.stdout + done.stderr
            if "Traceback (most recent call last)" in out:
                first = next(
                    (
                        l.strip()
                        for l in reversed(out.strip().splitlines())
                        if l.strip()
                    ),
                    "",
                )
                problems.append(
                    f"{name} given {label}: crashed instead of judging — {first}"
                )
            elif done.returncode == 0:
                problems.append(
                    f"{name} given {label}: answered 0. None of these inputs is "
                    f"evidence of anything, and a verdict that passes on rubbish "
                    f"passes on silence."
                )
            elif not done.stderr.strip():
                problems.append(
                    f"{name} given {label}: exit {done.returncode} and said nothing "
                    f"— a code with no sentence is the same failure one layer down."
                )

    if problems:
        print("verdict-sentences: RED — a verdict that cannot report its own finding\n")
        for p in problems:
            print(f"  {p}")
        return 1

    print(
        f"verdict-sentences: clean — {len(verdicts)} verdicts × "
        f"{len(DEGENERATE)} degenerate inputs, all answered in sentences"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
