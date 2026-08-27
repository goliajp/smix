#!/usr/bin/env python3
"""Polling ends a wait while the screen is still moving. Quiescence does not.

This started out asking whether being told is FASTER than guessing, and the
answer, measured fairly, was no: on a screen that settles in milliseconds
there is no waiting to save, and the first run's 20x was this script
charging the polling side ~50 ms of process startup per attempt.

The difference is not speed. It is that polling answers a question about
EXISTENCE — has this selector resolved — and a wait wants to know about
MOTION. A row is in the tree throughout a fling; a poll finds it and says
"ready" while it is still sliding, and a tap placed on that verdict lands
where the row no longer is.

So this measures the thing that actually differs. During a real scroll:

  the polling strategy      -> "ready" (the tag is there, it always was)
  the quiescence strategy   -> "not yet" (the tree is changing)

and only after the motion stops does quiescence agree.

The stimulus is verified before the verdicts are read. An earlier version of
this measurement swiped at coordinates that missed the list entirely and
concluded a signal was useless from a scroll that never happened.

Usage:
  a-wait-that-does-not-end-early.py --device emulator-5554 --port 22095
"""

import argparse
import json
import re
import subprocess
import sys
import time
import urllib.request

APP = "dev.smix.fixture"
QUIET_ENOUGH_MS = 300
problems = []


def adb(device, *args):
    return subprocess.run(["adb", "-s", device, "shell", *args],
                          capture_output=True, text=True).stdout


def get(port, path):
    try:
        with urllib.request.urlopen(f"http://localhost:{port}{path}", timeout=10) as r:
            return r.read().decode("utf-8", "replace")
    except Exception:
        return ""


def probe(port):
    body = get(port, f"/probe?app={APP}")
    try:
        return json.loads(body)
    except json.JSONDecodeError:
        return {"present": False, "why": f"the runner answered {body[:60]!r}"}


def visible_rows(port):
    """Which lazy rows the accessibility tree currently carries."""
    return sorted(int(m) for m in re.findall(r"compose_row_(\d+)", get(port, "/tree")))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--device", required=True)
    ap.add_argument("--port", default="22095")
    a = ap.parse_args()
    d = a.device

    st = probe(a.port)
    if not st.get("present"):
        print("a-wait-that-does-not-end-early: CANNOT RUN — no probe answered "
              f"({st.get('why', 'no reason given')}). Without it there is only "
              "the polling strategy, and one strategy is not a comparison.")
        return 1

    # Force-stopped first, because the list keeps its scroll position
    # between runs. Left alone, successive runs walk it towards the last
    # row and then the swipe stops moving anything — and the gate would
    # red on a stimulus that failed rather than on the thing it asks
    # about. A gate that only works the first few times is one that will
    # go red for the wrong reason later.
    adb(d, "am", "force-stop", APP)
    adb(d, "am", "start", "-n", f"{APP}/.ComposeActivity")
    time.sleep(2)

    # Where the list actually is. Coordinates guessed from the screen's
    # middle missed it entirely once, and the scroll that never happened
    # was read as a signal that did not work.
    tree = get(a.port, "/tree")
    if "compose_rows" not in tree:
        problems.append("the fixture's lazy list is not on screen")
        return report()
    bounds = probe_bounds(d, "compose_rows")
    if bounds is None:
        problems.append("the probe does not report bounds for compose_rows")
        return report()
    l, t, r, b = bounds
    x, y_from, y_to = (l + r) // 2, b - 20, t + 20

    before = visible_rows(a.port)
    subprocess.Popen(["adb", "-s", d, "shell", "input", "swipe",
                      str(x), str(y_from), str(x), str(y_to), "900"])

    # Read both verdicts while it is moving.
    time.sleep(0.25)
    polling_says_ready = bool(visible_rows(a.port))
    quiet = probe(a.port).get("quietMs", -1)
    quiescence_says_ready = quiet >= QUIET_ENOUGH_MS

    time.sleep(3)
    after = visible_rows(a.port)
    settled_quiet = probe(a.port).get("quietMs", -1)

    # The stimulus, checked before the verdicts are believed.
    if before == after:
        problems.append(
            f"the list did not move ({before[:3]}… both times) — the swipe "
            f"missed, and neither verdict below was taken during motion"
        )
        return report()

    if not polling_says_ready:
        problems.append(
            "the polling strategy did NOT say ready during the scroll — the "
            "rows were absent from the tree, so this is not the situation "
            "this gate is about"
        )
    if quiescence_says_ready:
        problems.append(
            f"quiescence said ready mid-scroll (quietMs={quiet}) — it is "
            f"supposed to be the one that waits"
        )
    if settled_quiet < QUIET_ENOUGH_MS:
        problems.append(
            f"quiescence never caught up after the motion stopped "
            f"(quietMs={settled_quiet}) — a wait that never ends is worse "
            f"than one that ends early"
        )

    if problems:
        return report()
    print(f"a-wait-that-does-not-end-early: mid-scroll the tag was resolvable "
          f"(polling would have proceeded) while quietMs={quiet}; after the "
          f"motion stopped quietMs={settled_quiet}. Rows {before[0]}..{before[-1]} "
          f"-> {after[0]}..{after[-1]}.")
    return 0


def probe_bounds(device, tag):
    out = adb(device, "content", "call", "--uri",
              f"content://{APP}.smixprobe", "--method", "tree")
    m = re.search(r"tree=(\[.*\])\}\]", out, re.S)
    if not m:
        return None
    roots = json.loads(m.group(1))
    stack = list(roots)
    while stack:
        n = stack.pop()
        if n.get("testTag") == tag:
            return n["bounds"]
        stack.extend(n.get("children") or [])
    return None


def report():
    print("a-wait-that-does-not-end-early: FAIL")
    for p in problems:
        print(f"  - {p}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
