#!/usr/bin/env python3
"""Read a PreToolUse hook payload on stdin, print the part that will run.

The device guards judge a command by matching patterns against it. They
were matching the whole string, and a Bash call carries more than the
command: a heredoc body is data being written somewhere, not a command
being executed. Appending a paragraph to the decision log that mentioned
an install command was refused by adb-guard — the guard read the prose
it was being asked to file.

A guard that refuses correct work teaches people to take the guard off,
which costs more than the false negative it was protecting against. So
heredoc bodies are stripped — but only when the command consuming them
demonstrably does not execute them.

That last clause is the whole design. `bash <<EOF` runs its body, and a
guard that dropped it could be bypassed by anyone typing a heredoc. The
consumers whose bodies are inert are listed explicitly (cat, tee — the
write-to-a-file shapes); every other consumer keeps its body, including
`python3 -`, which can perfectly well shell out. Allowlisting what is
safe rather than denylisting what is not is the stance the guards
themselves take with emulator serials.
"""

import json
import re
import sys

# Commands whose heredoc body is written or printed, never executed.
INERT_CONSUMERS = {"cat", "tee"}

OPENER = re.compile(r"<<(-?)\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\2")


def leading_word(line):
    """First command word on the line, ignoring env assignments."""
    for token in line.strip().split():
        if "=" in token.split("/")[0] and not token.startswith("-"):
            continue  # VAR=value prefix
        return token.rsplit("/", 1)[-1]
    return ""


def strip_inert_heredocs(command):
    lines = command.split("\n")
    out = []
    i = 0
    while i < len(lines):
        line = lines[i]
        out.append(line)
        i += 1

        openers = OPENER.findall(line)
        if not openers:
            continue

        inert = leading_word(line) in INERT_CONSUMERS
        pending = [(delim, dash == "-") for dash, _q, delim in openers]

        while i < len(lines) and pending:
            body = lines[i]
            delim, strip_tabs = pending[0]
            probe = body.lstrip("\t") if strip_tabs else body
            if probe.strip() == delim:
                pending.pop(0)
                out.append(body)
            elif not inert:
                out.append(body)
            i += 1

    return "\n".join(out)


def main():
    try:
        payload = json.load(sys.stdin)
    except Exception:
        print("")
        return 0

    command = payload.get("tool_input", {}).get("command", "")
    print(strip_inert_heredocs(command))
    return 0


if __name__ == "__main__":
    sys.exit(main())
