#!/usr/bin/env python3
"""Every Maven task the ship really runs must build its task graph.

The dry run publishes to the local Maven repo. `publishToMavenLocal` does
not create a Sonatype staging repository, so it never touches the build
service that the real `publish` shares across projects — and 36 dry runs
went green on a release whose real Maven leg failed in the first second,
before it had signed or uploaded anything:

    Cannot set the value of task ':sdk:createStagingRepository' property
    'buildService' ... loaded with ...project-sdk(export)... using a
    provider of type ... loaded with ...project-probe(export).

Two sibling projects each naming `com.vanniktech.maven.publish` with its
version got two copies of the plugin's classes, and a build service cannot
cross that boundary. Either project alone published fine. The fix is to
declare the plugin once in the root build script with `apply false`; this
gate is what would have caught it, because `--dry-run` builds the complete
task graph — including createStagingRepository — while uploading nothing.

The task names are read out of ship.sh rather than written here again. A
third published artifact should extend this gate by being added to the
release, not by somebody remembering this file exists.
"""
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
# A path argument is how the self-test hands this gate a ship.sh whose
# task list it has falsified. Without one it reads the real release.
SHIP = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "scripts" / "release" / "ship.sh"

m = re.search(r'^GRADLE_PUB_TASKS=\(([^)]*)\)', SHIP.read_text(), re.M)
if not m:
    print("the-publish-graph-builds: FAIL")
    print(f"  - no GRADLE_PUB_TASKS=(...) line in {SHIP};")
    print("    the gate reads the real task names from there and cannot")
    print("    guess them. If the release moved them, point this gate at")
    print("    their new home rather than copying them in here.")
    sys.exit(1)

tasks = re.findall(r'"([^"]+)"', m.group(1))
if not tasks:
    print("the-publish-graph-builds: FAIL")
    print("  - GRADLE_PUB_TASKS is empty, so this gate would run gradle with")
    print("    no tasks and pass on nothing. An empty predicate is not a")
    print("    predicate.")
    sys.exit(1)

proc = subprocess.run(
    ["./gradlew", "--dry-run", *tasks, "--console=plain"],
    cwd=ROOT / "android-runner",
    capture_output=True,
    text=True,
)
out = proc.stdout + proc.stderr

if proc.returncode != 0:
    print("the-publish-graph-builds: FAIL")
    print(f"  - `gradlew --dry-run {' '.join(tasks)}` exited {proc.returncode}.")
    print("    The real Maven leg of the release will fail the same way, an")
    print("    hour in, after crates.io and npm have already gone out.")
    print()
    for line in out.splitlines():
        print(f"    {line}")
    sys.exit(1)

# Exit 0 is not enough: gradle is content to build a graph for one project
# and say nothing about a task name it never resolved. Each project the
# release publishes has to appear in the plan by name.
missing = [t for t in tasks if t not in out]
if missing:
    print("the-publish-graph-builds: FAIL")
    print(f"  - gradle exited 0 but its plan never named: {', '.join(missing)}")
    print("    A task that does not appear in the dry-run plan is not one")
    print("    this gate has checked.")
    sys.exit(1)

# The staging repository is the thing that broke, and it is created only on
# the Central Portal path. Its absence would mean this gate had quietly
# gone back to checking publishToMavenLocal's easier route.
staging = [ln for ln in out.splitlines() if ":createStagingRepository" in ln]
if not staging:
    print("the-publish-graph-builds: FAIL")
    print("  - the plan contains no :createStagingRepository task, so it is")
    print("    not the Central Portal path this gate exists for. Check that")
    print(f"    {', '.join(tasks)} still publish to Maven Central.")
    sys.exit(1)

print(f"the-publish-graph-builds: clean — {len(tasks)} publish tasks "
      f"({', '.join(tasks)}) build their graph, "
      f"{len(staging)} staging repositories among them")
