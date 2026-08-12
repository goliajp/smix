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
heredoc bodies are stripped — every body except a shell's.

The line moved on 2026-08-12, and where it sits now follows from what
the guards match. Their patterns are shell syntax: the word `adb`, some
flags, a subcommand. A python heredoc that genuinely reached a device
would write `subprocess.run(["adb", "-s", serial, ...])`, and that
pattern does not match it. So judging a non-shell body with a
shell-shaped pattern cannot catch the dangerous form, and catches only
the harmless one — a string, a comment, a paragraph about a command.

It used to be the other way round: bodies were kept unless the consumer
was `cat` or `tee`. That read as the careful choice and was not one. It
blocked a document that described a device command, treating even the
placeholder `<serial>` as a real serial, and the same wall was hit three
times while writing the change that removed the reason for it. The
person who reported it moved eight copies of the guard aside instead.

`bash <<EOF` still has its body judged, because there the pattern means
exactly what it looks like. The shells are listed, so adding one is a
deliberate act; everything else has its body dropped.
"""

import json
import re
import sys

# Consumers that run their heredoc body as shell.
#
# The inversion matters, and the reason is what the guards downstream
# actually match on. Their patterns are shell syntax — a word `adb`,
# then flags, then a subcommand. A python heredoc that really did reach
# a device would say `subprocess.run(["adb", "-s", serial, ...])`, which
# that pattern never matches. So judging a non-shell body with a
# shell-shaped pattern cannot catch the dangerous form and can only
# catch the harmless one: a string, a comment, a paragraph.
#
# It did. Writing a document that described a device command made the
# command unwritable — the placeholder `<serial>` was read as a real
# serial — and the same wall was hit three times while implementing the
# change that removed the reason for it. The consumer who reported it
# moved eight copies of the guard aside instead.
#
# A shell body stays judged, because there the pattern means exactly
# what it looks like. Listed rather than derived: a consumer that is not
# named here has its body dropped, so anything added to shells must be
# added on purpose.
SHELL_CONSUMERS = {"bash", "sh", "zsh", "dash", "ksh", "shell"}

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

        inert = leading_word(line) not in SHELL_CONSUMERS
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
