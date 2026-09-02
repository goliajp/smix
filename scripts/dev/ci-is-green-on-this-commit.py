#!/usr/bin/env python3
"""Has CI passed, on the exact tree this ship is about to publish?

The release already depended on this and asked it too late and too
loosely. Too late: the question lived beside the `gh run download` that
pulls the three .node addons, at line 1349 of ship.sh — 144 minutes into
10.0.0's sixth run, after every gate and after crates.io had gone out.
Too loosely: it read the run-level `conclusion`, and a run whose job is
marked `continue-on-error` is green at the run level while that job is
red (see the `ci_job_allowed_to_fail_is_not_a_gate` finding).

Three things must hold, and the third is the one that makes the other two
mean anything:

1. a ci.yml run exists whose head_sha is HEAD;
2. every job in it concluded `success` — read per job, never from the run;
3. the worktree is clean.

Without (3) the answer is about HEAD while the ship publishes the
worktree, so a dirty tree would be signed off by a run that never saw it.

On success the run id is printed on stdout, so ship.sh takes the value
from the same call that judged it rather than asking twice and risking
two different answers.
"""

import json
import os
import subprocess
import sys

REPO = "goliajp/smix"


def sh(*args, cwd=None):
    p = subprocess.run(args, capture_output=True, text=True, cwd=cwd)
    return p.returncode, p.stdout.strip(), p.stderr.strip()


def main() -> int:
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

    code, head, err = sh("git", "rev-parse", "HEAD", cwd=root)
    if code != 0:
        print("ci-is-green-on-this-commit: CANNOT RUN")
        print(f"  - git rev-parse HEAD failed: {err}")
        return 2

    code, dirty, _ = sh("git", "status", "--porcelain", cwd=root)
    if code != 0:
        print("ci-is-green-on-this-commit: CANNOT RUN")
        print("  - git status failed; without it this gate cannot tell whether")
        print("    CI saw the tree that is about to be published.")
        return 2
    if dirty:
        n = len(dirty.splitlines())
        print("ci-is-green-on-this-commit: FAIL")
        print(f"  - the worktree has {n} uncommitted change(s). CI judged HEAD;")
        print("    the ship publishes the worktree. A green run says nothing")
        print("    about changes it never saw.")
        for line in dirty.splitlines()[:10]:
            print(f"      {line}")
        return 1

    code, out, err = sh(
        "gh", "run", "list", "--repo", REPO, "--workflow", "ci.yml",
        "--commit", head, "--json", "databaseId,conclusion,status",
    )
    if code != 0:
        print("ci-is-green-on-this-commit: CANNOT RUN")
        print(f"  - `gh run list` failed — is gh authenticated for {REPO}?")
        print(f"    {err}")
        return 2

    runs = json.loads(out or "[]")
    if not runs:
        print("ci-is-green-on-this-commit: FAIL")
        print(f"  - no ci.yml run for HEAD ({head}).")
        print("    Push HEAD and let CI run. The ship downloads the three .node")
        print("    addons and the CLI binaries from that run's artifacts, so")
        print("    there is nothing to publish until it exists.")
        return 1

    # Per job, never the run. A run carrying a continue-on-error job is
    # green at the run level with that job red inside it.
    for run in runs:
        rid = run["databaseId"]
        code, out, err = sh(
            "gh", "api", f"repos/{REPO}/actions/runs/{rid}/jobs",
            "--paginate", "-q",
            ".jobs[] | [.name, .conclusion] | @tsv",
        )
        if code != 0:
            print("ci-is-green-on-this-commit: CANNOT RUN")
            print(f"  - could not read the jobs of run {rid}: {err}")
            return 2

        # A job still running has a null conclusion, which @tsv renders as
        # an empty field — and a trailing empty field does not always
        # survive the trip. Parsed so that a missing verdict is read as
        # "no verdict" rather than raising: a red that is a traceback
        # carries no judgement.
        jobs = []
        for ln in out.splitlines():
            if not ln.strip():
                continue
            parts = ln.split("\t")
            jobs.append((parts[0], parts[1] if len(parts) > 1 else ""))
        if not jobs:
            print("ci-is-green-on-this-commit: FAIL")
            print(f"  - run {rid} reports no jobs at all. A run with nothing in")
            print("    it agrees with everything; refusing rather than reading")
            print("    its run-level conclusion.")
            return 1

        bad = [(n, c) for n, c in jobs if c != "success"]
        if bad:
            if run.get("status") != "completed":
                print("ci-is-green-on-this-commit: FAIL")
                print(f"  - run {rid} for HEAD is still {run.get('status')}.")
                print("    Wait for it rather than publishing beside it.")
                return 1
            print("ci-is-green-on-this-commit: FAIL")
            print(f"  - run {rid} is for HEAD, and {len(bad)} of its {len(jobs)} "
                  f"jobs did not succeed:")
            for n, c in bad[:12]:
                print(f"      {c or 'no conclusion'}\t{n}")
            print("    The run-level conclusion is not consulted here: a job")
            print("    marked continue-on-error is red inside a green run.")
            return 1

        print(f"ci-is-green-on-this-commit: clean — run {rid} on {head[:12]}, "
              f"all {len(jobs)} jobs green, worktree clean")
        print(rid)
        return 0

    print("ci-is-green-on-this-commit: FAIL")
    print(f"  - {len(runs)} ci.yml run(s) exist for HEAD and none is all-green.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
