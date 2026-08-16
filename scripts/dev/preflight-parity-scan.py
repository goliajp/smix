#!/usr/bin/env python3
"""preflight runs what CI runs, or says out loud which step it skips.

The first line of `scripts/dev/preflight.sh` is "run, locally, the
checks CI will run". Nothing checked that sentence. Two CI steps had no
local counterpart at all — `swift test` and the UITest build — and a
change to a handler's return type in `SmixRunnerServer` left one Swift
test uncompilable while preflight reported clean. The commit went in.

That is the failure this file is about, and it is not the same as a gate
being wrong: preflight was right about everything it ran. The gap was
between what it ran and what it claimed. A promise nobody checks decays
into a description of the past.

Every CI step is therefore in one of two lists. LOCAL names the fragment
of `preflight.sh` that runs the same thing. NOT_LOCAL names the step and
why running it locally is not wanted — a downgrade stated on purpose
rather than one nobody noticed. A step in neither list fails this scan,
because "not listed" must never be the way something becomes exempt.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CI = os.path.join(ROOT, ".github", "workflows", "ci.yml")
PREFLIGHT = os.path.join(ROOT, "scripts", "dev", "preflight.sh")

# CI step name -> a fragment that must appear in preflight.sh.
LOCAL = {
    "rustfmt": "cargo fmt --all --check",
    "clippy": "cargo clippy",
    "cargo test": "cargo test",
    "swift test": "swift test",
    "unit tests + androidTest compile (no device — instrumentation runs at ship)": "assembleDebugAndroidTest",
    "hygiene scan": "hygiene-scan",
    "route conformance": "route-conformance",
    "fact scan": "fact-scan",
    "workflow scan": "workflow-scan",
    "gate subject diversity": "gate-subject-diversity",
    "route context scan": "route-context-scan",
    "gate port scan": "gate-port-scan",
    "preflight parity scan": "preflight-parity-scan",
    "mcp cli parity scan": "mcp-cli-parity-scan",
    "known-unstable list scan": "known-unstable-scan",
    "corpus portability scan": "corpus-portability-scan",
    "portable tier parity": "portable-tier-parity",
    "a yield is not a failure": "yield-is-not-failure-scan",
    "teardown restores rather than imposes": "teardown-restores-scan",
    "android gates drive our own app": "android-subject-scan",
    "device facts are machine-scoped": "device-facts-are-machine-scoped",
    "leases are machine-scoped": "leases-are-machine-scoped",
    "the hook tells commands from prose": "hook-command.test",
    "the dev-sim picker asks the ledger": "pick-dev-sim.test",
    "no second ledger path": "no-second-ledger-path",
    "a retired sentence is off the surfaces": "retired-claims-scan",
    "the retired-wording scan can still go red": "retired-claims-scan.test",
    "the contract gate can still go red": "contract-scan.test",
    "every selector form is declared on every surface": "selector-surface-scan",
    "the selector-surface gate can still go red": "selector-surface-scan.test",
    "every health_ok call site says whether it decides": "health-is-not-a-session-check",
    "the health/session gate can still go red": "health-is-not-a-session-check.test",
    "every session probe says which app it asks about": "probes-name-the-app",
    "the probe-naming gate can still go red": "probes-name-the-app.test",
    "tap-then-frame is one path": "tap-then-capture-is-one-path",
    "the one-path gate can still go red": "tap-then-capture-is-one-path.test",
    "every flag says what it does": "every-flag-says-what-it-does",
    "the flag-description gate can still go red": "every-flag-says-what-it-does.test",
    "an-authorised-hatch-reaches-every-surface": "an-authorised-hatch-reaches-every-surface",
    "an-authorised-hatch-reaches-every-surface harness": "an-authorised-hatch-reaches-every-surface.test",
    "a-tap-proves-aim-not-arrival": "a-tap-proves-aim-not-arrival",
    "a-tap-proves-aim-not-arrival harness": "a-tap-proves-aim-not-arrival.test",
    "no-script-picks-a-device-by-accident": "no-script-picks-a-device-by-accident",
    "no-script-picks-a-device-by-accident harness": "no-script-picks-a-device-by-accident.test",
    "portable tier selftest": "portable-tier.sh --selftest",
    "guide corpus in step with the guides": "guide-corpus-sync.py --check",
    "fence check": "fence-check.sh",
    "android gate scan": "android-gate-scan",
    "stress tier selector self-test": "stress-select.py --test",
    "stress-gate aggregation self-test": "stress-gate.sh --selftest",
    "flake classifier self-test": "flake-classify.test.py",
    "corpus-gate verdict self-test": "corpus-gate.sh --selftest",
    "determinism verdict self-test": "v3.0-c3-determinism.sh --selftest",
    "llms.txt freshness": "gen-llms.py --check",
    "smix-server wiring suite": "cargo test",
}

# CI step -> why preflight does not run it. Each of these is a real
# downgrade; naming it is the point.
NOT_LOCAL = {
    "SmixRunner UITest build": (
        "xcodebuild build-for-testing is minutes, not seconds, and preflight "
        "runs dozens of times a day"
    ),
    "install (workspace)": "bun install is CI setup, not a check",
    "build napi addon": "prerequisite of the vitest step below, not a check itself",
    "typecheck": "the TS SDK's own toolchain; run from npm/smix-rn when working there",
    "vitest": "same",
    "clean-room install + drive": (
        "packs and installs into an empty project on a PATH without cargo — "
        "the point is a machine unlike this one"
    ),
    "build binaries": "cross-target release build matrix; a native runner each",
    "build .node": "same, for the napi addon",
    "package structure": "part of the prebuild matrix",
    "build smix (for the MCP parity scan)": (
        "a fresh checkout has no binary; locally the gate should judge whatever "
        "you last built, and it refuses rather than guessing when there is none"
    ),
    "what runtimes this runner has": "a runner's image is not this machine; printed there, meaningless here",
    "build smix": "CI setup for the job below, not a check",
    "boot a simulator": "CI setup; locally you already have one",
    "portable corpus tier": (
        "needs a booted simulator, so it stays out of a device-free preflight; "
        "run `scripts/release/portable-tier.sh` yourself when working on it"
    ),
    "upload artifact": "CI plumbing",
}

problems: list[str] = []

if not os.path.isfile(CI) or not os.path.isfile(PREFLIGHT):
    print("preflight-parity-scan: FAIL")
    print("  - ci.yml or preflight.sh is missing")
    sys.exit(1)

# What preflight RUNS — not what it says about what it runs.
#
# Two drafts of this line were fooled in a row. Searching the whole file
# passed a preflight whose `swift test` invocation had been deleted,
# because the comment above it explaining the step still contained the
# words; stripping comments passed it again, because the `echo "--- swift
# test"` banner above the invocation does too.
#
# A gate that reads prose as evidence of execution fails in the direction
# that reports coverage, which is the only direction that matters. This
# is the third scan in this repository to learn it.
NARRATION = re.compile(r"^\s*(?:#|echo\b|printf\b|log\b)")
preflight = "\n".join(
    line
    for line in open(PREFLIGHT, encoding="utf-8").read().splitlines()
    if not NARRATION.match(line)
)
steps = re.findall(r"^\s+- name: (.+)$", open(CI, encoding="utf-8").read(), re.M)

# A parser that finds nothing agrees with any workflow at all.
if len(steps) < 15:
    problems.append(
        f"only {len(steps)} CI steps parsed — the workflow's shape changed and "
        "this scan is now reading air"
    )

for step in steps:
    step = step.strip()
    if step in LOCAL:
        if LOCAL[step] not in preflight:
            problems.append(
                f'CI runs "{step}" and preflight is supposed to as well, but '
                f"{LOCAL[step]!r} is not in preflight.sh"
            )
    elif step not in NOT_LOCAL:
        problems.append(
            f'CI step "{step}" is in neither list. Either run it in preflight '
            "and add it to LOCAL, or add it to NOT_LOCAL with the reason it "
            "stays remote — silence must not be how a step becomes exempt"
        )

for step in LOCAL:
    if step not in [s.strip() for s in steps]:
        problems.append(f'"{step}" is listed here and CI no longer runs it — drop it')

if problems:
    print("preflight-parity-scan: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print(
    f"preflight-parity-scan: clean — {len(LOCAL)} CI steps have a local "
    f"counterpart, {len(NOT_LOCAL)} deliberately do not"
)
