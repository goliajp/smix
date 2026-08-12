#!/usr/bin/env python3
"""What `retired-claims-scan.py` must answer, fed trees rather than this one.

The scanner's whole job is to be red when a sentence an invariant
retired is still on a surface a reader reaches. A harness that only ever
ran it against this checkout would therefore prove nothing on the day it
matters: it would be green, and green is also what a scanner that reads
nothing prints.

So every case here builds a git repository of its own. Enumeration is by
`git ls-files` in both the fixture and the real tree — the same call, not
a test-only path — because the failure that keeps recurring in this
repository is a harness that passes for an implementation which cannot
read real input.

The fixture's shape is derived from the scanner's own GOVERNED and
EXEMPT columns rather than hand-copied, so adding a column entry does not
silently leave the fixture describing the previous release.
"""

from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SCAN = os.path.join(ROOT, "scripts", "dev", "retired-claims-scan.py")

problems: list[str] = []

if not os.path.isfile(SCAN):
    print("retired-claims-scan.test: FAIL")
    print(f"  - {os.path.relpath(SCAN, ROOT)} does not exist")
    sys.exit(1)

spec = importlib.util.spec_from_file_location("retired_claims_scan", SCAN)
assert spec and spec.loader
scan = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scan)


def run(root: str) -> tuple[int, str]:
    out = subprocess.run(
        [sys.executable, SCAN, "--root", root],
        capture_output=True,
        text=True,
        check=False,
    )
    return out.returncode, out.stdout + out.stderr


def fixture(tmp: str) -> None:
    """A tree shaped like this repository's, materialised from the columns."""
    for entry in list(scan.GOVERNED) + list(scan.EXEMPT):
        # A glob stands for whatever matches it; one match is enough.
        path = entry.replace("*", "4")
        full = os.path.join(tmp, path)
        if path.endswith("/"):
            os.makedirs(full, exist_ok=True)
            open(os.path.join(full, "index.md"), "w").write("# placeholder\n")
        else:
            os.makedirs(os.path.dirname(full) or tmp, exist_ok=True)
            open(full, "w").write("# placeholder\n")
    subprocess.run(["git", "init", "-q"], cwd=tmp, check=True)
    subprocess.run(["git", "add", "-A"], cwd=tmp, check=True)


def expect(label: str, ok: bool, detail: str) -> None:
    if not ok:
        problems.append(f"{label}: {detail}")


# 1. The sentence itself, on a surface a reader reaches. The scanner has
#    to say which invariant retired it and when, because a scanner that
#    only says "forbidden word" sends the reader to argue with the word.
with tempfile.TemporaryDirectory() as tmp:
    fixture(tmp)
    page = os.path.join(tmp, "web", "index.md")
    open(page, "w").write(
        "smix drives the simulators and never a physical device.\n"
    )
    subprocess.run(["git", "add", "-A"], cwd=tmp, check=True)
    code, out = run(tmp)
    expect("a retired sentence on a governed surface", code != 0, f"exit 0:\n{out}")
    expect("names the invariant", "§9 #1" in out, f"no '§9 #1' in:\n{out}")
    expect("names the day", "2026-08-06" in out, f"no '2026-08-06' in:\n{out}")
    expect("names the file", "web/index.md" in out, f"no path in:\n{out}")

# 2. The same sentence where quoting it is the point. A migration guide
#    that cannot say what the old behaviour was is not a migration guide.
with tempfile.TemporaryDirectory() as tmp:
    fixture(tmp)
    open(os.path.join(tmp, "docs", "migrating-to-4.md"), "w").write(
        "Before 4.0 smix drove the simulator and never a physical device.\n"
    )
    subprocess.run(["git", "add", "-A"], cwd=tmp, check=True)
    code, out = run(tmp)
    expect("a migration guide may quote what changed", code == 0, f"exit {code}:\n{out}")

# 3. This repository, as it stands. The scanner is worth nothing if the
#    surfaces it governs are not already clean.
code, out = run(ROOT)
expect("this checkout is clean", code == 0, f"exit {code}:\n{out}")

# 4. A new top-level surface that no column claims. Not being listed is
#    how `android-runner/sdk/README.md` kept a coordinate three minor
#    versions stale: nobody added the file to the list, so nobody read it.
with tempfile.TemporaryDirectory() as tmp:
    fixture(tmp)
    os.makedirs(os.path.join(tmp, "handbook"))
    open(os.path.join(tmp, "handbook", "intro.md"), "w").write("# hello\n")
    subprocess.run(["git", "add", "-A"], cwd=tmp, check=True)
    code, out = run(tmp)
    expect("an unclaimed root fails", code != 0, f"exit 0:\n{out}")
    expect("and is named", "handbook" in out, f"no 'handbook' in:\n{out}")

# 5. The register moved out from under the list. Every entry cites an
#    invariant and a day; if §9 stops recording that day, this list is
#    describing a rule that no longer says what it is quoted as saying.
#
#    The register is written but deliberately NOT staged: `.claude/` is
#    untracked in the real tree, and a first draft that staged it passed
#    this case on the wrong check — the unclaimed-root rule fired on
#    `.claude` itself, so the case stayed red with the date reconcile
#    removed. A fixture unlike the tree it stands for proves the wrong
#    thing quietly.
with tempfile.TemporaryDirectory() as tmp:
    fixture(tmp)
    os.makedirs(os.path.join(tmp, ".claude"))
    open(os.path.join(tmp, ".claude", "CLAUDE.md"), "w").write(
        "## 9. 不变量\n\n"
        "1. **设备后端按可得性显式分级**。物理设备是一等后端。\n"
        "2. **AI 层对外只暴露一个原语**。\n\n"
        "## 10. 决策记录\n"
    )
    code, out = run(tmp)
    expect("a register that dropped the date fails", code != 0, f"exit 0:\n{out}")
    expect("and says which entry", "§9 #1" in out, f"no '§9 #1' in:\n{out}")
    expect(
        "for the reason it is meant to",
        "no longer records that day" in out,
        f"red for some other reason:\n{out}",
    )

# 6. The register absent altogether — a checkout, where `.claude/` is not
#    version-controlled. The scanner still has work to do on `docs/` and
#    `web/`, so it does it, and says out loud which half it could not run.
with tempfile.TemporaryDirectory() as tmp:
    fixture(tmp)
    code, out = run(tmp)
    expect("a checkout without the register still scans", code == 0, f"exit {code}:\n{out}")
    expect(
        "and says so rather than reporting full coverage",
        "the invariant register is not in this checkout" in out,
        f"no stated downgrade in:\n{out}",
    )

if problems:
    print("retired-claims-scan.test: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print("retired-claims-scan.test: 6 cases pass")
