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

Scope: the scripts the release path actually runs -- derived by reading
ship.sh, preflight.sh, the version's exit acceptance and the CI
workflows for the `.sh` files they name, not by a list kept here that
would go stale. The harm this scan exists for is "the next gate on the
same device", and that only exists where gates run one after another.

Thirteen older per-version e2e scripts discard their teardown's stderr
too, and one of them (`v5.1-c11`) passes `--runner-port`, which `runner
down` does not accept -- the same never-worked teardown. Nothing runs
them today. They are named in `.claude/docs/open-items.md` rather than
fixed in a release week and left unrun; when one is wired back into the
release path this scan picks it up on its own.
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
BRINGS_IT_DOWN = re.compile(
    r"""\$?\{?(?:smix|SMIX_BIN|SMIX)\}?"?(?:/[\w/.\-]*smix)?\s+runner\ down\b""",
)
# `2>/dev/null` or `>/dev/null 2>&1` on the same line as the teardown.
SWALLOWS_STDERR = re.compile(r"2>\s*/dev/null|>\s*/dev/null\s+2>&1")
MENTIONS_ONLY = re.compile(r"^\s*(?:#|log\b|echo\b|printf\b|\*[\"'])")

# The release path, read rather than remembered.
ENTRY_POINTS = [
    "scripts/release/ship.sh",
    "scripts/dev/preflight.sh",
    os.path.join(".github", "workflows", "ci.yml"),
]
_version = open(os.path.join(ROOT, "Cargo.toml"), encoding="utf-8").read()
_major = re.search(r'^version\s*=\s*"(\d+)', _version, re.M)
if _major:
    ENTRY_POINTS.append(f"scripts/dev/v{_major.group(1)}-exit.sh")

NAMED = re.compile(r"[\w./-]+\.sh")
in_scope: set[str] = set()
for entry in ENTRY_POINTS:
    try:
        text = open(os.path.join(ROOT, entry), encoding="utf-8").read()
    except OSError:
        continue
    in_scope.add(os.path.basename(entry))
    for hit in NAMED.findall(text):
        in_scope.add(os.path.basename(hit))

problems: list[str] = []
checked = 0

for dirpath, _dirs, files in os.walk(SCRIPTS):
    for name in sorted(files):
        if not name.endswith(".sh"):
            continue
        if name not in in_scope:
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

if checked == 0:
    problems.append(
        "no script on the release path was found starting a runner — either "
        "the invocation shape changed or the entry points stopped naming the "
        "gates, and this scan is now reading air"
    )
# The derivation is the part most likely to break quietly: an entry point
# renamed, a gate invoked through a variable. Both would empty the scope
# without emptying the repo, and an empty scope agrees with everything.
if len(in_scope) < len(ENTRY_POINTS):
    problems.append(
        f"the release path named only {len(in_scope)} scripts — the entry "
        f"points are not being read: {ENTRY_POINTS}"
    )

if problems:
    print("every-runner-a-gate-starts-comes-down: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print(
    f"every-runner-a-gate-starts-comes-down: clean — {checked} gates start a "
    f"runner, each takes one down and lets the teardown speak"
)
