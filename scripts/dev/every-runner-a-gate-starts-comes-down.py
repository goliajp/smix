#!/usr/bin/env python3
"""A gate that brings a runner up takes it down, and says so if it cannot.

Two xcodebuild test sessions against one simulator terminate each
other's runner app. The second `Activate` then waits for an app that
keeps being killed, XCTest's watchdog ends the session, and every flow
after it reports `runner unreachable` -- true, and about the wrong
thing.

Measured 2026-08-29 on the release corpus: `a-tap-that-cannot-land`
brought a runner up and had no teardown at all. Run alone, the corpus
gate is 26/26 green; run after that gate on the same sim, 23 of 26 red.
The sibling gate did have a teardown -- and it passed `--port`, which
`runner down` does not accept. The argument error went to /dev/null
behind `|| true`, so a teardown that had never once worked read exactly
like one that had.

Hence both halves. A missing teardown is the first; a teardown whose
stderr is discarded is the second, because that is what let the first
one hide for a whole cycle.

Scope: every script under `scripts/` that brings a runner up.

It was narrower for a day. Thirteen older per-version e2e scripts had
the same silenced teardown, nothing runs them, and fixing thirteen
scripts in a release week looked like trading one proven defect for
thirteen unverified changes -- so the scan was scoped to the release
path and the rest were named in `open-items.md` instead. That deferral
was overruled; they are fixed, and the narrowing went with them,
because a scope that exists to excuse work already done is just an
exemption with a better name.

Two of those thirteen findings were this scan being wrong, which is
worth keeping: it required the word "runner" in the teardown and so read
`smix down` -- the capsule's verb for the same act -- as no teardown at
all, and it counted a `runner up` aimed at a serial that reaches
nothing, whose next line asserts the refusal, as a runner that could be
left running. A gate that reports work which does not need doing is the
same failure as one that misses work: both say something untrue.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SCRIPTS = os.path.join(ROOT, "scripts")

STARTS_A_RUNNER = re.compile(
    r"""^\s*
        (?:if\s+!?\s*|\(\s*cd\s[^&]*&&\s*)?
        "?\$?\{?(?:smix|SMIX_BIN|SMIX)\}?"?
        (?:/[\w/.\-]*smix)?
        \s+runner\ up\b""",
    re.VERBOSE,
)
# `runner down`, and the capsule's `down`, which is the same act through
# a different verb. Requiring the word "runner" read two scripts as
# having no teardown at all when they had one -- the finding named work
# that did not need doing, which is the failure this file is otherwise
# about.
BRINGS_IT_DOWN = re.compile(
    r"""\$?\{?(?:smix|SMIX_BIN|SMIX)\}?"?(?:/[\w/.\-]*smix)?\s+(?:runner\ )?down\b""",
)
# A `runner up` the script requires to be REFUSED never starts one, so
# there is nothing for it to stop. Keyed on the assertion rather than on
# the fixture's name: `v2.3-c15` aims one at a serial that reaches
# nothing and then greps the output for the refusal.
IS_A_REFUSAL_CASE = re.compile(r"did not refuse|refused:|may not address|is not a device")
# `2>/dev/null` or `>/dev/null 2>&1` on the same line as the teardown.
SWALLOWS_STDERR = re.compile(r"2>\s*/dev/null|>\s*/dev/null\s+2>&1")
MENTIONS_ONLY = re.compile(r"^\s*(?:#|log\b|echo\b|printf\b|\*[\"'])")

problems: list[str] = []
checked = 0
judged = 0
refusal_cases: list[str] = []
no_runner_survives: list[str] = []

for dirpath, _dirs, files in os.walk(SCRIPTS):
    for name in sorted(files):
        if not name.endswith(".sh"):
            continue
        path = os.path.join(dirpath, name)
        rel = os.path.relpath(path, ROOT)
        try:
            body = open(path, encoding="utf-8").read()
        except (OSError, UnicodeDecodeError):
            continue
        lines = body.splitlines()
        if not any(
            STARTS_A_RUNNER.match(ln) and not MENTIONS_ONLY.match(ln) for ln in lines
        ):
            continue
        checked += 1
        # Only the ups that could actually leave something running.
        real_ups = []
        for idx, ln in enumerate(lines):
            if not (STARTS_A_RUNNER.match(ln) and not MENTIONS_ONLY.match(ln)):
                continue
            near = " ".join(lines[idx : idx + 4])
            if IS_A_REFUSAL_CASE.search(near):
                refusal_cases.append(f"{rel}:{idx + 1}")
                continue
            real_ups.append(ln)
        if not real_ups:
            no_runner_survives.append(rel)
            continue
        judged += 1
        downs = [
            ln for ln in lines
            if BRINGS_IT_DOWN.search(ln) and not MENTIONS_ONLY.match(ln)
        ]
        if not downs:
            problems.append(
                f"{rel} brings a runner up and never brings one down. It stays "
                f"on the device, and the next gate to start one there gets two "
                f"xcodebuild sessions fighting over one simulator -- measured, "
                f"that is 23 of 26 corpus flows red about the wrong thing."
            )
            continue
        silenced = [ln.strip() for ln in downs if SWALLOWS_STDERR.search(ln)]
        if silenced:
            problems.append(
                f"{rel} discards what its teardown says. A wrong flag, a device "
                f"it cannot address, a runner that will not stop -- all of them "
                f"then look exactly like a teardown that worked.\n"
                f"      {silenced[0]}"
            )

# The refusal carve-out has to exclude something, or it is a sentence
# that reads like consideration and considers nothing (§14.7).
if no_runner_survives and not refusal_cases:
    problems.append(
        "a script was excused as never starting a runner, but no `runner up` "
        "in it was found to be a refusal case — the carve-out matched nothing "
        "and excused something anyway"
    )
# And it has to leave something behind. Widened to match everything, the
# carve-out excused every script in the repo and this still printed "22
# gates, each takes one down": a true-sounding count of a set nothing was
# checked against. Found by making it always fire; the guard above did not
# catch it, because the carve-out having members is not the same as the
# subject having any.
if checked and not judged:
    problems.append(
        f"all {checked} runner-starting scripts were excused as refusal "
        f"cases — the carve-out is swallowing the subject, and a summary "
        f"over an empty set agrees with anything"
    )
if checked == 0:
    problems.append(
        "no script was found starting a runner — the invocation shape "
        "changed and this scan is now reading air"
    )

if problems:
    print("every-runner-a-gate-starts-comes-down: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

excused = f", {len(no_runner_survives)} excused (every `up` in them is asserted to be refused)" if no_runner_survives else ""
print(
    f"every-runner-a-gate-starts-comes-down: clean — {judged} of {checked} "
    f"runner-starting scripts judged; each takes one down and lets the "
    f"teardown speak{excused}"
)
