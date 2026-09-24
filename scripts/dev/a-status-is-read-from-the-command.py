#!/usr/bin/env python3
"""An exit status must be read from the command being judged, not from the
filter at the end of its pipeline.

    out="$("$SMIX" run … 2>&1 | grep -v '^noise:')" || rc=$?

`rc` is grep's. `grep -v` exits 1 when it selected nothing, so a smix that
succeeded with nothing left to print reads as a failure; without
`pipefail`, a smix that failed reads as grep's 0. This repository had
forty-one scripts filtering the store's replay line this way, and the
session that removed the line at its source (smix-store `kevy_config`)
had read this shape wrong more than three times before it was written
down. So it is looked for everywhere, not fixed once.

Flagged:

* a `$( … )` whose pipeline's last stage is a filter, and whose status is
  read — `|| rc=$?`, `&& rc=0 || rc=$?`, or `rc=$?` / a `$?` test on the
  next line;
* a plain pipeline ending in a filter, with `$?` read on the next line;
* `$?` read on the line after `… || true` — always 0.

Not flagged: a pipeline whose last stage is the judge itself
(`printf … | verdict_fn`), a status taken before the output is filtered,
a filter whose status nobody reads, and anything in a comment.
"""

from __future__ import annotations

import argparse
import os
import re
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

FILTERS = {
    "grep", "egrep", "fgrep", "tail", "head", "sed", "awk", "tr", "cut",
    "sort", "uniq", "wc", "tee", "cat", "jq",
}
STATUS_READ_SAME_LINE = re.compile(r"\|\|\s*\w+=\$\?|&&\s*\w+=0\s*\|\|\s*\w+=\$\?")
STATUS_READ_LINE = re.compile(r"^\s*(\w+=\$\?|if\s+\[+\s*\"?\$\?|\[+\s*\"?\$\?|echo\s+[^#]*\$\?)")


def logical_lines(src: str) -> list[tuple[int, str]]:
    """Lines with backslash continuations joined, numbered by their first."""
    out: list[tuple[int, str]] = []
    buf, start = "", 0
    for n, line in enumerate(src.splitlines(), 1):
        if not buf:
            start = n
        if line.rstrip().endswith("\\"):
            buf += line.rstrip()[:-1] + " "
            continue
        out.append((start, buf + line))
        buf = ""
    if buf:
        out.append((start, buf))
    return out


def substitutions(line: str) -> list[str]:
    """Bodies of every `$( … )` on the line, quotes and nesting respected."""
    bodies: list[str] = []
    i = 0
    while True:
        i = line.find("$(", i)
        if i == -1:
            return bodies
        j, depth, dq = i + 2, 1, False
        while j < len(line) and depth:
            c = line[j]
            if c == "'" and not dq:
                k = line.find("'", j + 1)
                j = len(line) if k == -1 else k + 1
                continue
            if c == '"':
                dq = not dq
            elif c == "\\":
                j += 2
                continue
            elif line.startswith("$(", j):
                depth += 1
                j += 2
                continue
            elif c == "(" and not dq:
                depth += 1
            elif c == ")":
                depth -= 1
            j += 1
        bodies.append(line[i + 2 : j - 1])
        i = j


def stages(cmd: str) -> list[str]:
    """Top-level pipeline stages: split on `|` that is not `||`, outside
    quotes and parentheses."""
    parts, cur, depth, i, sq, dq = [], "", 0, 0, False, False
    while i < len(cmd):
        c = cmd[i]
        if c == "\\" and not sq:
            cur += cmd[i : i + 2]
            i += 2
            continue
        if c == "'" and not dq:
            sq = not sq
        elif c == '"' and not sq:
            dq = not dq
        elif not sq and not dq:
            if c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
            elif c == "|" and depth == 0:
                if cmd[i + 1 : i + 2] == "|" or cur.endswith("|"):
                    cur += c
                    i += 1
                    continue
                parts.append(cur)
                cur = ""
                i += 1
                continue
        cur += c
        i += 1
    parts.append(cur)
    return [p.strip() for p in parts]


def head_word(stage: str) -> str:
    words = stage.replace("(", " ").split()
    while words and re.match(r"^\w+=", words[0]):
        words = words[1:]
    return os.path.basename(words[0]) if words else ""


def ends_in_filter(cmd: str) -> str | None:
    """Stages cut at the first `;`, `&&` or `||` outside quotes — a status
    operator ends the pipeline."""
    s = stages(cmd)
    if len(s) < 2:
        return None
    last = re.split(r"\s*(?:&&|\|\||;)\s*", s[-1])[0]
    w = head_word(last)
    return w if w in FILTERS else None


def scan(path: str, rel: str) -> list[str]:
    with open(path, encoding="utf-8", errors="replace") as fh:
        lines = [(n, l) for n, l in logical_lines(fh.read()) if not l.lstrip().startswith("#")]
    found: list[str] = []
    for idx, (n, line) in enumerate(lines):
        nxt = lines[idx + 1][1] if idx + 1 < len(lines) else ""
        reads_next = bool(STATUS_READ_LINE.match(nxt))
        code = line.split(" #", 1)[0]
        for body in substitutions(code):
            f = ends_in_filter(body)
            if f and (STATUS_READ_SAME_LINE.search(code) or reads_next):
                found.append(
                    f"{rel}:{n}: the status read here is `{f}`'s, the last stage of the "
                    f"pipeline inside `$( … )`, not the command being judged"
                )
        if "$(" not in code:
            f = ends_in_filter(code)
            if f and reads_next:
                found.append(
                    f"{rel}:{n}: `$?` on the next line is `{f}`'s, not the command's"
                )
        if re.search(r"\|\|\s*true\s*$", code.rstrip()) and reads_next:
            found.append(f"{rel}:{n}: `$?` on the next line follows `|| true`, so it is always 0")
    return found


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=REPO)
    root = os.path.abspath(ap.parse_args().root)
    scripts = []
    for d, _, files in os.walk(os.path.join(root, "scripts")):
        scripts += [os.path.join(d, f) for f in files if f.endswith(".sh")]
    if not scripts:
        print("a-status-is-read-from-the-command: FAIL")
        print("  - no shell scripts under scripts/ — the scan read nothing, which is not clean")
        return 1
    problems: list[str] = []
    for p in sorted(scripts):
        problems += scan(p, os.path.relpath(p, root))
    if problems:
        print(f"a-status-is-read-from-the-command: FAIL — {len(problems)} status(es) read from the wrong program")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(
        f"a-status-is-read-from-the-command: clean — {len(scripts)} scripts, every status "
        f"read is the judged command's"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
