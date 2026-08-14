#!/usr/bin/env python3
"""Every route that drives the target app reads which app that is.

`App-Bundle-Id` is how a request says which app it means, and
`contextGuardedResponse` is the only thing that reads it. A route that
drives the app and forgets the wrapper does not fail — it silently uses
whichever app the runner booted with. `/tree` had the wrapper and
`/find` did not, so a cross-app flow could see an element in the tree
and fail to find it, and `/fill` typed into the wrong app entirely.

The wrapper's own doc comment listed the routes that should use it.
Nothing checked, so three were missing and had been for as long as
anyone had looked.

Which routes drive the app is decided here rather than read out of the
source, because the source is the thing under test: a route that lost
its wrapper would also lose its membership if membership were inferred
from the wrapper. So this list is the claim, and the scan is what keeps
the code equal to it.

Adding a route means adding it to one of the two lists below. That is
the point: the decision is made once, in writing, instead of being
whatever each registration happened to do.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SERVER = os.path.join(
    ROOT, "swift-bridge", "Sources", "SmixRunnerCore", "SmixRunnerServer.swift"
)

# Routes that act on the app under test. Each must read the header.
DRIVES_THE_APP = {
    "POST /tap",
    "GET /tree",
    "POST /fill",
    "POST /clear",
    "POST /find",
    "POST /scroll",
    "POST /foreground",
    "POST /back",
    "POST /swipe-once",
    "POST /tap-at-norm-coord",
    "POST /tap-by-id",
    "POST /find-text-by-ocr",
    "POST /swipe-at-norm-coord",
    "POST /double-tap",
    "POST /long-press",
    "POST /hide-keyboard",
    "POST /input-text",
    "POST /session/relaunch-app",
    "GET /system-popups",
    "POST /system-popup-action",
    "POST /set-orientation",
    "POST /record/start",
    "POST /record/stop",
    # Reports rather than acts, and still belongs here: half of what it
    # returns is `XCUIApplication.frame`, so answering about whichever
    # app the runner booted with would describe a different screen than
    # the caller asked about — the exact confusion it was added to
    # measure.
    "GET /coordinate-space",
}

# Routes that do not. Listed rather than assumed: "not in the first
# list" would let a new app-driving route be silently exempt.
INFRASTRUCTURE = {
    "GET /health",
    "POST /shutdown",
    "POST /soft-cycle",
    "POST /press-key",
    "GET /screenshot",
    "GET /record/poll",
    "POST /session/open",
    "POST /session/close",
    "POST /session/close-all",
    "POST /session/list",
    "POST /session/renew-activation",
    "POST /diagnostic/dump",
}

problems: list[str] = []


def routes_with_their_wrapper() -> dict[str, bool]:
    """Each registered route, and whether its body reaches the wrapper."""
    if not os.path.isfile(SERVER):
        problems.append(f"no runner server at {SERVER}")
        return {}
    out: dict[str, bool] = {}
    current: str | None = None
    for line in open(SERVER):
        m = re.search(r'appendRoute\("([A-Z]+ [^"]+)"', line)
        if m:
            current = m.group(1)
            out[current] = False
            continue
        if current and "contextGuardedResponse" in line:
            out[current] = True
    return out


found = routes_with_their_wrapper()

# A parser that finds nothing agrees with any server at all.
if found and len(found) < 20:
    problems.append(
        f"only {len(found)} routes parsed out of the server — the registration "
        "shape changed and this scan is now reading air"
    )

for route, wrapped in sorted(found.items()):
    if route in DRIVES_THE_APP and not wrapped:
        problems.append(
            f"{route} drives the app and does not read `App-Bundle-Id` — it will "
            "use whichever app the runner booted with, silently"
        )
    elif route in INFRASTRUCTURE and wrapped:
        problems.append(
            f"{route} is listed as infrastructure and reads the header; either it "
            "drives the app after all, or the wrapper is noise here"
        )
    elif route not in DRIVES_THE_APP and route not in INFRASTRUCTURE:
        problems.append(
            f"{route} is in neither list. Say which it is: does it act on the app "
            "under test, or on the runner / device / springboard?"
        )

for route in sorted(DRIVES_THE_APP | INFRASTRUCTURE):
    if found and route not in found:
        problems.append(f"{route} is listed here and no longer registered — drop it")

if problems:
    print("route-context-scan: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

drives = sum(1 for r in found if r in DRIVES_THE_APP)
print(
    f"route-context-scan: clean — {drives} app-driving routes read `App-Bundle-Id`, "
    f"{len(found) - drives} infrastructure routes deliberately do not"
)
