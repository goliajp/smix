#!/usr/bin/env python3
"""Check that the development contract survives a fresh checkout.

fact-scan asks whether user-facing surfaces tell the truth. This asks a
question one layer under it: do the files that govern how this repo is
worked on actually travel with it?

They did not. `.claude/` was ignored wholesale as "process residue", and
three things that are not residue were sitting in it:

  - `.claude/CLAUDE.md`, which opens by saying every AI touching this
    project must read it first and that it outranks every other doc;
  - `.claude/rule/`, the project's own lint-grade rule cards;
  - `.claude/settings.json`, which is where the sim-guard hook is wired.

The cost came due twice. sim-guard.sh's own header records that its v5.x
implementation "was never committed and was lost with the local
checkout". Then adb-guard landed with a commit message stating it was
"wired into the PreToolUse hook beside sim-guard" — true when written,
untrue by the next session, because the wiring lived in the ignored
file and the commit could not carry it. The script was committed; the
line that makes it run was not. A guard that is present but unwired is
indistinguishable from one that is working, right up until a physical
phone gets wiped.

Checks:
  1. Contract files are tracked. Being on disk is not the same as being
     in the repo, and the difference is invisible until someone clones.
  2. Every script a hook invokes exists. A missing hook script surfaces
     as a non-blocking error on every single Bash call — noisy enough to
     be tuned out, quiet enough to leave the guard off.
  3. Every device guard is wired. `scripts/dev/*-guard.sh` exists to be
     run by a hook; one that no hook names is inert. This is the check
     that would have caught adb-guard.
  4. Every device guard has a harness. sim-guard went from v5.x to here
     with none, which is why nobody noticed it read heredoc bodies as
     commands and refused an append to the decision log for mentioning
     `simctl shutdown all`. A guard's own judgement needs testing as
     much as anything it judges.
  5. No script calls a GNU tool macOS does not ship. corpus-gate.sh
     called `timeout`, which is coreutils and absent on a stock Mac, so
     the shell answered "command not found" for every yaml in the
     corpus. Every one was recorded FAIL, the gate was permanently RED,
     and ship.sh could not finish on the machine smix is developed on —
     a missing tool reading as a product that fails all of its tests.
  6. Every source gate runs in all three places. hygiene-scan's own
     docstring says it exits non-zero "so it can gate a release", and
     ship.sh mentioned it only in two comments. workflow-scan was absent
     from ship as well: preflight and CI each ran six source gates while
     ship ran four, and the missing one is the path to users. Same
     sentence as check 3, with hooks swapped for release paths.

Exit non-zero on any failure.

What this file cannot see, said plainly so it is not read as omniscient:

  * Check 6 verifies a gate is NAMED in non-comment text, not that it
    runs. `[[ -n "$SKIP" ]] || python3 scripts/dev/x.py` counts. Proving
    execution means running ship, and running ship publishes.
  * Check 6's set is preflight's gate loop plus scripts/dev/*-scan.py.
    Things invoked outside that loop — gen-llms.py --check, the
    *-guard.test.sh round — are not in it. Guards are covered by checks
    3 and 4; gen-llms --check is in all three places today and watched
    by nothing. That hole is recorded rather than papered over.
"""

import glob
import json
import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

# Files that are contract rather than residue. Anything added here must
# also be re-included in .gitignore, or check 1 fails by construction.
CONTRACT = [
    ".claude/CLAUDE.md",
    ".claude/settings.json",
    ".claude/rule/*.md",
    ".claude/rfcs/*.md",
]

SETTINGS = ".claude/settings.json"


def tracked_paths():
    """Paths git knows about, including staged-but-uncommitted adds.

    --cached rather than HEAD: a rule card written and staged in this
    session is already leaving with the next commit, and failing it
    would make the gate impossible to satisfy before committing.
    """
    out = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "--cached"],
        capture_output=True,
        text=True,
        check=True,
    )
    return set(out.stdout.splitlines())


def check_contract_tracked(failures):
    tracked = tracked_paths()
    for pattern in CONTRACT:
        matches = sorted(
            os.path.relpath(p, ROOT).replace(os.sep, "/")
            for p in glob.glob(os.path.join(ROOT, pattern))
        )
        if not matches:
            failures.append(
                f"{pattern}: no file matches — the contract names a file "
                f"that is not there"
            )
            continue
        for rel in matches:
            if rel not in tracked:
                failures.append(
                    f"{rel}: on disk but not tracked — it will not survive a "
                    f"clone. Re-include it in .gitignore and `git add` it."
                )


def hook_commands(settings):
    """Every command string across every hook event."""
    for event_hooks in settings.get("hooks", {}).values():
        for matcher in event_hooks:
            for hook in matcher.get("hooks", []):
                command = hook.get("command")
                if command:
                    yield command


def referenced_scripts(settings):
    """Repo-relative script paths a hook invokes.

    Hook commands address scripts through $CLAUDE_PROJECT_DIR, which is
    the repo root at run time.
    """
    pattern = re.compile(r'\$(?:\{)?CLAUDE_PROJECT_DIR(?:\})?/([^"\'\s]+)')
    found = set()
    for command in hook_commands(settings):
        found.update(pattern.findall(command))
    return found


def check_hook_scripts_exist(settings, failures):
    for rel in sorted(referenced_scripts(settings)):
        if not os.path.isfile(os.path.join(ROOT, rel)):
            failures.append(
                f"{SETTINGS} invokes {rel}, which does not exist — every Bash "
                f"call will report a hook error and the guard will not run"
            )


def check_guards_wired(settings, failures):
    referenced = referenced_scripts(settings)
    for path in sorted(glob.glob(os.path.join(ROOT, "scripts/dev/*-guard.sh"))):
        rel = os.path.relpath(path, ROOT).replace(os.sep, "/")
        if rel not in referenced:
            failures.append(
                f"{rel} is a device guard that no hook invokes — it is inert. "
                f"Wire it in {SETTINGS} under PreToolUse."
            )


# Commands that exist on GNU/Linux and not on a stock macOS, mapped to
# what to reach for instead. Only tools that have actually bitten are
# listed — a speculative list would be noise, and the point is that each
# entry names a real outage.
GNU_ONLY = {
    "timeout": "scripts/dev/run-with-timeout.py (same exit codes)",
}

# `word` at the start of a command position: line start, after a pipe,
# after && / || / ; , or after `if` / `then` / `else` / `do`.
def gnu_tool_pattern(tool):
    return re.compile(
        r"(?:^|\||&&|\|\||;|\bif\s|\bthen\s|\belse\s|\bdo\s)\s*" + re.escape(tool) + r"\s",
        re.M,
    )


def check_no_gnu_only_tools(failures):
    for path in sorted(glob.glob(os.path.join(ROOT, "scripts/**/*.sh"), recursive=True)):
        rel = os.path.relpath(path, ROOT).replace(os.sep, "/")
        with open(path, encoding="utf-8") as f:
            body = "\n".join(
                line for line in f.read().splitlines() if not line.lstrip().startswith("#")
            )
        for tool, alternative in GNU_ONLY.items():
            if gnu_tool_pattern(tool).search(body):
                failures.append(
                    f"{rel} invokes `{tool}`, which macOS does not ship — on a "
                    f"stock Mac that is 'command not found' for every call, not "
                    f"a timeout. Use {alternative}."
                )


def read_without_comments(rel):
    """File contents with whole-line comments removed.

    Shell and YAML both comment with `#`. Naming a command in a comment
    is not invoking it, and the difference is the entire point of check
    6 — ship.sh mentions hygiene-scan twice in prose while calling it
    never.
    """
    with open(os.path.join(ROOT, rel), encoding="utf-8") as f:
        lines = f.read().splitlines()
    return "\n".join(l for l in lines if not l.lstrip().startswith("#"))


SOURCE_GATE_LOOP = re.compile(r"^\s*for gate in (.+?);\s*do", re.M)
PREFLIGHT = "scripts/dev/preflight.sh"
DOWNSTREAM = (".github/workflows/ci.yml", "scripts/release/ship.sh")

# Below this, assume the loop failed to parse rather than that the
# project runs fewer gates. A regex reading zero names would let every
# later comparison pass on an empty set.
MIN_SOURCE_GATES = 4


def check_source_gates_wired(failures):
    """Every source gate named in preflight also runs in CI and at ship.

    Matched against comment-stripped text, and that is the load-bearing
    part rather than tidiness: ship.sh names hygiene-scan in two
    comments while never calling it. A plain substring search would find
    those and pass — the checker would be committing the same fault it
    is here to catch. `read()` drops whole-line comments; do not
    "simplify" this back to matching raw text.
    """
    loop = SOURCE_GATE_LOOP.search(read_without_comments(PREFLIGHT))
    if not loop:
        failures.append(
            f"{PREFLIGHT}: could not find the `for gate in …; do` loop that "
            f"defines the source-gate set. Without it this check compares "
            f"against nothing."
        )
        return

    names = [n for n in loop.group(1).split() if n and not n.startswith("$")]
    if len(names) < MIN_SOURCE_GATES:
        failures.append(
            f"{PREFLIGHT}: parsed {len(names)} source gates from the loop, "
            f"expected at least {MIN_SOURCE_GATES}. Treating this as a broken "
            f"parse rather than a shorter list — a set read as empty makes "
            f"every comparison below vacuous."
        )
        return

    # Disk → preflight: a scan nobody lists cannot be checked downstream.
    for path in sorted(glob.glob(os.path.join(ROOT, "scripts/dev/*-scan.py"))):
        stem = os.path.basename(path)[: -len(".py")]
        if stem not in names:
            failures.append(
                f"{stem} exists but {PREFLIGHT}'s gate loop does not list it, so "
                f"nothing checks where else it runs. Add it to the loop."
            )

    # preflight → CI and ship, in text that is not a comment.
    for name in names:
        for gate in DOWNSTREAM:
            if f"scripts/dev/{name}.py" not in read_without_comments(gate):
                failures.append(
                    f"{gate} does not invoke {name}, which {PREFLIGHT} runs. "
                    f"Three places, not two: preflight is the local habit, CI is "
                    f"the branch, ship is the release — and the one most often "
                    f"missing is the only one on the path to users."
                )


def check_guards_tested(failures):
    for path in sorted(glob.glob(os.path.join(ROOT, "scripts/dev/*-guard.sh"))):
        harness = path[: -len(".sh")] + ".test.sh"
        if not os.path.isfile(harness):
            rel = os.path.relpath(path, ROOT).replace(os.sep, "/")
            want = os.path.relpath(harness, ROOT).replace(os.sep, "/")
            failures.append(
                f"{rel} has no harness — write {want}. A guard decides what "
                f"is allowed to touch a device; nothing checks that decision."
            )


def main():
    failures = []
    check_contract_tracked(failures)
    check_guards_tested(failures)
    check_no_gnu_only_tools(failures)
    check_source_gates_wired(failures)

    settings_path = os.path.join(ROOT, SETTINGS)
    if os.path.isfile(settings_path):
        with open(settings_path, encoding="utf-8") as f:
            settings = json.load(f)
        check_hook_scripts_exist(settings, failures)
        check_guards_wired(settings, failures)

    if failures:
        print("workflow-scan: FAIL", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    print("workflow-scan: clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
