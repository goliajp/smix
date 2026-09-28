#!/usr/bin/env python3
"""Every multi-line `python3 -c '...'` in the repo's shell scripts compiles.

A script that moved into a function carried its inline Python with it, and
the Python moved two columns to the right. Bash does not look inside the
quotes, so the script read clean to `bash -n`; the release found the
IndentationError half an hour in, when that line finally ran.

    python3 scripts/dev/inline-python-compiles.py            # the repo
    python3 scripts/dev/inline-python-compiles.py FILE...    # these files
"""

import pathlib
import re
import subprocess
import sys

START = re.compile(r"python3 -c '")


def read_word(text, i):
    """The shell word starting at `text[i]` (a `'`), as the Python it hands
    over: single-quoted runs verbatim, and a double-quoted run spliced in
    between them (`'...'"$VAR"'...'`) as one placeholder name, which is what
    the substituted value is in every snippet here."""
    out = []
    while i < len(text) and text[i] in "'\"":
        q = text[i]
        j = text.find(q, i + 1)
        if j < 0:
            return None
        out.append(text[i + 1 : j] if q == "'" else "SPLICED")
        i = j + 1
    return "".join(out)


def snippets(text):
    """(line, code) for every multi-line `python3 -c` body."""
    for m in START.finditer(text):
        code = read_word(text, m.end() - 1)
        if code is not None and "\n" in code:
            yield text.count("\n", 0, m.start()) + 1, code


def problems(path, text):
    out = []
    for line, code in snippets(text):
        try:
            compile(code, f"{path}:{line}", "exec")
        except SyntaxError as e:
            out.append(f"{path}:{line}: {e.msg} (line {e.lineno} of the snippet)")
    return out


def tracked_scripts(root):
    listed = subprocess.run(
        ["git", "ls-files", "*.sh"], cwd=root, capture_output=True, text=True, check=True
    ).stdout.split()
    return [root / p for p in listed]


def main(argv):
    root = pathlib.Path(__file__).resolve().parents[2]
    files = [pathlib.Path(a) for a in argv] or tracked_scripts(root)
    if not files:
        print("inline-python-compiles: FAIL — no shell scripts found, so nothing was checked")
        return 1
    seen = 0
    bad = []
    for f in files:
        text = f.read_text(encoding="utf-8", errors="replace")
        seen += sum(1 for _ in snippets(text))
        bad += problems(f.relative_to(root) if f.is_absolute() and root in f.parents else f, text)
    if bad:
        print("inline-python-compiles: FAIL")
        for b in bad:
            print(f"  - {b}")
        return 1
    print(f"inline-python-compiles: clean — {seen} multi-line snippet(s) in {len(files)} script(s) compile")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
