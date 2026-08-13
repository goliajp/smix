#!/usr/bin/env python3
"""Tap-then-frame is one implementation, and the frame comes from the
process that tapped.

The whole value of the combined action is those two sentences, and
neither is visible to a behaviour test. A surface that quietly does its
own `tap()` then `screenshot()` works perfectly: same result, same
assertions green, and the frame is 237 ms later and taken from a
different layer than the touch. A second implementation on a third
surface is worse and just as quiet — two copies drift, and what drifts
first is the ordering, which is the entire point.

So this reads the source:

- `tap_then_capture_with` is defined in smix-sdk and reached from every
  surface that offers the combined action;
- smix-sdk references `HttpRunnerClient::screenshot` — the code-level
  evidence for "the frame comes from the runner", rather than a sentence
  in a guide;
- both route names live in that one function, so "Android goes another
  way" is said in the code and not only in the documentation (§9 #1 ③).

The plan for this gate said to forbid `self.screenshot(` inside the
combined action. That rule is unsatisfiable: Android's branch legitimately
is that call — it has no runner route to use. Forbidding it would have
refused a correct implementation, so the predicate below asks for the
evidence instead of banning a spelling.
"""

import os
import re
import sys

ROOT = (
    sys.argv[1]
    if len(sys.argv) > 1
    else os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
)
CRATES = os.path.join(ROOT, "crates")

HOME = os.path.join(CRATES, "smix-sdk", "src", "lib.rs")
CORE = "tap_then_capture_with"
RESULT = "CapturedAfterTap"
# The two answers `via` can carry. Both have to be in the one function:
# a platform difference that only exists in a document is one nobody
# reading the code can see.
ROUTES = ('"runner"', '"device-tooling"')
# The code-level evidence that the frame comes from the runner.
FROM_THE_RUNNER = "screenshot"

# Below either of these the scan is describing a codebase that is not
# this one. Small because this axis has few sites: the floors refuse a
# cleared-out tree, they do not describe a size.
MIN_SURFACES = 2
MIN_ROUTES = 2

DOC = re.compile(r"^\s*//[/!]")


def rust_files(root):
    """Product code only. A test that calls the combined action is
    exercising it, not offering it as a surface."""
    for base, dirs, names in os.walk(root):
        dirs[:] = [d for d in dirs if d not in ("target", ".git", "tests", "benches")]
        for name in sorted(names):
            if name.endswith(".rs"):
                yield os.path.join(base, name)


def read(path):
    with open(path, encoding="utf-8") as fh:
        return "\n".join(
            "" if DOC.match(line) else line
            for line in fh.read().splitlines()
        )


def function_body(text, name):
    """The text of `fn name(...)`, by brace matching from its signature."""
    at = text.find(f"fn {name}(")
    if at < 0:
        return None
    depth = 0
    started = False
    out = []
    for ch in text[at:]:
        out.append(ch)
        if ch == "{":
            depth += 1
            started = True
        elif ch == "}":
            depth -= 1
            if started and depth == 0:
                break
    return "".join(out)


problems = []

if not os.path.isdir(CRATES):
    print("tap-then-capture-is-one-path: FAIL")
    print(f"  - no crates/ under {ROOT} — this scan has nothing to read")
    sys.exit(1)

home = read(HOME) if os.path.isfile(HOME) else ""

# --- the subject exists where the rules point ----------------------------

body = function_body(home, CORE)
if body is None:
    problems.append(
        f"{CORE} is not defined in {os.path.relpath(HOME, CRATES)}. Every rule "
        "below is phrased in terms of it, so without it this scan goes quiet "
        "rather than red — which is the failure it exists to prevent"
    )
if f"struct {RESULT}" not in home:
    problems.append(
        f"{RESULT} is not defined in {os.path.relpath(HOME, CRATES)} — the "
        "combined action has no answer type, and this scan is reading air"
    )

# --- the frame comes from the runner, in code ----------------------------

if body is not None and FROM_THE_RUNNER not in body:
    problems.append(
        f"{CORE} never asks the runner for a frame. The point of the combined "
        "action is that the picture is taken by the process that tapped; a "
        "version that goes out to device tooling on iOS works, is 237 ms "
        "later, and no test can tell"
    )

# --- both routes are named in the code, not only in a guide --------------

named = [r for r in ROUTES if body is not None and r in body]
if len(named) < MIN_ROUTES:
    missing = [r for r in ROUTES if r not in named]
    problems.append(
        f"{CORE} names {len(named)} of the {len(ROUTES)} routes a frame can "
        f"come from — missing {', '.join(missing)}. A platform that goes "
        "another way has to say so where the code says it, or the difference "
        "exists only in a document (§9 #1 ③)"
    )

# --- every surface reaches the one implementation ------------------------

surfaces = []
for path in rust_files(CRATES):
    rel = os.path.relpath(path, CRATES)
    if os.path.abspath(path) == os.path.abspath(HOME):
        continue
    text = read(path)
    if CORE in text or "tap_then_capture(" in text:
        surfaces.append(rel)

if len(surfaces) < MIN_SURFACES:
    problems.append(
        f"{len(surfaces)} surface(s) reach the combined action — expected at "
        f"least {MIN_SURFACES} (a command line and a tool). Either one of "
        "them grew its own copy, or this scan is reading air: "
        f"{surfaces or 'none found'}"
    )

if problems:
    print("tap-then-capture-is-one-path: FAIL")
    for problem in problems:
        print(f"  - {problem}")
    sys.exit(1)

print(
    f"tap-then-capture-is-one-path: clean — {len(surfaces)} surfaces reach it, "
    f"the frame comes from the runner, {len(named)} routes named"
)
