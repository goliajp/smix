#!/usr/bin/env python3
"""Check that the development contract survives a fresh checkout.

fact-scan asks whether user-facing surfaces tell the truth. This asks a
question one layer under it: do the files that govern how this repo is
worked on actually travel with it?

This check has been through both answers, and the reasons for each are
worth keeping because they are the same reasons in opposite directions.

It first ran because `.claude/` was ignored wholesale as "process
residue" while things that are not residue sat in it, and the cost came
due twice: sim-guard.sh's own header records that its v5.x implementation
"was never committed and was lost with the local checkout", and adb-guard
landed with a commit message stating it was "wired into the PreToolUse
hook beside sim-guard" — true when written, untrue by the next session,
because the wiring lived in the ignored file and the commit could not
carry it. A guard that is present but unwired is indistinguishable from
one that is working, right up until a physical phone gets wiped.

On 2026-07-29 the project decided the other way: `.claude/` is not
version-controlled at all. It is single-author, and the AI-side working
capability is deliberately unpublished. That accepts the loss above as a
standing risk — a hook wiring still lives only on this machine — in
exchange for keeping the development surface out of what ships. The
check therefore inverts rather than disappears: what used to be "the
contract must be tracked" is now "the private surface must not be", so
an accidental `git add -f` of the working record is caught the same way
its absence used to be.

Checks:
  1. The private surface is absent from the index everywhere, and
     present on disk wherever it lives. A tracked one means the
     development record leaked into what ships; a missing one, on the
     machine that has a `.claude/` at all, means this repo is being
     worked without the file that governs how.
  2. Every script a hook invokes exists. A missing hook script surfaces
     as a non-blocking error on every single Bash call — noisy enough to
     be tuned out, quiet enough to leave the guard off.
  3. Every device guard is wired. `plugin/scripts/*-guard.sh` exists to be
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

import fnmatch
import glob
import json
import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

# The private development surface: what this repo is worked with, and
# what must never reach the index. Anything added here must stay covered
# by the `.claude/` line in .gitignore, or check 1 fails by construction.
PRIVATE_SURFACE = [
    ".claude/CLAUDE.md",
    ".claude/settings.json",
    ".claude/rule/*.md",
    ".claude/rfcs/*.md",
    ".claude/docs/*.md",
]

SETTINGS = ".claude/settings.json"


def tracked_paths():
    """Paths git knows about, including staged-but-uncommitted adds.

    --cached rather than HEAD: a staged `git add -f` of the private
    surface is already leaving with the next commit, so it has to count
    as leaked before the commit is made, not after.
    """
    out = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "--cached"],
        capture_output=True,
        text=True,
        check=True,
    )
    return set(out.stdout.splitlines())


def check_private_surface(failures):
    """No part of `.claude/` is in the index; on the authoring machine, all
    of it is on disk.

    The two halves run in different places on purpose. The leak check is
    universal — a bare checkout that has `.claude/` files tracked is exactly
    the failure this exists to catch, and it can be answered from the index
    alone. The completeness check needs the surface to be there to be
    checked, so it runs only where it lives; asserting it on a checkout
    would fail every CI build for the thing that is supposed to be true.
    """
    # A stray `git add -f` on a plan leaks as well as one on the charter,
    # so this reads the whole subtree rather than the named patterns.
    for rel in sorted(p for p in tracked_paths() if p.startswith(".claude/")):
        failures.append(
            f"{rel}: tracked — the private development surface reached the "
            f"index and would ship. `git rm --cached` it; `.gitignore` "
            f"already covers `.claude/`."
        )

    if not os.path.isdir(os.path.join(ROOT, ".claude")):
        # A checkout, not the authoring machine. Nothing further to say.
        return

    for pattern in PRIVATE_SURFACE:
        if not glob.glob(os.path.join(ROOT, pattern)):
            failures.append(
                f"{pattern}: no file matches — this machine has a `.claude/` "
                f"but is missing part of the surface that governs how this "
                f"repo is worked"
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


# Where the device guards live.
#
# They were looked for under `scripts/dev/` until 2026-08-11, and there
# are none there — they have been in `plugin/scripts/` since the plugin
# was carved out. Both checks below iterated an empty list, so "every
# device guard is wired" and "every device guard has a harness" were
# saying "there are no device guards" in a repository with two. The
# scan reported clean, in preflight, in CI, and in ship.
GUARDS = "plugin/scripts/*-guard.sh"


def device_guards() -> list[str]:
    """Every guard, or a hard stop.

    An empty answer here is indistinguishable from a clean one for both
    callers, which is the whole reason this exists as a function: the
    glob that returned nothing for months looked exactly like a glob
    that found everything in order.
    """
    found = sorted(glob.glob(os.path.join(ROOT, GUARDS)))
    if not found:
        raise SystemExit(
            f"workflow-scan: FAIL\n"
            f"  - no device guard matches {GUARDS} — either they moved again, "
            f"in which case the checks below are inert, or there are none, in "
            f"which case nothing decides what may touch a device."
        )
    return found


def check_guards_wired(settings, failures):
    referenced = referenced_scripts(settings)
    for path in device_guards():
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
        body = f.read()
    if rel.endswith(".py"):
        # Python prose does not start with `#`. This checker's own
        # docstring names a script while running it never, and a first
        # pass read that as coverage — the checker committing the fault
        # it is here to catch, one layer up from ship.sh's two mentions
        # of hygiene-scan.
        # Built at runtime: writing the fence literally here would put
        # an unpaired triple quote in this file and mangle its own
        # stripping, which is how the first attempt kept reading its
        # own docstring as code.
        for fence in (chr(34) * 3, chr(39) * 3):
            body = re.sub(re.escape(fence) + r"(?:.|\n)*?" + re.escape(fence), "", body)
    lines = body.splitlines()
    return "\n".join(l for l in lines if not l.lstrip().startswith("#"))


SOURCE_GATE_LOOP = re.compile(r"^\s*for gate in (.+?);\s*do", re.M)
PREFLIGHT = "scripts/dev/preflight.sh"
DOWNSTREAM = (".github/workflows/ci.yml", "scripts/release/ship.sh")

# Below this, assume the loop failed to parse rather than that the
# project runs fewer gates. A regex reading zero names would let every
# later comparison pass on an empty set.
MIN_SOURCE_GATES = 4

# Gates whose inputs are the development record under `.claude/`, which is
# not version-controlled. They run where that record lives — the authoring
# machine, via preflight and ship — and cannot run on a bare checkout.
#
# Wiring them into CI would not be stricter, it would be worse: they would
# report "cannot run" on every branch build, and a gate that always says
# that is one nobody reads. The honest arrangement is that they are absent
# from CI on purpose, named here so the absence is a decision on the record
# rather than something that fell off.
LOCAL_ONLY = {
    "audit-ledger-scan",
    "scope-promise-scan",
    "release-record-scan",
    # Reconciles the guides against the probe ledgers in `.claude/docs/`.
    # Its own refusal states the rule: it runs where that record lives.
    # Learned live — its first CI run refused on a clean checkout,
    # exactly as designed, one job ahead of an include_str! of the same
    # record that stopped the test target building at all.
    "guide-claims-scan",
}
CI_GATE = ".github/workflows/ci.yml"


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
            local_only_in_ci = gate == CI_GATE and name in LOCAL_ONLY
            present = f"scripts/dev/{name}.py" in read_without_comments(gate)
            if local_only_in_ci:
                # Absent from CI is the intent; present would mean it runs
                # somewhere its inputs do not exist and reports "cannot run"
                # on every build.
                if present:
                    failures.append(
                        f"{CI_GATE} invokes {name}, whose inputs live under "
                        f"`.claude/` and do not travel with a checkout. It "
                        f"would report cannot-run on every branch build. "
                        f"Keep it to preflight and ship."
                    )
                continue
            if not present:
                failures.append(
                    f"{gate} does not invoke {name}, which {PREFLIGHT} runs. "
                    f"Three places, not two: preflight is the local habit, CI is "
                    f"the branch, ship is the release — and the one most often "
                    f"missing is the only one on the path to users."
                )


# The three gates, in the order the doctrine names them: preflight is
# the local habit, CI is the branch, ship is the release.
GATES = (PREFLIGHT, ".github/workflows/ci.yml", "scripts/release/ship.sh")

# Scripts that are gates themselves, so asking whether a gate runs them
# is circular.
GATE_ENTRY_POINTS = {"preflight.sh", "ship.sh"}


def runs_somewhere(rel, by_name, by_glob):
    """Is this script invoked, by name or by a glob that covers it?

    Loops like `for h in scripts/dev/*-guard.test.sh; do bash "$h";
    done` invoke by pattern, and a basename search reads that as
    nothing running — which is how a first pass reported two harnesses
    as orphans that preflight has been running all along.

    Globs count only in shell and workflow text. In Python a glob is
    how a checker *enumerates* files, and this checker's own
    `scripts/dev/*.sh` was duly read as running every script it was
    about to judge.
    """
    base = os.path.basename(rel)
    if any(base in text for text in by_name):
        return True
    for text in by_glob:
        for pattern in re.findall(r"[\w./-]*\*[\w./*-]*", text):
            if "/" in pattern and fnmatch.fnmatch(rel, pattern):
                return True
    return False


def check_every_dev_script_runs(failures):
    """Every script under scripts/dev/ is run by something.

    `fence-check.sh` guards an architectural invariant — the AI tier
    stays out of the sense path — and was run by one archived hot
    plan's checkpoint and nothing since. Committed, and then invoked by
    no gate, no hook, and no other script.

    That is the same species as the runner-sources tarball, whose
    regeneration script carried a header naming a ship gate that did
    not exist, and two Swift fixes consequently never reached the
    artefact consumers build. A file that runs nowhere is indistinguish-
    able from a file that was deleted, except that it reads as coverage.
    """
    # Keyed by path so a file is never searched for inside itself: its
    # own usage line and its own header both name it.
    texts = {}
    for g in GATES:
        texts[g] = read_without_comments(g)
    for path in sorted(
        glob.glob(os.path.join(ROOT, "scripts/**/*.sh"), recursive=True)
    ) + sorted(glob.glob(os.path.join(ROOT, "scripts/**/*.py"), recursive=True)):
        rel = os.path.relpath(path, ROOT).replace(os.sep, "/")
        texts.setdefault(rel, read_without_comments(rel))
    hooks = os.path.join(ROOT, ".claude/settings.json")
    if os.path.isfile(hooks):
        with open(hooks, encoding="utf-8") as f:
            texts[".claude/settings.json"] = f.read()

    for path in sorted(glob.glob(os.path.join(ROOT, "scripts/dev/*.sh"))) + sorted(
        glob.glob(os.path.join(ROOT, "scripts/dev/*.py"))
    ):
        rel = os.path.relpath(path, ROOT).replace(os.sep, "/")
        if os.path.basename(rel) in GATE_ENTRY_POINTS:
            continue
        by_name = [t for k, t in texts.items() if k != rel]
        by_glob = [
            t for k, t in texts.items() if k != rel and not k.endswith(".py")
        ]
        if not runs_somewhere(rel, by_name, by_glob):
            failures.append(
                f"{rel} is run by nothing — not preflight, not CI, not ship, "
                f"not a hook, not another script. Wire it into a gate or "
                f"delete it; a check that never runs reads as coverage while "
                f"providing none."
            )


def check_guards_tested(failures):
    for path in device_guards():
        # The guard ships with the plugin; its harness is a dev script
        # and stays here. Deriving the harness path from the guard's own
        # directory would look for it inside the published plugin, where
        # it does not belong.
        name = os.path.basename(path)[: -len(".sh")]
        harness = os.path.join(ROOT, "scripts", "dev", f"{name}.test.sh")
        if not os.path.isfile(harness):
            rel = os.path.relpath(path, ROOT).replace(os.sep, "/")
            want = os.path.relpath(harness, ROOT).replace(os.sep, "/")
            failures.append(
                f"{rel} has no harness — write {want}. A guard decides what "
                f"is allowed to touch a device; nothing checks that decision."
            )


def main():
    failures = []
    check_private_surface(failures)
    check_guards_tested(failures)
    check_no_gnu_only_tools(failures)
    check_source_gates_wired(failures)
    check_every_dev_script_runs(failures)

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
