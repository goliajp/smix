#!/usr/bin/env python3
"""Check that the scripts that gate this repo are wired and runnable.

Checks:
  1. Every device guard has a harness. A guard decides what may touch a
     device; its own judgement needs testing as much as anything it judges.
  2. No script calls a GNU tool macOS does not ship. A missing tool reads
     as a product that fails every one of its tests.
  3. Every source gate runs in all three places — preflight, CI and ship.
  4. Every script under scripts/dev/ is run by something.
  5. Every script loads under the interpreter ship runs it with.

Exit non-zero on any failure.

What this file cannot see, said plainly so it is not read as omniscient:

  * Check 3 verifies a gate is NAMED in non-comment text, not that it
    runs. `[[ -n "$SKIP" ]] || python3 scripts/dev/x.py` counts. Proving
    execution means running ship, and running ship publishes.
  * Check 3's set is preflight's gate loop plus scripts/dev/*-scan.py.
    Things invoked outside that loop — gen-llms.py --check, the
    *-guard.test.sh round — are not in it. Guards are covered by check
    1; gen-llms --check is in all three places today and watched
    by nothing. That hole is recorded rather than papered over.
"""

import fnmatch
import glob
import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

# Where the device guards live.
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
# The same set, written as an array — which is how it is kept now that
# forty-four names no longer fit on a line anybody can read. Both forms
# are accepted: the loop is what runs, and the array is what the loop
# runs over.
SOURCE_GATE_ARRAY = re.compile(r"^SOURCE_GATES=\(\s*$(.*?)^\)\s*$", re.M | re.S)
PREFLIGHT = "scripts/dev/preflight.sh"
DOWNSTREAM = (".github/workflows/ci.yml", "scripts/release/ship.sh")

# Below this, assume the loop failed to parse rather than that the
# project runs fewer gates. A regex reading zero names would let every
# later comparison pass on an empty set.
MIN_SOURCE_GATES = 4

# Gates that cannot run on a CI host. Absent from CI on purpose, named here
# so the absence is a decision rather than something that fell off.
LOCAL_ONLY = {
    # Its input is the devicectl
    # installed with Xcode on the machine it runs on, and the CI job is
    # ubuntu, where it can only answer "cannot run". Its self-test carries
    # a fake devicectl and does run in CI.
    "a-refusal-devicectl-outgrew",
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
    text = read_without_comments(PREFLIGHT)
    array = SOURCE_GATE_ARRAY.search(text)
    loop = SOURCE_GATE_ARRAY if array else SOURCE_GATE_LOOP.search(text)
    if not array and not loop:
        failures.append(
            f"{PREFLIGHT}: could not find the `for gate in …; do` loop that "
            f"defines the source-gate set. Without it this check compares "
            f"against nothing."
        )
        return

    raw = array.group(1) if array else loop.group(1)
    names = [n for n in raw.split() if n and not n.startswith("$")]
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
                        f"{CI_GATE} invokes {name}, whose inputs do not exist on a "
                        f"CI host. It would report cannot-run on every branch build. "
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


def check_scripts_load_under_the_ship_interpreter(failures):
    """Every scan must import under the python a login shell finds.

    `preflight.sh` runs in the interactive shell and gets whatever
    `python3` is on that PATH — homebrew's 3.14 here. `ship.sh` is
    started with `bash -lc`, which finds Xcode's 3.9.6 first. Four scans
    used `X | None` in annotations, which 3.9 evaluates at definition
    time: they passed every local run for weeks and died in the ship,
    after an hour of gates, at the one place a failure costs the most.

    Loading rather than running: a scan that needs a device or a
    argument would fail for reasons that are not about the interpreter.
    An import is enough to catch the whole class — annotations, walrus,
    match, f-string nesting — because all of them are decided before the
    first statement runs.
    """
    login_python = subprocess.run(
        ["bash", "-lc", "command -v python3"],
        capture_output=True, text=True, check=False,
    ).stdout.strip()
    if not login_python:
        failures.append(
            "a login shell finds no python3, and `ship.sh` runs under one — "
            "every scan it invokes would die on the first line"
        )
        return
    mine = sys.executable
    if os.path.realpath(login_python) == os.path.realpath(mine):
        return  # One interpreter, nothing to diverge.

    probe = (
        "import importlib.util,sys\n"
        "spec=importlib.util.spec_from_file_location('m',sys.argv[1])\n"
        "m=importlib.util.module_from_spec(spec)\n"
        "try: spec.loader.exec_module(m)\n"
        "except SystemExit: pass\n"
    )
    checked = 0
    for pattern in ("scripts/dev/*.py", "scripts/release/*.py"):
        for path in sorted(glob.glob(os.path.join(ROOT, pattern))):
            if os.path.basename(path) == os.path.basename(__file__):
                continue
            checked += 1
            r = subprocess.run(
                [login_python, "-c", probe, path],
                capture_output=True, text=True, check=False,
            )
            first = (r.stderr or "").strip().splitlines()
            bad = [l for l in first if l.startswith(("SyntaxError", "TypeError"))]
            if bad:
                rel = os.path.relpath(path, ROOT).replace(os.sep, "/")
                failures.append(
                    f"{rel} does not load under {login_python} "
                    f"({bad[-1][:90]}). preflight runs under {mine} and ship "
                    f"runs under a login shell; a scan that only works in one "
                    f"of them passes every rehearsal and fails the performance."
                )
    if checked == 0:
        failures.append(
            "no scan was probed against the login shell's python — the layout "
            "changed and this check is reading air"
        )


def main():
    failures = []
    check_scripts_load_under_the_ship_interpreter(failures)
    check_guards_tested(failures)
    check_no_gnu_only_tools(failures)
    check_source_gates_wired(failures)
    check_every_dev_script_runs(failures)

    if failures:
        print("workflow-scan: FAIL", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    print("workflow-scan: clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
