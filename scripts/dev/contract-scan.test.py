#!/usr/bin/env python3
"""What `contract-scan.py` must answer, fed trees rather than this one.

The gate's whole subject is a gap: the moment after a checkpoint is
archived and before the next segment is hot. §0 says all four layers are
present at every moment; §6 says the next segment is not opened without
the user saying so. Between those two, at every version boundary, there
is a window neither rule can describe — and two of them in this cycle
were written down as carelessness before anyone noticed the contract
made them inevitable.

So the thing under test is not "is the file there". It is whether the
gap has been *claimed*: archived, and either hot again or holding a
written note saying whose word it waits on. Waiting is a legal state and
has to be expressible, which means the harness has to prove the gate can
tell a claimed gap from a lost one — the same tree, differing by one
note, red then green.

Every case builds its own tree. This repository's own state is checked
too, but last and separately: it is one sample, it is green today, and a
harness that only ever ran against it would be green on the day the gate
stopped reading anything at all.
"""

from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SCAN = os.path.join(ROOT, "scripts", "dev", "contract-scan.py")

problems: list[str] = []

if not os.path.isfile(SCAN):
    print("contract-scan.test: FAIL")
    print(f"  - {os.path.relpath(SCAN, ROOT)} does not exist")
    sys.exit(1)

spec = importlib.util.spec_from_file_location("contract_scan", SCAN)
assert spec and spec.loader
scan = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scan)

# Line by line rather than one triple-quoted block: hygiene-scan strips
# quoted spans before it looks for noise, and its quote pattern does not
# span lines — so a heredoc-shaped fixture reads to it as unquoted
# chatter naming a plan file and a checkpoint. This is data; written this
# way it is seen as data.
AWAITING = "\n".join([
    "# plan-hot 空缺 — 等拍板",
    "",
    "- 已归档：`.claude/docs/archive/plan-history/v4.2-c1-hot.md`",
    "- 下一段：v4.2 C2",
    "- 自：2026-08-12",
    "- 等的是：用户明确说「开始 C2」（CLAUDE.md §6 触发条件 2）",
    "",
])

HOT = "# plan-hot — v4.2 到 C2：契约门\n\n## 目标 checkpoint\n"


def run(root: str) -> tuple[int, str]:
    out = subprocess.run(
        [sys.executable, SCAN, "--root", root],
        capture_output=True,
        text=True,
        check=False,
    )
    return out.returncode, out.stdout + out.stderr


def write(path: str, body: str) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(body)


def tree(tmp: str) -> str:
    """A `.claude/docs/` shaped like this one, with layer three absent."""
    docs = os.path.join(tmp, ".claude", "docs")
    write(os.path.join(tmp, "Cargo.toml"), 'version = "4.1.0"\n')
    write(os.path.join(docs, "roadmap.md"), "# roadmap\n\n- v4.2：说出去的话与契约\n")
    write(os.path.join(docs, "v4.md"), "# v4 边界\n\n## 决策日志\n")
    write(
        os.path.join(docs, "plan-cold", "v4.2-claims-and-contract.md"),
        "# cold\n\n- C1：退役断言门\n- C2：契约门\n",
    )
    write(
        os.path.join(docs, "archive", "plan-history", "v4.2-c1-hot.md"),
        "# plan-hot — v4.2 到 C1：退役断言门\n",
    )
    for beside in scan.BESIDE:
        p = os.path.join(docs, beside)
        if beside.endswith("/"):
            os.makedirs(p, exist_ok=True)
        elif not os.path.exists(p):
            write(p, "# placeholder\n")
    return docs


def expect(label: str, ok: bool, detail: str) -> None:
    if not ok:
        problems.append(f"{label}: {detail}")


def expect_verdict(label: str, code: int, out: str) -> None:
    """Red, and red because it judged — not because it fell over.

    A first draft of the scanner reached three of its branches with a
    file that was not there or a match that was None, so it raised. An
    exception and a verdict leave the same exit code, and three of the
    cases below were passing on the traceback: removing the rule each
    one tested left them red anyway. A gate that crashes is not a gate
    that disagrees with you.
    """
    expect(label, code != 0, f"exit 0:\n{out}")
    expect(f"{label} — a verdict, not a crash", "Traceback" not in out, f"raised:\n{out}")
    expect(
        f"{label} — leads with FAIL",
        out.startswith("contract-scan: FAIL"),
        f"not a verdict:\n{out}",
    )


# 1. Archived, and neither hot nor waiting. This is the only one of the
#    three states that is actually a loss, and it is the one that looked
#    identical to the other two from outside.
with tempfile.TemporaryDirectory() as tmp:
    tree(tmp)
    code, out = run(tmp)
    expect_verdict("an unclaimed gap fails", code, out)
    expect(
        "and names the archive it was left at",
        "v4.2-c1-hot.md" in out,
        f"no archive named in:\n{out}",
    )
    expect(
        "and says what is missing rather than which file",
        "waiting on anyone" in out,
        f"reads as a file-existence complaint:\n{out}",
    )

# 2. The same tree, one note added. Waiting on a person is not a defect;
#    a gate that cannot say so makes the honest state indistinguishable
#    from the careless one, which is how two of these got recorded as
#    carelessness.
with tempfile.TemporaryDirectory() as tmp:
    docs = tree(tmp)
    write(os.path.join(docs, "plan-hot.awaiting.md"), AWAITING)
    code, out = run(tmp)
    expect("a claimed gap passes", code == 0, f"exit {code}:\n{out}")
    expect("and says whose word it waits on", "awaiting (" in out, f"silent about it:\n{out}")

# 3. Layer three is one position. Both at once means the segment is
#    being worked and waited on simultaneously, which describes nothing.
with tempfile.TemporaryDirectory() as tmp:
    docs = tree(tmp)
    write(os.path.join(docs, "plan-hot.md"), HOT)
    write(os.path.join(docs, "plan-hot.awaiting.md"), AWAITING)
    code, out = run(tmp)
    expect_verdict("hot and waiting at once fails", code, out)
    expect("and says so", "EXACTLY ONE" in out, f"no reason given:\n{out}")

# 4. A note that does not say what it waits on is not a note. Four
#    fields, and the one that carries the meaning is the last.
with tempfile.TemporaryDirectory() as tmp:
    docs = tree(tmp)
    write(
        os.path.join(docs, "plan-hot.awaiting.md"),
        "\n".join(l for l in AWAITING.splitlines() if not l.startswith("- 等的是")) + "\n",
    )
    code, out = run(tmp)
    expect_verdict("a note missing a field fails", code, out)
    expect("and names the field", "等的是" in out, f"no field named in:\n{out}")

# 5. A hot plan that will not say which segment it is. Every other layer
#    is located from that line, so a plan which does not declare itself
#    takes the whole check down with it quietly.
with tempfile.TemporaryDirectory() as tmp:
    docs = tree(tmp)
    write(os.path.join(docs, "plan-hot.md"), "# 计划\n\n## 目标 checkpoint\n")
    code, out = run(tmp)
    expect_verdict("a plan that will not name its segment fails", code, out)

# 6. Hot, and the shape this repository is in while the gate is written.
with tempfile.TemporaryDirectory() as tmp:
    docs = tree(tmp)
    write(os.path.join(docs, "plan-hot.md"), HOT)
    code, out = run(tmp)
    expect("a hot segment passes", code == 0, f"exit {code}:\n{out}")

# 7. The boundary file for the major that is actually shipping. v4's
#    decisions went into v2.md for a while because nobody noticed v4.md
#    was never opened — the log kept working, in the wrong book.
with tempfile.TemporaryDirectory() as tmp:
    docs = tree(tmp)
    write(os.path.join(docs, "plan-hot.md"), HOT)
    write(os.path.join(tmp, "Cargo.toml"), 'version = "5.0.0"\n')
    code, out = run(tmp)
    expect_verdict("a major with no boundary file fails", code, out)
    expect("and names it", "v5.md" in out, f"no v5.md in:\n{out}")

# 8. Layer four for the minor in flight. The hot plan says v4.2; if no
#    cold plan covers it, the segment is being worked without the version
#    it belongs to ever having been scoped.
with tempfile.TemporaryDirectory() as tmp:
    docs = tree(tmp)
    write(os.path.join(docs, "plan-hot.md"), HOT)
    os.remove(os.path.join(docs, "plan-cold", "v4.2-claims-and-contract.md"))
    code, out = run(tmp)
    expect_verdict("a segment with no cold plan fails", code, out)
    expect("and names the version", "v4.2" in out, f"no version in:\n{out}")

# 9. A note left pointing at the segment before last. Staleness is judged
#    mechanically — the archive it names must be the newest on its line —
#    because a date threshold teaches people to touch up the date.
with tempfile.TemporaryDirectory() as tmp:
    docs = tree(tmp)
    write(os.path.join(docs, "plan-hot.awaiting.md"), AWAITING)
    write(
        os.path.join(docs, "archive", "plan-history", "v4.2-c2-hot.md"),
        "# plan-hot — v4.2 到 C2：契约门\n",
    )
    code, out = run(tmp)
    expect_verdict("a note stuck on the previous segment fails", code, out)
    expect("and names the newer archive", "v4.2-c2-hot.md" in out, f"no archive in:\n{out}")

# 10. Something beside the four layers that neither column claims. This
#     is the rule `retired-claims-scan` and `android-subject-scan` both
#     carry: not being listed is never how a thing gets in.
with tempfile.TemporaryDirectory() as tmp:
    docs = tree(tmp)
    write(os.path.join(docs, "plan-hot.md"), HOT)
    write(os.path.join(docs, "notes.md"), "# 随手记\n")
    code, out = run(tmp)
    expect_verdict("an unclaimed entry beside the layers fails", code, out)
    expect("and names it", "notes.md" in out, f"no name in:\n{out}")

# 11. Layer one has to carry the version that is actually in flight. A
#     roadmap that stops one minor short is the path with the current
#     step missing from it.
with tempfile.TemporaryDirectory() as tmp:
    docs = tree(tmp)
    write(os.path.join(docs, "plan-hot.md"), HOT)
    write(os.path.join(docs, "roadmap.md"), "# roadmap\n\n- v4.1：Android 装包\n")
    code, out = run(tmp)
    expect_verdict("a roadmap missing the segment in flight fails", code, out)
    expect("and names the version", "v4.2" in out, f"no version in:\n{out}")

# 12. The rule and the gate must agree about what layer three is. This
#     gate enforces a shape; if §0 stops describing that shape, one of
#     the two moved first and the other is now enforcing a form the
#     constitution does not know about.
#
#     These last two rules were written with no case covering either, and
#     the mutation sweep is how that surfaced: removing each one left all
#     eleven cases green. An assertion that was never red is an assertion
#     nobody has checked.
with tempfile.TemporaryDirectory() as tmp:
    docs = tree(tmp)
    write(os.path.join(docs, "plan-hot.md"), HOT)
    write(
        os.path.join(tmp, ".claude", "CLAUDE.md"),
        "## 0. 文档分层\n\n| [3] 热计划 | `.claude/docs/plan-hot.md` | 唯一 |\n",
    )
    code, out = run(tmp)
    expect_verdict("a constitution that does not know the shape fails", code, out)
    expect(
        "and names the form it has not heard of",
        "plan-hot.awaiting.md" in out,
        f"no form named in:\n{out}",
    )

# 13. Both named — the shape this repository is in after §0 was rewritten.
with tempfile.TemporaryDirectory() as tmp:
    docs = tree(tmp)
    write(os.path.join(docs, "plan-hot.md"), HOT)
    write(
        os.path.join(tmp, ".claude", "CLAUDE.md"),
        "## 0. 文档分层\n\n| [3] 热计划 / 交接单 | `plan-hot.md` 或 "
        "`plan-hot.awaiting.md` | 二者恰居其一 |\n",
    )
    code, out = run(tmp)
    expect("a constitution that names both passes", code == 0, f"exit {code}:\n{out}")

# 14. A checkpoint archived against a cold plan that never heard of it.
#     The shape and what was built have parted, and it is the shape that
#     is fiction. v4.3's cold plan described runner-attach work through
#     two checkpoints of selector work, and the rearrangement had been
#     left in the previous checkpoint's closing actions — where it was
#     skipped, because a documentation edit has nothing that goes red.
with tempfile.TemporaryDirectory() as tmp:
    docs = tree(tmp)
    write(os.path.join(docs, "plan-hot.md"), HOT)
    write(
        os.path.join(docs, "plan-cold", "v4.2-claims-and-contract.md"),
        "# cold\n\n- C2：契约门\n- C3：账本\n",
    )
    code, out = run(tmp)
    expect_verdict("an archive the cold plan does not list fails", code, out)
    expect("and names the checkpoint", "C1" in out, f"no C1 in:\n{out}")

# 15. A cold plan with no checkpoint list. Every archive is unlisted, and
#     a check written the other way round would agree with anything.
with tempfile.TemporaryDirectory() as tmp:
    docs = tree(tmp)
    write(os.path.join(docs, "plan-hot.md"), HOT)
    write(os.path.join(docs, "plan-cold", "v4.2-claims-and-contract.md"), "# cold\n")
    code, out = run(tmp)
    expect_verdict("a cold plan with no checkpoints fails", code, out)
    expect("and says it is reading air", "reading air" in out, f"no reason in:\n{out}")

# 16. This repository. Last, and never the only one — see the header.
#    On a bare checkout there is no `.claude/docs/` to read, and the case
#    says which one it dropped rather than counting itself as coverage.
if os.path.isdir(os.path.join(ROOT, ".claude", "docs")):
    code, out = run(ROOT)
    expect("this repository is claimed", code == 0, f"exit {code}:\n{out}")
    checked = 16
else:
    print(
        "note: no .claude/docs/ here — case 16 (this repository) was not run",
        file=sys.stderr,
    )
    checked = 15

if problems:
    print("contract-scan.test: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print(f"contract-scan.test: {checked} cases pass")
