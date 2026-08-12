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


# Where one command ends.
#
# A guard matches a pattern against text and then reads a word out of it,
# and while the text was the whole Bash call those two could land on
# different commands. `adb -s emulator-5554 shell getprop` followed by
# `adb -s <a physical serial> install -r app.apk` was allowed through:
# the emulator pin on the first answered for the second. Five shapes of
# that were reproduced on 2026-08-12, including installing to a phone and
# `rm -rf` on one, and the mirror-image false refusal — a `curl -s <url>`
# beside a legitimate device command had its URL read as a device name.
#
# Splitting belongs here rather than in either guard because both read
# from here, and because this file already owns the other half of the
# same question: which text a guard may match at all.
# One character each. `&&` and `||` need no entry of their own: the
# second character starts a fragment that is empty, and empty fragments
# are dropped. Listing them as well changed nothing that could be
# observed, which is the definition of a line that should not be here.
SEPARATORS = (";", "|", "&", "\n")


def split_commands(command):
    """One command per element, honouring quotes and backslash escapes.

    Quoting is the half that has to be right. `shell input text "note;
    more"` is one command — splitting inside the quotes would invent a
    second command out of a string somebody is typing into a text field,
    and the guard would then judge text that never runs.
    """
    out = []
    current = []
    quote = None
    i = 0
    while i < len(command):
        ch = command[i]
        if quote:
            current.append(ch)
            if ch == "\\" and quote == '"' and i + 1 < len(command):
                current.append(command[i + 1])
                i += 2
                continue
            if ch == quote:
                quote = None
            i += 1
            continue
        if ch in "'\"":
            quote = ch
            current.append(ch)
            i += 1
            continue
        if ch == "\\" and i + 1 < len(command):
            current.append(ch)
            current.append(command[i + 1])
            i += 2
            continue
        hit = next((s for s in SEPARATORS if command.startswith(s, i)), None)
        if hit:
            out.append("".join(current))
            current = []
            i += len(hit)
            continue
        current.append(ch)
        i += 1
    out.append("".join(current))
    return [c.strip() for c in out if c.strip()]


def main():
    try:
        payload = json.load(sys.stdin)
    except Exception:
        print("")
        return 0

    command = payload.get("tool_input", {}).get("command", "")
    for one in split_commands(strip_inert_heredocs(command)):
        print(one)
    return 0


if __name__ == "__main__":
    sys.exit(main())
