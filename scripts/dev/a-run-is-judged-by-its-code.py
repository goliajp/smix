#!/usr/bin/env python3
"""An e2e script runs a flow through `smix-run`, so a failure is judged by
the code smix reported.

    out="$("$SMIX" run --device "$D" "$flow" 2>&1)" || rc=$?
    [ "$rc" -eq 0 ] || fail "the scroll stopped with the row still crossing the edge"

Any non-zero exit reads as the rule's failure. When the runner answered
half a body, smix printed `FAIL [DRIVER_ERROR]: …` — and the script's
next line said something about the screen that nobody had looked at.
`scripts/lib/smix-run` hands a verdict about the screen back to the
script and ends the script on anything else, naming smix's own code.

Flagged, in every `scripts/**/*.sh`:

* `"$SMIX" run` / `"$SMIX_BIN" run` (and unquoted / braced spellings),
  continuation lines joined, inside `$( … )` too;
* a bare `smix run` in command position — the binary on PATH, or a
  `smix()` that forwards to it (four scripts had one ending `|| true`,
  which made the `|| fail` after their run unreachable);
* a `# raw run:` note with no reason, or one with no raw run under it;
* a script calling `"$SMIX_RUN"` that does not source `judged-run.sh`
  (directly or through `e2e-binary.sh`).

A call that means to see a failure smix-run would end the script on — a
refusal before any step, an undefined variable — keeps the raw form
with `# raw run: <why>` on its line or the comment line above it.

Red as well when there is no script to scan, or none calls
`$SMIX_RUN`: a scan over nothing would pass.
"""

from __future__ import annotations

import argparse
import os
import re
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

RAW = re.compile(r'(?:"\$\{?SMIX(?:_BIN)?\}?"|\$\{?SMIX(?:_BIN)?\}?)\s+run(?=\s|$|;|\))')
WRAPPED = re.compile(r'(?:^|[;&|(!]\s*|\bif\s+|\bthen\s+|\$\(\s*|^\s*(?:\w+=\S+\s+)+)smix\s+run(?=\s|$)')
NOTE = re.compile(r"#\s*raw run:(.*)$")
WRAPPER_CALL = re.compile(r'"\$\{?SMIX_RUN\}?"')
SOURCES = re.compile(r"^\s*(?:\.|source)\s+\S*(?:judged-run|e2e-binary)\.sh", re.M)
HEREDOC = re.compile(r"<<-?\s*(['\"]?)(\w+)\1")


def logical_lines(src: str) -> list[tuple[int, str, bool]]:
    """(first line number, text, is-code) with continuations joined and
    heredoc bodies marked as not code."""
    out: list[tuple[int, str, bool]] = []
    buf, start, until = "", 0, None
    for n, line in enumerate(src.splitlines(), 1):
        if until is not None:
            out.append((n, line, False))
            if line.strip() == until:
                until = None
            continue
        if not buf:
            start = n
        if line.rstrip().endswith("\\"):
            buf += line.rstrip()[:-1] + " "
            continue
        joined = buf + line
        buf = ""
        out.append((start, joined, True))
        m = HEREDOC.search(code_part(joined))
        if m:
            until = m.group(2)
    return out


def code_part(line: str) -> str:
    """The line up to an unquoted `#` that starts a comment."""
    sq = dq = False
    for i, c in enumerate(line):
        if c == "'" and not dq:
            sq = not sq
        elif c == '"' and not sq:
            dq = not dq
        elif c == "#" and not sq and not dq and (i == 0 or line[i - 1] in " \t;"):
            return line[:i]
    return line


def strip_quoted(code: str) -> str:
    """Quoted prose removed, so `fail "smix run did not answer"` is not a call.
    A quoted variable (`"$SMIX"`) is kept: that is how the binary is named."""
    out, i = [], 0
    while i < len(code):
        c = code[i]
        if c in "'\"":
            j = code.find(c, i + 1)
            j = len(code) - 1 if j == -1 else j
            seg = code[i : j + 1]
            keep = c == '"' and re.fullmatch(r'"\$\{?SMIX(?:_BIN|_RUN)?\}?"', seg)
            out.append(seg if keep else c + c)
            i = j + 1
            continue
        out.append(c)
        i += 1
    return "".join(out)


def substitutions(line: str) -> list[str]:
    """Bodies of every `$( … )` on the line, nested ones included — a call
    inside `out="$( … )"` sits in a quoted string."""
    bodies: list[str] = []
    i = 0
    while True:
        i = line.find("$(", i)
        if i == -1:
            return bodies
        j, depth, sq = i + 2, 1, False
        while j < len(line) and depth:
            c = line[j]
            if c == "'" and not sq and line.count("'", j) >= 2:
                k = line.find("'", j + 1)
                j = k + 1
                continue
            if c == "\\":
                j += 2
                continue
            if line.startswith("$(", j):
                depth += 1
                j += 2
                continue
            if c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
            j += 1
        bodies.append(line[i + 2 : j - 1])
        i += 2


def calls_raw(code: str) -> bool:
    for part in [code] + substitutions(code):
        bare = strip_quoted(part)
        if RAW.search(bare) or WRAPPED.search(bare):
            return True
    return False


def scan_file(path: str, rel: str) -> tuple[list[str], int, int]:
    src = open(path, encoding="utf-8", errors="replace").read()
    lines = logical_lines(src)
    problems: list[str] = []
    wrapper_calls = raw_ok = 0
    pending_note: tuple[int, str] | None = None
    for n, text, is_code in lines:
        if not is_code:
            continue
        note = NOTE.search(text)
        code = code_part(text)
        if not code.strip():
            if note:
                if pending_note:
                    problems.append(f"{rel}:{pending_note[0]}: a `# raw run:` note with no raw run under it")
                pending_note = (n, note.group(1).strip())
            continue
        if any(WRAPPER_CALL.search(strip_quoted(p)) for p in [code] + substitutions(code)):
            wrapper_calls += 1
        is_raw = calls_raw(code)
        reason = note.group(1).strip() if note else (pending_note[1] if pending_note else None)
        where = n if note or not pending_note else pending_note[0]
        if is_raw:
            if reason is None:
                problems.append(
                    f"{rel}:{n}: `smix run` called directly — a failure it reports is read by "
                    f'this script\'s rule, whatever smix said. Use "$SMIX_RUN", or add '
                    f"`# raw run: <why>`: {code.strip()[:120]}"
                )
            elif len(reason.split()) < 3:
                problems.append(f"{rel}:{where}: `# raw run:` says no reason: {reason!r}")
            else:
                raw_ok += 1
        elif note or pending_note:
            problems.append(f"{rel}:{where}: a `# raw run:` note with no raw run under it")
        pending_note = None
    if pending_note:
        problems.append(f"{rel}:{pending_note[0]}: a `# raw run:` note with no raw run under it")
    if wrapper_calls and not SOURCES.search(src):
        problems.append(
            f"{rel}: calls \"$SMIX_RUN\" without sourcing judged-run.sh (or e2e-binary.sh) — "
            "it is unset there, and nothing would end the script on a failure that is not a verdict"
        )
    return problems, wrapper_calls, raw_ok


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=REPO)
    root = ap.parse_args().root
    scripts_dir = os.path.join(root, "scripts")
    subjects: list[tuple[str, str]] = []
    for dirpath, _, files in os.walk(scripts_dir):
        for f in sorted(files):
            if not f.endswith(".sh"):
                continue
            path = os.path.join(dirpath, f)
            rel = os.path.relpath(path, root)
            if rel == os.path.join("scripts", "lib", "smix-run"):
                continue
            subjects.append((path, rel))
    if not subjects:
        print(f"a-run-is-judged-by-its-code: no .sh under {scripts_dir} — nothing was scanned")
        return 1
    problems: list[str] = []
    calls = raw = 0
    for path, rel in sorted(subjects, key=lambda s: s[1]):
        p, c, r = scan_file(path, rel)
        problems += p
        calls += c
        raw += r
    if calls == 0:
        problems.append("no script calls \"$SMIX_RUN\" — the rule this gate keeps has no subject")
    for p in problems:
        print(p)
    if problems:
        print(f"a-run-is-judged-by-its-code: {len(problems)} problem(s) in {len(subjects)} script(s)")
        return 1
    print(
        f"a-run-is-judged-by-its-code: {len(subjects)} script(s) scanned; "
        f"{calls} run(s) go through smix-run, {raw} raw run(s) say why"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
