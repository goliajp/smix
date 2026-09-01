#!/usr/bin/env python3
"""Does the publish-graph gate go red, and for the right reason?

Four shapes. The first two falsify how it learns the task names — it reads
them out of ship.sh rather than holding its own copy, so a release that
moves them must stop the gate rather than leave it checking a list nobody
publishes any more.

The third is the one this gate exists for. `publishToMavenLocal` builds its
task graph happily and creates no staging repository, which is exactly why
36 dry runs of 10.0.0 went green over a Maven leg that died in its first
second. A gate that would accept those task names has quietly gone back to
checking the easier path.
"""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "the-publish-graph-builds.py")


def ship_with(body):
    fd, path = tempfile.mkstemp(suffix=".sh")
    with os.fdopen(fd, "w") as fh:
        fh.write("#!/usr/bin/env bash\n" + body + "\n")
    return path


def run(ship_path):
    p = subprocess.run([sys.executable, GATE, ship_path],
                       capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


def case(name, body, want_code, must_say):
    code, out = run(ship_with(body))
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


def main():
    ok = True

    ok &= case(
        "a release that no longer names its publish tasks",
        'echo "no task list here"',
        1,
        "cannot",
    )

    ok &= case(
        "an empty task list is not a task list",
        'GRADLE_PUB_TASKS=()',
        1,
        "empty",
    )

    # The dry run's own task names. They build a graph and exit 0, so only
    # the staging-repository check separates them from the real thing.
    ok &= case(
        "the dry run's easier path is not the path being checked",
        'GRADLE_PUB_TASKS=(":sdk:publishToMavenLocal" ":probe:publishToMavenLocal")',
        1,
        ":createStagingRepository",
    )

    ok &= case(
        "the real task names build their graph",
        'GRADLE_PUB_TASKS=(":sdk:publish" ":probe:publish")',
        0,
        "staging repositories",
    )

    print(f"=== the-publish-graph-builds.test: {'PASS' if ok else 'FAIL'} ===")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
