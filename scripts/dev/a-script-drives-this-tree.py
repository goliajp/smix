#!/usr/bin/env python3
"""A script that drives smix drives the smix this tree builds.

    SMIX_BIN="${SMIX_BIN:-$(command -v smix)}"

That line picks whatever is first on the PATH, and on the development
machine that is the last release installed — 10.1.0 while this tree was
11.0. So a release gate run by hand judged a binary the tree had long
since moved past, and passed or failed for reasons that had nothing to
do with the change being checked. It is M2 again (two halves of one
check reading two binaries) in the place M2's own gate did not look:
M2's gate asks whether a script NAMES a build path, and a PATH lookup
names none.

It was not only by hand. `ship.sh` gave every gate an explicit binary
except the smoke it runs first, which therefore smoked the installed
release rather than the one about to be published; and `corpus-gate.sh`
left `SMIX_BIN` unexported, so the flake classifier it calls read the
installed release's records.

The one place a binary is chosen is `scripts/lib/e2e-binary.sh`: this
tree's debug build unless `SMIX_BIN` names another. Flagged, everywhere
under `scripts/`:

* a PATH lookup used to find smix — `command -v smix`, `which smix`,
  `shutil.which("smix")`;
* `smix <verb>` run as a command in a shell script that does not define
  a `smix()` function driving `$SMIX` / `$SMIX_BIN` — a bare name is a
  PATH lookup too, only quieter.

Not flagged: comments, heredoc bodies (they are text for another
program), `smix-mcp` and other names that merely start with `smix`, and
`smix` inside a quoted string.

Usage:
  scripts/dev/a-script-drives-this-tree.py [--root DIR]
"""

from __future__ import annotations

import argparse
import os
import re
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# How few scripts mean the walk lost its subject. 118 shell and 136
# Python files exist today; a walk that finds a handful is looking at
# the wrong directory, and that reads exactly like a clean tree.
MIN_SCRIPTS = 100

NAME = r"smix(?![\w-])"
PATH_LOOKUP = [
    re.compile(rf"\bcommand\s+-v\s+{NAME}"),
    re.compile(rf"\bwhich\s+{NAME}"),
    re.compile(r"""\bshutil\.which\(\s*['"]smix['"]\s*\)"""),
]
# `smix` in command position: start of line, or after an operator,
# an opening paren, `!`, or a keyword that starts a command.
BARE_CALL = re.compile(rf"(?:^|[;&|(!]|\bthen\b|\bdo\b|\belse\b)\s*{NAME}\s+[a-z]")
# A build path of this tree taken as a script's default — the choice is
# the resolver's. `${X:-…/target/release/smix}` anywhere, or a plain
# assignment to the variable a script then drives. Naming a binary for a
# child (`SMIX_BIN=… cmd`, `export SMIX_BIN=…`) is not a default: it is
# the explicit naming the resolver honours.
IN_TREE = r"target/(?:debug|release)/smix(?:-mcp)?"
SH_DEFAULT = re.compile(rf":-[^}}\s]*{IN_TREE}")
SH_ASSIGN = re.compile(rf"^\s*(?:SMIX|SMIX_MCP|MCP|BIN)=\S*{IN_TREE}\S*\s*$")
PY_PATH = re.compile(rf"""(['"])[^'"\s]*{IN_TREE}\1""")
# The resolver itself is the one place a build path is the default.
RESOLVER = os.path.join("scripts", "lib", "e2e-binary.sh")
WRAPPER = re.compile(r"^\s*smix\s*\(\)\s*\{")
# `<<EOF`, `<<-'EOF'`, `<< "EOF"` — not `<<<`, which is a string, not a body.
HEREDOC = re.compile(r"<<(?!<)-?\s*['\"]?(\w+)['\"]?")


def shell_code(src: str, keep_quotes: bool = False) -> str:
    """The shell source with everything that is not code blanked out.

    Comments, quoted text and heredoc bodies become spaces, newlines kept
    so line numbers still hold. A `$( … )` inside double quotes stays:
    it is code, and `"$(smix runner down)"` runs a bare smix as surely
    as the unquoted form. A line scanner cannot see that, nor a string
    that spans lines, which is why this reads the file as a whole.
    """
    out = list(src)
    i, n = 0, len(src)
    stack = ["code"]  # "code" | "dq"; a $( inside "dq" pushes "code"
    depth = [0]
    pending: list[str] = []

    def blank(a: int, b: int, quoted: bool = False) -> None:
        if quoted and keep_quotes:
            return
        for k in range(a, b):
            if out[k] != "\n":
                out[k] = " "

    while i < n:
        c = src[i]
        state = stack[-1]
        if state == "dq":
            if c == "\\":
                blank(i, min(i + 2, n), quoted=True)
                i += 2
                continue
            if c == '"':
                stack.pop()
                depth.pop()
                i += 1
                continue
            if src.startswith("$(", i):
                stack.append("code")
                depth.append(0)
                i += 2
                continue
            blank(i, i + 1, quoted=True)
            i += 1
            continue
        # code
        if c == "\n" and pending:
            end = pending.pop(0)
            j = i + 1
            while j < n:
                k = src.find("\n", j)
                k = n if k < 0 else k
                if src[j:k].strip() == end:
                    blank(i + 1, k)
                    i = k
                    break
                j = k + 1
            else:
                blank(i + 1, n)
                i = n
            continue
        if c == "#" and (i == 0 or src[i - 1] in " \t\n;"):
            k = src.find("\n", i)
            k = n if k < 0 else k
            blank(i, k)
            i = k
            continue
        if c == "'":
            k = src.find("'", i + 1)
            k = n - 1 if k < 0 else k
            blank(i, k + 1, quoted=True)
            i = k + 1
            continue
        if c == '"':
            stack.append("dq")
            depth.append(0)
            i += 1
            continue
        m = HEREDOC.match(src, i)
        if m:
            pending.append(m.group(1))
            i = m.end()
            continue
        if src.startswith("<<<", i):
            i += 3
            continue
        if len(stack) > 1:
            if c == "(":
                depth[-1] += 1
            elif c == ")":
                if depth[-1] == 0:
                    stack.pop()
                    depth.pop()
                else:
                    depth[-1] -= 1
        i += 1
    return "".join(out)


def python_code(src: str) -> str:
    """Python source with comment lines blanked (strings are the point here)."""
    return "\n".join("" if ln.lstrip().startswith("#") else ln for ln in src.split("\n"))


def wrapper_drives_the_tree(src: str) -> bool:
    """Whether the file defines `smix()` and that function runs `$SMIX`."""
    lines = src.splitlines()
    for i, line in enumerate(lines):
        if not WRAPPER.match(line):
            continue
        depth, body = 0, []
        for ln in lines[i:]:
            depth += ln.count("{") - ln.count("}")
            body.append(ln)
            if depth <= 0:
                break
        return any(re.search(r"\$\{?SMIX(?:_BIN)?\b", b) for b in body)
    return False


def scan(path: str, rel: str) -> list[str]:
    with open(path, encoding="utf-8", errors="replace") as fh:
        src = fh.read()
    shell = path.endswith(".sh")
    code = shell_code(src) if shell else python_code(src)
    shown = src.split("\n")
    problems: list[str] = []
    wrapped = shell and wrapper_drives_the_tree(src)
    for n, line in enumerate(code.split("\n"), 1):
        if any(p.search(line) for p in PATH_LOOKUP):
            problems.append(
                f"{rel}:{n}: finds smix on the PATH (`{shown[n - 1].strip()}`) — that is "
                f"the installed release, not this tree. Source scripts/lib/e2e-binary.sh "
                f"and use `$SMIX`; SMIX_BIN still overrides it."
            )
            continue
        if shell and not wrapped and BARE_CALL.search(line):
            problems.append(
                f"{rel}:{n}: runs a bare `smix` (`{shown[n - 1].strip()}`) — the name "
                f"resolves through the PATH. Use `\"$SMIX\"` from scripts/lib/e2e-binary.sh."
            )
    if rel == RESOLVER:
        return problems
    quoted = shell_code(src, keep_quotes=True) if shell else code
    for n, line in enumerate(quoted.split("\n"), 1):
        hit = (SH_DEFAULT.search(line) or SH_ASSIGN.search(line)) if shell else PY_PATH.search(line)
        if hit:
            problems.append(
                f"{rel}:{n}: picks its own binary (`{shown[n - 1].strip()}`) — two halves "
                f"of one check then read two binaries (M2). Source scripts/lib/e2e-binary.sh "
                f"(Python: scripts/dev/_e2e_binary.py) and use what it names."
            )
    return problems


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=REPO)
    ap.add_argument("--min-scripts", type=int, default=MIN_SCRIPTS)
    args = ap.parse_args()
    root = os.path.abspath(args.root)
    # This gate and its test spell every shape they look for; they are
    # the two files whose job is to say these words.
    here = os.path.abspath(__file__)
    itself = {here, here[: -len(".py")] + ".test.py"}
    scripts = []
    for d, _, files in os.walk(os.path.join(root, "scripts")):
        scripts += [
            os.path.join(d, f)
            for f in files
            if f.endswith((".sh", ".py")) and os.path.abspath(os.path.join(d, f)) not in itself
        ]
    if len(scripts) < args.min_scripts:
        print("a-script-drives-this-tree: FAIL")
        print(
            f"  - {len(scripts)} scripts under scripts/, fewer than {args.min_scripts} — "
            f"a scan that lost its subject reads exactly like a clean one"
        )
        return 1
    problems: list[str] = []
    for p in sorted(scripts):
        problems += scan(p, os.path.relpath(p, root))
    if problems:
        print(f"a-script-drives-this-tree: FAIL — {len(problems)} place(s) that drive another smix")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(
        f"a-script-drives-this-tree: clean — {len(scripts)} scripts, every smix they "
        f"drive comes from scripts/lib/e2e-binary.sh or an explicit SMIX_BIN"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
