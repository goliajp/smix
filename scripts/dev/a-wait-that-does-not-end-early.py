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
  a-wait-that-does-not-end-early.py --device <serial> --port <runner port>

Exit 0 judged and passed, 1 judged and failed, 2 could not judge (three
scrolls and no reading taken in motion). SMIX_WAIT_TRACE=1 prints every
read of the rows and every reading of the probe to stderr.
"""

import argparse
import json
import re
import subprocess
import os
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _probe_wire  # noqa: E402
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


def probe_rows(port):
    """Where the lazy rows are, from the probe's tree: (row, top) pairs."""
    body = get(port, f"/probe/tree?app={APP}")
    return sorted((int(m.group(1)), int(m.group(2))) for m in re.finditer(
        r'"testTag":"compose_row_(\d+)"[^{}]*?"visibleBounds":\[\d+,(-?\d+)', body))


def visible_rows(port):
    """Which lazy rows the accessibility tree currently carries."""
    return sorted(int(m) for m in re.findall(r"compose_row_(\d+)", get(port, "/tree")))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--device", required=True)
    ap.add_argument("--port", required=True, help="the port of the runner the caller brought up")
    a = ap.parse_args()
    d = a.device

    st = probe(a.port)
    if not st.get("present"):
        print("a-wait-that-does-not-end-early: CANNOT RUN — no probe answered "
              f"({st.get('why', 'no reason given')}). Without it there is only "
              "the polling strategy, and one strategy is not a comparison.")
        return 1

    # A run that took no reading in motion measured nothing, and is taken
    # again; a verdict is never retaken. On an emulator whose guest CPUs
    # are held by busy loops one read of the probe's tree stalled for
    # 1.8 s and the whole drag happened inside it.
    for attempt in range(1, ATTEMPTS + 1):
        problems.clear()
        rc = once(a, d)
        if rc is not UNMEASURED:
            return rc
    print(f"a-wait-that-does-not-end-early: CANNOT JUDGE — {ATTEMPTS} scrolls and "
          f"no reading was taken in motion; the longest gap between two reads "
          f"of the rows was {unmeasured_gap[0]:.2f}s, so this side could not "
          f"see the list moving closely enough to say when it was still")
    return 2


ATTEMPTS = 3
UNMEASURED = object()
unmeasured_gap = [0.0]


def once(a, d):
    # Force-stopped first, because the list keeps its scroll position
    # between runs. Left alone, successive runs walk it towards the last
    # row and then the swipe stops moving anything — and the gate would
    # red on a stimulus that failed rather than on the thing it asks
    # about. A gate that only works the first few times is one that will
    # go red for the wrong reason later.
    adb(d, "am", "force-stop", APP)
    adb(d, "am", "start", "-n", f"{APP}/.ComposeActivity")
    # Not a fixed wait: see `wait_for_probe`. Two seconds was the
    # boundary this screen actually arrives on, and under load it fell
    # on the wrong side and the verdict blamed the feature.
    _probe_wire.wait_for_probe(d, APP)

    # Where the list actually is. Coordinates guessed from the screen's
    # middle missed it entirely once, and the scroll that never happened
    # was read as a signal that did not work.
    #
    # And waiting for the probe to answer is not waiting for this. The
    # probe lives in the app's process and answers as soon as that
    # process is up; whether the list has been composed into the
    # accessibility tree is a later moment. Measured 2026-08-31: the
    # gate read the tree, found no `compose_rows`, and said "the
    # fixture's lazy list is not on screen" -- true at that instant, and
    # about a screen that had not arrived rather than about waiting
    # ending early, which is what this gate asks. Same shape as the
    # `am start` waits already taken out of the behaviour gate.
    tree = ""
    for _ in range(30):
        tree = get(a.port, "/tree")
        if "compose_rows" in tree:
            break
        time.sleep(1)
    if "compose_rows" not in tree:
        problems.append(
            "the fixture's lazy list never reached the tree within 30s — this is "
            "about the screen arriving, not about a wait ending early"
        )
        return report()
    bounds = probe_bounds(d, "compose_rows")
    if bounds is None:
        problems.append("the probe does not report bounds for compose_rows")
        return report()
    l, t, r, b = bounds
    x, y_from, y_to = (l + r) // 2, b - 20, t + 20

    before = visible_rows(a.port)
    subprocess.Popen(["adb", "-s", d, "shell", "input", "swipe",
                      str(x), str(y_from), str(x), str(y_to), "2000"])
    # Read both verdicts while it is moving — and prove it was moving at
    # that moment. A fixed sleep after starting `adb shell input swipe`
    # does not: the shell takes its own time to start, seconds on a loaded
    # machine, and a reading taken before the list moved says nothing about
    # a wait during motion. Until 14.1 that reading was always -1 (the
    # probe's sampler had not started), so this gate passed without having
    # measured anything. A reading counts only when the rows were seen changing
    # shortly before it and shortly after it (see BOUND_S below).
    #
    # Which rows are where is read from the probe's tree, which answers in
    # milliseconds. The accessibility tree took up to four seconds a read
    # during the scroll, longer than the scroll, and a release dry-run went
    # red with no reading inside the motion at all.
    #
    # And only once the list has been seen moving for longer than the
    # probe's sampler takes to notice: it looks every 50 ms, so the first
    # reading after the list starts still describes the stillness before
    # it. Measured: that first reading was 2.3-3.1 s in four runs of five,
    # every one after it under 100 ms.
    SAMPLER_CATCHES_UP_S = 0.2
    # A reading counts as taken in motion only if this side saw the rows
    # change wholly before it, starting less than a bound before it: the
    # screen then cannot have been still for longer than the bound, and a
    # stillness reported at or over the threshold is wrong. Judged after
    # the run, from every
    # read, rather than from the last two: unloaded, reads come every 10 ms,
    # faster than frames, so every other one sees no change — and a rule
    # that reset on an unchanged read found no motion at all. Loaded, an
    # injected drag stalls for over a second between two reads that
    # differ, and the probe is right to call that still. The margin under
    # the threshold is one sampler tick plus how late it may run.
    BOUND_S = (QUIET_ENOUGH_MS - 100) / 1000
    readings = []  # (asked, quietMs, (sampleLateMs, framesPending))
    changes = []   # (after the read before, after the read that differed)
    deadline = time.time() + 6
    rows = probe_rows(a.port)
    rows_at = time.time()
    polled = None
    while time.time() < deadline:
        asked = time.time()
        said = probe(a.port)
        readings.append((asked, said.get("quietMs", -1),
                         (said.get("sampleLateMs"), said.get("framesPending"))))
        now = probe_rows(a.port)
        now_at = time.time()
        if now != rows:
            changes.append((rows_at, now_at))
            if polled is None and now_at - changes[0][0] >= SAMPLER_CATCHES_UP_S:
                # The other strategy, asked once while the list is seen
                # moving: does the tag resolve in the tree it polls? Asked
                # beside the readings, not between them — the accessibility
                # tree took 1.8 s a read with the guest's CPUs held, and the
                # whole drag went by while this side waited for it.
                polled = []
                asker = threading.Thread(target=lambda: polled.append(bool(visible_rows(a.port))))
                asker.start()
        rows, rows_at = now, now_at
    began = changes[0][0] if changes else None
    if os.environ.get("SMIX_WAIT_TRACE"):
        t0 = readings[0][0] if readings else 0
        print("changes:", [(round(s - t0, 3), round(e - t0, 3)) for s, e in changes], file=sys.stderr)
        print("readings:", [(round(r[0] - t0, 3), r[1]) for r in readings], file=sys.stderr)
    moving = []
    lates_moving = []
    for asked, q, extra in readings:
        moved_before = any(s > asked - BOUND_S and e <= asked for s, e in changes)
        if moved_before and asked - began >= SAMPLER_CATCHES_UP_S:
            moving.append(q)
            lates_moving.append(extra)
    if polled is not None:
        asker.join()
    polling_says_ready = bool(polled) and polled[0]
    after = visible_rows(a.port)
    settled = probe(a.port)
    settled_quiet = settled.get("quietMs", -1)
    settled_pending = settled.get("framesPending")
    quiet = max(moving) if moving else None
    # The stimulus, checked before the verdicts are believed.
    if before == after:
        problems.append(
            f"the list did not move ({before[:3]}… both times) — the swipe "
            f"missed, and neither verdict below was taken during motion"
        )
        return report()
    if not moving:
        unmeasured_gap[0] = max((e - s for s, e in changes), default=0.0)
        return UNMEASURED
    if not polling_says_ready:
        problems.append(
            "the polling strategy did NOT say ready during the scroll — the "
            "rows were absent from the tree, so this is not the situation "
            "this gate is about"
        )
    if quiet >= QUIET_ENOUGH_MS:
        problems.append(
            f"quiescence said ready mid-scroll (quietMs={quiet}, readings taken "
            f"while the rows changed: {moving}; (sampler lateness ms, "
            f"frames pending) at each: {lates_moving}) — it is supposed to be the one that waits"
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
          f"(polling would have proceeded) while quietMs={quiet} ((sampler lateness ms, "
          f"frames pending) at those readings: {lates_moving}); after the "
          f"motion stopped quietMs={settled_quiet}. Rows {before[0]}..{before[-1]} "
          f"-> {after[0]}..{after[-1]}.")
    return 0


def probe_bounds(device, tag):
    roots = _probe_wire.probe_tree(device, APP)
    if roots is None:
        return None
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
