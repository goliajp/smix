#!/usr/bin/env python3
"""The gate finds the letter that never left, and only it.

Four fakes in a fake tree: one delivered to a thread that is there, one
delivered to a thread that is there WITHOUT it, one that says it was
deliberately not sent and why, and one that says nothing at all. A gate
that flagged all four, or none, would be useless in opposite directions.

The third is the one worth being careful about: "deliberately not sent"
is an escape hatch, and this repo has watched two of those go stale. So
the reason has to have something in it, and a fifth fake checks that
`no — n/a` does not pass.
"""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "a-reply-nobody-sent.py")


def build(root):
    dog = os.path.join(root, ".claude", "dogfood")
    thread = os.path.join(root, "consumer", ".claude", "state", "thread-1")
    os.makedirs(dog)
    os.makedirs(thread)

    sent_to = os.path.join(thread, "smix-reply-2026-01-01.md")
    open(sent_to, "w", encoding="utf-8").write("# smix → them\n")

    letters = {
        "reply-delivered.md": f"# smix → them\n\n<!-- delivered: {sent_to} -->\n",
        "reply-not-in-the-thread.md": (
            "# smix → them\n\n"
            f"<!-- delivered: {os.path.join(thread, 'smix-reply-never-written.md')} -->\n"
        ),
        "reply-declined.md": (
            "# smix → them\n\n"
            "<!-- delivered: no — superseded by the later letter, which answers "
            "every item in this one -->\n"
        ),
        "reply-silent.md": "# smix → them\n\nno line at all\n",
        "reply-hollow-reason.md": "# smix → them\n\n<!-- delivered: no — n/a -->\n",
    }
    for name, body in letters.items():
        open(os.path.join(dog, name), "w", encoding="utf-8").write(body)


def main():
    with tempfile.TemporaryDirectory() as root:
        build(root)
        done = subprocess.run(
            [sys.executable, GATE, root], capture_output=True, text=True, timeout=120
        )
    out = done.stdout + done.stderr
    problems = []

    if done.returncode == 0:
        problems.append("answered 0 with an undelivered letter in the tree")
    for name, why in (
        ("reply-not-in-the-thread.md", "the thread is there and the letter is not"),
        ("reply-silent.md", "it says nothing about where it went"),
        ("reply-hollow-reason.md", "'no' with nothing after it"),
    ):
        if name not in out:
            problems.append(f"{name} was not named — {why}")
    for name, why in (
        ("reply-delivered.md", "it is in the thread"),
        ("reply-declined.md", "it says why it was not sent"),
    ):
        if name in out:
            problems.append(f"{name} was named as a problem — {why}")
    if "1 delivered and checked" not in out:
        problems.append("the delivered one was not counted")
    if "1 deliberately not sent" not in out:
        problems.append("the declined one was not counted")

    if problems:
        print("reply-sent.test: RED")
        for p in problems:
            print(f"  {p}")
        print("\n--- gate output ---")
        print(out)
        return 1
    print("reply-sent.test: clean — names the letters nobody sent, and only those")
    return 0


if __name__ == "__main__":
    sys.exit(main())
