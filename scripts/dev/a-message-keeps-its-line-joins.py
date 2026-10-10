#!/usr/bin/env python3
"""A message wrapped over several source lines reads as one line.

Rust joins the lines of a string literal only where a line ends in a
backslash: the backslash eats the newline and the next line's leading
spaces. Without it the newline and the indentation stay in the string, and
the reader of an error gets a sentence with a hole of spaces in the middle
— three refusals in `runner up` and the CLI's platform dial read that way
until 13.2, and nothing that ran them looked at the text.

This reads every non-raw string literal in the workspace's product sources
— not the tests, whose literals are YAML and captured device output — and
reports one in which a sentence runs on across a line break and its
indentation. A raw string (`r"…"`, `r#"…"#`) is left alone: there the layout
is the point, and it is how a multi-line YAML fixture is written in product
code — in a plain string its lines read exactly like a broken sentence.

Usage:
    python3 scripts/dev/a-message-keeps-its-line-joins.py [--root DIR]
"""

from __future__ import annotations

import argparse
import os
import re
import sys

# A sentence broken mid-way: a word or a comma, the break, the indentation,
# and the sentence going on in lower case. Layout that is meant — a YAML
# fixture, a help page's indented list — breaks after a colon or before a
# marker, and is left alone.
HOLE = re.compile(r"[A-Za-z0-9,;)`'\]]\n {4,}[a-z(`]")
CHAR = re.compile(r"'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^\\'\n])'")


def literals(src: str):
    """(line, text) of every non-raw string literal in `src`."""
    i, n, line = 0, len(src), 1
    while i < n:
        c = src[i]
        if c == "\n":
            line += 1
            i += 1
        elif src.startswith("//", i):
            j = src.find("\n", i)
            i = n if j < 0 else j
        elif src.startswith("/*", i):
            depth, i = 1, i + 2
            while i < n and depth:
                if src.startswith("/*", i):
                    depth, i = depth + 1, i + 2
                elif src.startswith("*/", i):
                    depth, i = depth - 1, i + 2
                else:
                    line += src[i] == "\n"
                    i += 1
        elif c == "'":
            m = CHAR.match(src, i)
            i = m.end() if m else i + 1  # a lifetime otherwise
        elif c == "r" and re.match(r'r#*"', src[i:i + 300]) and not (i and (src[i - 1].isalnum() or src[i - 1] == "_")):
            hashes = re.match(r"r(#*)\"", src[i:]).group(1)
            close = '"' + hashes
            j = src.find(close, i + 2 + len(hashes))
            j = n if j < 0 else j + len(close)
            line += src.count("\n", i, j)
            i = j
        elif c == '"':
            start_line, j, buf = line, i + 1, []
            while j < n and src[j] != '"':
                if src[j] == "\\" and j + 1 < n:
                    if src[j + 1] == "\n":
                        # a line join: the newline and the indentation after it go
                        line += 1
                        j += 2
                        while j < n and src[j] in " \t":
                            j += 1
                        continue
                    buf.append(src[j:j + 2])
                    j += 2
                    continue
                if src[j] == "\n":
                    line += 1
                buf.append(src[j])
                j += 1
            yield start_line, "".join(buf)
            i = j + 1
        else:
            i += 1


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
    args = ap.parse_args()
    crates = os.path.join(args.root, "crates")
    files = [
        os.path.join(d, f)
        for d, _, fs in os.walk(crates)
        if "/src" in d and "/target" not in d and "/fuzz/" not in d + "/"
        for f in fs
        if f.endswith(".rs") and f != "tests.rs"
    ]
    if len(files) < 100:
        print(f"line-joins: CANNOT JUDGE — only {len(files)} Rust files under {crates}; a scan of nothing finds no hole")
        return 2
    holes = []
    scanned = 0
    for path in sorted(files):
        with open(path, encoding="utf-8") as fh:
            src = fh.read()
        # A trailing `#[cfg(test)] mod tests` is a test, whatever file it is in.
        cut = src.find("#[cfg(test)]\nmod ")
        if cut >= 0:
            src = src[:cut]
        for line, text in literals(src):
            scanned += 1
            if HOLE.search(text):
                snippet = re.sub(r"\n {4,}", " ⏎… ", text)[:90]
                holes.append(f"{os.path.relpath(path, args.root)}:{line}: {snippet!r}")
    if scanned < 1000:
        print(f"line-joins: CANNOT JUDGE — only {scanned} string literals read; the reader stopped finding them")
        return 2
    if holes:
        print(f"line-joins: FAIL — {len(holes)} string literal(s) keep a line break and its indentation; end the line with `\\`:")
        for h in holes:
            print(f"  - {h}")
        return 1
    print(f"line-joins: clean — {scanned} string literals in {len(files)} files, none keeps a line break with its indentation")
    return 0


if __name__ == "__main__":
    sys.exit(main())
