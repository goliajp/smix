#!/usr/bin/env python3
"""The project→device pointer holds a pointer, never a device fact.

A project's default device is recorded as an alias string keyed by the
project's path, in the machine store — a pointer into the registered
devices. What that alias resolves to (UDID, kind, destructive opt-in,
runner port, lease) stays in the registry, machine-scoped. §9 #9: a
project may keep the pointer; it may not keep the facts. This gate reads
the pointer's writer and refuses any device fact inside it, so the
per-project pointer can never quietly become a second place device facts
live.

By absence-needs-presence (.claude/rule/empty-predicate.md): the writer
must exist — a scan of a writer that has moved or been renamed certifies
nothing and so fails loudly — and its body must carry no fact token.

Env override (used by the .test.py harness against fixtures):
  SMIX_POINTER_FILE  — scan this file instead of the registry
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
TARGET = os.environ.get(
    "SMIX_POINTER_FILE",
    os.path.join(ROOT, "crates", "smix-simctl", "src", "registry.rs"),
)
WRITER = "fn set_project_alias"

# Tokens that name a device FACT. None may appear in the pointer's writer.
FACT_TOKENS = [
    (re.compile(r"[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-"), "a UDID literal"),
    (re.compile(r"\budid\b", re.I), "a udid field"),
    (re.compile(r"\bkind\b"), "a kind field"),
    (re.compile(r"destructive_opt_in"), "a destructive opt-in"),
    (re.compile(r"runner_port"), "a runner_port"),
    (re.compile(r"RegisteredSim"), "a RegisteredSim (the fact record)"),
    (re.compile(r"\bserial\b"), "a serial"),
    (re.compile(r"\bholder\b"), "a lease holder"),
    (re.compile(r"by_us|byUs"), "lease ownership"),
]


def strip_comments(text):
    """Blank out // line comments and /* */ blocks, keeping line count."""
    out = re.sub(r"/\*.*?\*/", lambda m: re.sub(r"[^\n]", " ", m.group(0)), text, flags=re.S)
    out = re.sub(r"//[^\n]*", "", out)
    return out


def writer_body(text):
    """The brace-matched body of the `fn set_project_alias` writer, or None."""
    idx = text.find(WRITER)
    if idx == -1:
        return None
    brace = text.find("{", idx)
    if brace == -1:
        return None
    depth = 0
    for i in range(brace, len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                return text[brace : i + 1]
    return None


def main():
    problems = []
    try:
        raw = open(TARGET, encoding="utf-8").read()
    except OSError as e:
        print(f"project-pointer-holds-no-facts: FAIL\n  - cannot read {TARGET}: {e}")
        return 1

    code = strip_comments(raw)
    body = writer_body(code)

    # presence: the writer must exist, or the scan is reading air.
    if body is None:
        problems.append(
            f"no `{WRITER}` in {os.path.relpath(TARGET, ROOT)} — the pointer's "
            f"writer moved or was renamed, so this scan is certifying air. "
            f"Point it at the writer."
        )
    else:
        # presence: it must actually write the pointer (the alias), not be a stub.
        if "project_devices()" not in body:
            problems.append(
                f"`{WRITER}` does not write through project_devices() — it is not "
                f"the pointer writer this gate thinks it is."
            )
        # absence: no device fact in the writer's body.
        for rx, desc in FACT_TOKENS:
            if rx.search(body):
                problems.append(
                    f"`{WRITER}` mentions {desc} — the project pointer must hold "
                    f"only the alias string; device facts stay in the registry "
                    f"(§9 #9). A pointer that carries a fact is a second place "
                    f"the fact lives."
                )

    if problems:
        print("project-pointer-holds-no-facts: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    print(
        "project-pointer-holds-no-facts: clean — the project pointer writer holds "
        "only the alias, no device facts"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
