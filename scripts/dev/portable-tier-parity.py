#!/usr/bin/env python3
"""CI runs the portable tier by the same script you can run yourself.

The job in `ci.yml` and the script in `scripts/release/` are two places
that have to agree, and this repository has already watched that kind of
agreement rot: `preflight.sh` opens with "run, locally, the checks CI
will run" and had gone two major versions without running `swift test`,
which is how a broken Swift test reached a commit with preflight clean.

So the job may not inline its own version of the tier. It calls the
script, and this checks that it does — which also means anyone
reproducing a CI failure runs the same thing, rather than something that
was the same when it was written.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CI = os.path.join(ROOT, ".github", "workflows", "ci.yml")
SCRIPT_REL = "scripts/release/portable-tier.sh"
SCRIPT = os.path.join(ROOT, SCRIPT_REL)

problems: list[str] = []

if not os.path.isfile(SCRIPT):
    problems.append(f"{SCRIPT_REL} does not exist, and the CI job runs it")
elif not os.access(SCRIPT, os.X_OK) and "#!/usr/bin/env bash" not in open(SCRIPT).readline():
    problems.append(f"{SCRIPT_REL} is neither executable nor a bash script")

ci = open(CI, encoding="utf-8").read() if os.path.isfile(CI) else ""
if not ci:
    problems.append("no ci.yml")

# The job must exist and must reach the script.
if ci:
    if "portable-corpus:" not in ci:
        problems.append(
            "ci.yml has no `portable-corpus` job. The portable tier exists so a "
            "runner can execute part of the corpus; a tier nothing runs is a "
            "directory of yaml files"
        )
    elif SCRIPT_REL not in ci:
        problems.append(
            f"the CI job does not call {SCRIPT_REL}. Inlining the steps makes "
            f"two things that must agree, and the one that drifts is the one "
            f"nobody runs by hand"
        )

    # The flow list must not be restated in the workflow.
    #
    # `portable-tier.sh` derives it from corpus-portability-scan; a
    # workflow naming flows itself would be a third copy, and every copy
    # of a list in this cycle had drifted from its original.
    for flow in re.findall(r"portable-[a-z-]+", ci):
        if flow != "portable-corpus" and f"{flow}.yaml" in ci:
            problems.append(
                f"ci.yml names the flow {flow!r} directly. The tier is derived "
                f"from corpus-portability-scan; a list here is a copy that will "
                f"drift"
            )

if problems:
    print("portable-tier-parity: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print(f"portable-tier-parity: clean — the CI job runs {SCRIPT_REL}, no flow list of its own")
