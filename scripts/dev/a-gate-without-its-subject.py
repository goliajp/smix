#!/usr/bin/env python3
"""Take away what a gate reads, and watch whether it still says yes.

A gate that passes when its subject is missing has no subject. The rule
card says so (`gate/no-empty-predicate`), and until now nothing checked
it across the gates themselves — which is how `a-selftest-nobody-runs`
came to answer green about an empty set, and how an exemption naming a
deleted file went on satisfying a check for a month.

The first design for this asked every gate to declare the paths it reads
in a module-level `SOURCES`. Measurement killed it twice: of 93 scripts,
7 declared anything, so 49 declarations would have been written by hand
— a second copy of something already in the code, which is the root
cause this repo keeps tripping over — and `hygiene-scan` opens 1991
files in the repo on one run, so "remove each declared path in turn" is
not a shape its subject fits.

So the list is observed rather than declared. A Python audit hook records
what the gate actually opens; those files are moved aside; the gate runs
again. Observation cannot go stale and cannot lie about itself.

It also fixes the blunt probe that preceded it. Copying `scripts/` into
an otherwise empty tree left 14 of 56 gates passing, and every one was
legitimate: their subjects live *inside* `scripts/`, so the probe never
took them away. Here they are in the observed set like anything else.

**No exemption list.** The three outcomes are derived, not granted:

  * a gate that opens nothing in the repo has no subject to take — the
    self-tests that build their own fixtures land here, by measurement
    rather than by being named;
  * a gate that stops being green once its subject is gone is doing its
    job;
  * a gate that stays green is the finding.

A fourth line is honest rather than clever: seven of the gates are bash,
and this instrument only sees Python's `open`. They are listed as not
observable rather than counted as passing — the whole point of the
exercise is that silence about a subject is not evidence about it.

Usage:  a-gate-without-its-subject.py [repo-root]
"""

import os
import re
import shutil
import time
import subprocess
import sys
import tempfile

ROOT = os.path.abspath(
    sys.argv[1]
    if len(sys.argv) > 1
    else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
)

SHIP = os.path.join("scripts", "release", "ship.sh")

# Long enough for the slowest gate that only reads files; a gate that
# shells out to cargo will hit this and be reported as unjudgeable
# rather than silently counted either way.
PER_RUN_TIMEOUT = 180

# Paths that are not a subject: the interpreter reading itself, and the
# kernel's randomness. Without this every gate "reads files outside the
# tree" and nothing is ever judged.
NOT_A_SUBJECT = (
    "/lib/python",
    "/site-packages/",
    "Python.framework",
    "/encodings/",
    # Randomness, not evidence: `workflow-scan` reads it and taking it
    # away would prove nothing about what the gate judges.
    "/dev/",
)

AUDIT = '''
import sys, os
_out = os.environ.get("SMIX_AUDIT_OUT")
if _out:
    _fd = os.open(_out, os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o644)

    def _hook(event, args):
        # Only reads. A gate writing its own log file is not reading a
        # subject, and taking the log away would prove nothing.
        if event == "open" and len(args) >= 2 and isinstance(args[0], str):
            mode = args[1]
            if mode is None or (isinstance(mode, str) and "r" in mode and "+" not in mode):
                try:
                    os.write(_fd, (args[0] + "\\n").encode("utf-8", "replace"))
                except Exception:
                    pass

    sys.addaudithook(_hook)
'''


def gates_from_ship(root: str):
    """Every gate the ship runs, read off the ship rather than listed.

    A list of gates beside the ship is the second copy that goes stale;
    this is the same reasoning that keeps `FailureCode::ALL` in the enum
    and not in the test that walks it.
    """
    path = os.path.join(root, SHIP)
    try:
        text = open(path, encoding="utf-8").read()
    except OSError as e:
        print(f"gate-subject: CANNOT RUN — {SHIP} could not be read: {e}")
        return None
    found = re.findall(
        r'(python3|bash)\s+"\$ROOT/(scripts/(?:dev|release)/[^"]+)"', text
    )
    seen, out = set(), []
    for lang, rel in found:
        if rel not in seen:
            seen.add(rel)
            out.append((lang, rel))
    return out


# Development record, deliberately not version-controlled (the 2026-07-29
# decision), and the subject of fourteen of the gates. Listing files by
# what git knows leaves it out, and those gates then say "cannot run" —
# a true sentence that this sweep would file as caution when it is
# really the copy being wrong.
ALSO_COPY = (".claude",)


def pristine_copy(root: str, dest: str):
    """Everything a reader gets: tracked, plus untracked and not ignored."""
    listing = subprocess.run(
        ["git", "ls-files", "-c", "-o", "--exclude-standard"],
        cwd=root,
        capture_output=True,
        text=True,
        check=True,
    )
    for rel in listing.stdout.splitlines():
        src = os.path.join(root, rel)
        if not os.path.isfile(src):
            continue
        dst = os.path.join(dest, rel)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copy2(src, dst)
    # A copy without a repository is not a copy of this repository: a
    # third of the gates ask git what is tracked, and in a bare
    # directory they fail for that reason and get filed as unjudgeable —
    # a wrong answer that looks like a cautious one. The self-test built
    # its fixture this way from the start; the real sweep did not, and
    # the two instruments disagreeing is how an instrument lies.
    for extra in ALSO_COPY:
        src = os.path.join(root, extra)
        if os.path.isdir(src):
            shutil.copytree(src, os.path.join(dest, extra), dirs_exist_ok=True)
    subprocess.run(["git", "init", "-q"], cwd=dest, check=True)
    subprocess.run(["git", "add", "-A"], cwd=dest, check=True)
    # And a commit, because "staged" is not "has a history": the gates
    # that ask `git log` or `git describe` answer nothing in a tree whose
    # files were only added, and nothing is not the same as an answer.
    subprocess.run(
        ["git", "-c", "user.email=gate@localhost", "-c", "user.name=gate",
         "commit", "-q", "-m", "pristine copy"],
        cwd=dest,
        check=True,
    )


def run_gate(tree, lang, rel, audit_dir):
    """Run one gate in `tree`; return (exit code, opened repo files, first line).

    Unannotated on purpose: `str | None` is a syntax the ship's login
    shell python does not have, and `workflow-scan` caught this file
    failing to load there while it ran fine under the preflight's newer
    one. A scan that only works in one of them passes every rehearsal.
    """
    env = dict(os.environ)
    out_file = None
    if audit_dir:
        out_file = os.path.join(audit_dir, "opened.txt")
        open(out_file, "w").close()
        env["SMIX_AUDIT_OUT"] = out_file
        env["PYTHONPATH"] = audit_dir + os.pathsep + env.get("PYTHONPATH", "")
    started = time.time()
    # macOS resolves /var to /private/var, so a path inside the copy can
    # come back with a prefix the copy's own name does not match — three
    # gates were filed as reading "outside the tree" while reading the
    # tree. And a gate that builds a fixture with `copy2` carries the
    # originals' mtimes into it, so "newer than the run" cannot see that
    # the directory is the gate's own: what can is which temp entries
    # existed before it started.
    real_tree = os.path.realpath(tree)
    tmp_root = os.path.realpath(tempfile.gettempdir())
    try:
        tmp_before = set(os.listdir(tmp_root))
    except OSError:
        tmp_before = set()
    try:
        done = subprocess.run(
            [lang, os.path.join(tree, rel)],
            cwd=tree,
            capture_output=True,
            text=True,
            timeout=PER_RUN_TIMEOUT,
            env=env,
        )
        code = done.returncode
        said = next(
            (
                l.strip()
                for l in (done.stdout + done.stderr).splitlines()
                if l.strip()
            ),
            "",
        )
    except subprocess.TimeoutExpired:
        code, said = None, ""
    opened, elsewhere = set(), set()
    if out_file and os.path.exists(out_file):
        for line in open(out_file, encoding="utf-8", errors="replace"):
            p = line.strip()
            if p.startswith(tree + os.sep) or p.startswith(real_tree + os.sep):
                base = tree if p.startswith(tree + os.sep) else real_tree
                r = os.path.relpath(p, base)
                if r.startswith(".git" + os.sep) or r == rel:
                    continue
                if os.path.isfile(os.path.join(tree, r)):
                    opened.add(r)
                continue
            if not p:
                continue
            if not os.path.isabs(p):
                # The gate runs with cwd=tree, so a relative path is an
                # in-tree path spelled the short way. Reading it as
                # "outside" made nine gates unjudgeable that were not.
                r = os.path.normpath(p)
                if os.path.isfile(os.path.join(tree, r)) and r != rel:
                    opened.add(r)
                continue
            if not any(seg in p for seg in NOT_A_SUBJECT):
                rp = os.path.realpath(p)
                born_here = False
                if rp.startswith(tmp_root + os.sep):
                    entry = rp[len(tmp_root) + 1 :].split(os.sep)[0]
                    born_here = entry not in tmp_before
                if not born_here:
                    try:
                        born_here = os.path.getmtime(p) >= started
                    except OSError:
                        born_here = True
                # A file the gate wrote during its own run is not a
                # subject it reads — most gates spool through a temp
                # file, and counting those made eighteen of them
                # unjudgeable. What matters is a subject that was
                # already there and that this sweep cannot take away.
                if born_here:
                    continue
                # A subject this sweep cannot take away. `cheap-gates-
                # come-first` reads the ship's profile from /tmp, so
                # taking every in-tree file away left it green and this
                # reported it as hollow — a finding about the
                # instrument's reach, not about the gate.
                elsewhere.add(p)
    return code, opened, said, elsewhere


def main() -> int:
    gates = gates_from_ship(ROOT)
    if gates is None:
        return 1
    if not gates:
        # A sweep that found no gates agrees with every repo there is.
        print("gate-subject: CANNOT RUN — no gate invocations found in the ship")
        return 1

    tree = tempfile.mkdtemp(prefix="smix-gate-subject-")
    aside = tempfile.mkdtemp(prefix="smix-gate-aside-")
    audit_dir = tempfile.mkdtemp(prefix="smix-gate-audit-")
    open(os.path.join(audit_dir, "sitecustomize.py"), "w", encoding="utf-8").write(AUDIT)

    no_subject, held, findings, unjudgeable, not_observable = [], [], [], [], []
    try:
        pristine_copy(ROOT, tree)
        for lang, rel in gates:
            if os.path.basename(rel) == os.path.basename(__file__):
                # Not an exemption — a sweep cannot be its own subject.
                # Running it here runs it nested, and the nested one
                # would do the same; it showed up as a three-minute
                # timeout on the first run after it was wired into the
                # ship. Its own red is proved by its self-test instead.
                unjudgeable.append(
                    (rel, "the sweep cannot sweep itself; its `.test.py` proves it can go red")
                )
                continue
            if lang != "python3":
                not_observable.append(rel)
                continue
            base, opened, said, elsewhere = run_gate(tree, lang, rel, audit_dir)
            if base is None:
                unjudgeable.append((rel, "timed out before it answered"))
                continue
            if base != 0:
                # With the reason, because "could not be judged" repeated
                # nine times is a shrug, and a reader cannot tell an
                # environment this copy lacks from a gate that is simply
                # broken.
                unjudgeable.append(
                    (rel, f"already exit {base} in the copy — {said[:120] or 'said nothing'}")
                )
                continue
            if not opened:
                no_subject.append(rel)
                continue
            if elsewhere:
                # Judged only if everything it reads can be taken away.
                unjudgeable.append(
                    (
                        rel,
                        f"also reads {len(elsewhere)} file(s) outside the tree "
                        f"(e.g. {sorted(elsewhere)[0]}) — taking the in-tree ones "
                        f"away proves nothing about it",
                    )
                )
                continue
            moved = []
            for r in sorted(opened):
                src = os.path.join(tree, r)
                dst = os.path.join(aside, r)
                os.makedirs(os.path.dirname(dst), exist_ok=True)
                try:
                    shutil.move(src, dst)
                    moved.append(r)
                except OSError:
                    pass
            after, _, _, _ = run_gate(tree, lang, rel, None)
            for r in moved:
                dst = os.path.join(tree, r)
                os.makedirs(os.path.dirname(dst), exist_ok=True)
                shutil.move(os.path.join(aside, r), dst)
            if after == 0:
                findings.append((rel, len(moved)))
            else:
                held.append(rel)
    finally:
        shutil.rmtree(tree, ignore_errors=True)
        shutil.rmtree(aside, ignore_errors=True)
        shutil.rmtree(audit_dir, ignore_errors=True)

    print(f"gate-subject: {len(held)} gates refused once their subject was gone")
    print(f"gate-subject: {len(no_subject)} read nothing in the repo (no subject to take)")
    for rel in no_subject:
        print(f"    {rel}")
    if unjudgeable:
        print(f"gate-subject: {len(unjudgeable)} could not be judged")
        for rel, why in unjudgeable:
            print(f"    {rel} — {why}")
    if not_observable:
        print(
            f"gate-subject: {len(not_observable)} are bash and this instrument "
            f"only sees Python opens — listed, not counted as passing"
        )
        for rel in not_observable:
            print(f"    {rel}")

    if findings:
        print("\ngate-subject: RED — a gate that says yes with its subject gone\n")
        for rel, n in findings:
            print(
                f"  {rel}: exit 0 after {n} file(s) it had read were taken away. "
                f"A predicate true on an empty subject has no subject."
            )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
