"""Which lines of a shell script are code.

A gate that reads shell scripts has to tell a command from words that
name one: a string, a heredoc body, a comment. Shared by the gates that
read scripts/ so they agree on what a command is.
"""

from __future__ import annotations

import re


def code_lines(text: str) -> list[tuple[int, str]]:
    """Lines that start outside any string, heredoc or comment.

    The quoting state is carried across lines. It was counted a line at a
    time, which let a multi-line `python3 -c '…'` program — whose body has
    double quotes of its own — flip the state and hide every command after
    it: v2.12-c5's two `sim list` calls were invisible to this gate.
    """
    out: list[tuple[int, str]] = []
    heredoc_end: str | None = None
    quote: str | None = None
    for n, line in enumerate(text.splitlines(), 1):
        if heredoc_end is not None:
            if line.strip() == heredoc_end:
                heredoc_end = None
            continue
        starts_in_code = quote is None
        stripped = line.strip()
        if starts_in_code and (not stripped or stripped.startswith("#")):
            continue
        prev = ""
        for ch in line:
            if quote is None:
                if ch == "#" and (prev == "" or prev.isspace()):
                    break
                if ch in ("'", '"') and prev != "\\":
                    quote = ch
            elif ch == quote and (quote == "'" or prev != "\\"):
                quote = None
            prev = ch
        if starts_in_code:
            m = re.search(r"<<-?\s*['\"]?(\w+)['\"]?", line)
            if m and quote is None:
                heredoc_end = m.group(1)
            out.append((n, line))
    return out
