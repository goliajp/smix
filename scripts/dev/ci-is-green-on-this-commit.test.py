#!/usr/bin/env python3
"""Does the CI-green gate go red, and for the right reason?

Its three judgements are made against a git checkout and the GitHub API,
so each case builds a real throwaway repo and stubs `gh` and `git` on
PATH. Stubbing the API rather than reaching for it keeps the cases
offline and lets them describe a run this repository has never had —
notably the one that matters most: a run that is green at the run level
while one of its jobs is red, which is what a `continue-on-error` job
produces and what the previous check could not see.
"""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "ci-is-green-on-this-commit.py")


def make_repo(dirty=False):
    """A checkout with the gate committed into it.

    The gate must be committed, not merely present: it locates its repo as
    parents[2] of its own path, so a copy dropped in afterwards leaves the
    worktree dirty and the first judgement swallows every later case. The
    first draft did exactly that, and six of seven cases failed on a
    verdict about the fixture rather than about their subject.
    """
    import shutil
    d = tempfile.mkdtemp()
    root = os.path.join(d, "repo")
    os.makedirs(os.path.join(root, "scripts", "dev"))
    shutil.copy(GATE, os.path.join(root, "scripts", "dev", os.path.basename(GATE)))
    subprocess.run(["git", "init", "-q"], cwd=root, check=True)
    subprocess.run(["git", "config", "user.email", "t@t"], cwd=root, check=True)
    subprocess.run(["git", "config", "user.name", "t"], cwd=root, check=True)
    with open(os.path.join(root, "f"), "w") as fh:
        fh.write("x")
    subprocess.run(["git", "add", "-A"], cwd=root, check=True)
    subprocess.run(["git", "commit", "-qm", "c"], cwd=root, check=True)
    if dirty:
        with open(os.path.join(root, "untracked"), "w") as fh:
            fh.write("y")
    return root


def stub_gh(runs_json, jobs_tsv, fail=False):
    """A `gh` on PATH that answers `run list` and `api .../jobs`."""
    d = tempfile.mkdtemp()
    path = os.path.join(d, "gh")
    if fail:
        body = 'echo "not authenticated" >&2; exit 1'
    else:
        body = (
            'if [ "$1" = "run" ]; then cat <<\'EOF\'\n'
            f"{runs_json}\n"
            "EOF\n"
            "else cat <<'EOF'\n"
            f"{jobs_tsv}\n"
            "EOF\n"
            "fi"
        )
    with open(path, "w") as fh:
        fh.write("#!/usr/bin/env bash\n" + body + "\n")
    os.chmod(path, 0o755)
    return d


def run(root, ghdir):
    env = dict(os.environ, PATH=ghdir + os.pathsep + os.environ["PATH"])
    p = subprocess.run(
        [sys.executable, os.path.join(root, "scripts", "dev", os.path.basename(GATE))],
        capture_output=True, text=True, env=env, cwd=root,
    )
    return p.returncode, p.stdout + p.stderr


def case(name, want_code, must_say, dirty=False, runs="[]", jobs="", gh_fails=False):
    root = make_repo(dirty=dirty)
    code, out = run(root, stub_gh(runs, jobs, fail=gh_fails))
    if code != want_code:
        print(f"  FAIL {name}: exit {code}, wanted {want_code}\n{out}")
        return False
    if "Traceback" in out:
        print(f"  FAIL {name}: red by raising, not by judging\n{out}")
        return False
    if must_say not in out:
        print(f"  FAIL {name}: does not say {must_say!r}\n{out}")
        return False
    print(f"  ok   {name}")
    return True


GREEN_RUN = '[{"databaseId":777,"conclusion":"success","status":"completed"}]'


def main():
    ok = True

    ok &= case(
        "a dirty worktree is not what CI judged",
        1, "never saw",
        dirty=True, runs=GREEN_RUN, jobs="build\tsuccess",
    )

    ok &= case(
        "no run for this commit",
        1, "no ci.yml run for HEAD",
        runs="[]",
    )

    # The one the previous check could not see. `gh run list` says the run
    # succeeded; a job inside it did not.
    ok &= case(
        "a green run with a red job inside it",
        1, "continue-on-error",
        runs=GREEN_RUN,
        jobs="rust-and-swift\tsuccess\nportable-corpus\tfailure",
    )

    ok &= case(
        "a run still going",
        1, "still",
        runs='[{"databaseId":778,"conclusion":null,"status":"in_progress"}]',
        jobs="rust-and-swift\tsuccess\nportable-corpus\t",
    )

    # A run reporting no jobs agrees with everything.
    ok &= case(
        "a run with no jobs in it",
        1, "agrees with everything",
        runs=GREEN_RUN, jobs="",
    )

    ok &= case(
        "gh cannot answer",
        2, "CANNOT RUN",
        runs=GREEN_RUN, gh_fails=True,
    )

    ok &= case(
        "every job green on a clean tree",
        0, "all 2 jobs green",
        runs=GREEN_RUN,
        jobs="rust-and-swift\tsuccess\nportable-corpus\tsuccess",
    )

    print(f"=== ci-is-green-on-this-commit.test: {'PASS' if ok else 'FAIL'} ===")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
