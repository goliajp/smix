#!/usr/bin/env python3
"""The probe stages the screen; the touch stays real.

A Compose semantics action calls the composable's own lambda. Nothing in
that path does hit-testing, so it fires on a node that nothing on screen
could reach. Measured on the fixture with a dialog's scrim over
`compose_submit`: a real touch at that node's screen coordinates left the
app unchanged — correctly blocked — while semantics `OnClick` returned true
and the submit went through.

Offering that as smix's tap would manufacture passes for taps a user could
not make. So the probe refuses it, and this gate holds both halves:

  - the refusal, on the offered surface
  - the thing it refuses, still doing what it does

The second half is the point. A rule whose subject has never been observed
is a rule nobody has checked, and this one guards against a capability that
would look like an improvement right up until a release went out on it.

Usage:
  a-semantics-action-is-not-a-touch.py --device emulator-5554 --port 22095
"""

import argparse
import json
import re
import subprocess
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _probe_wire  # noqa: E402

APP = "dev.smix.fixture"
problems = []


def adb(device, *args):
    r = subprocess.run(["adb", "-s", device, "shell", *args],
                       capture_output=True, text=True)
    return r.stdout


def probe(device, method, arg=None, extra=None):
    cmd = ["content", "call", "--uri", f"content://{APP}.smixprobe",
           "--method", method]
    if arg:
        cmd += ["--arg", arg]
    if extra:
        # `content call` wants a typed binding, `key:type:value`. Passing
        # the pair untyped made adb print its usage, and the gate read that
        # as "the action did not fire" — a measurement device failing in a
        # way that looks exactly like data.
        for k, v in extra.items():
            cmd += ["--extra", f"{k}:s:{v}"]
    return adb(device, *cmd)


def tree(device):
    return _probe_wire.probe_tree(device, APP)


def find(roots, tag):
    stack = list(roots or [])
    while stack:
        n = stack.pop()
        if n.get("testTag") == tag:
            return n
        stack.extend(n.get("children") or [])
    return None


def result_text(device):
    n = find(tree(device), "compose_result")
    return None if n is None else n.get("text")


def read_result_or_wait(device, tries=6):
    """The result node's text, or a complaint that it could not be read.

    `None` came back once here and the comparison below happily passed on
    it: "the touch changed nothing" and "the tree could not be read" are
    the same value. A measurement device failing looks exactly like data,
    so absence is a failure of this gate rather than an answer from it.
    """
    for _ in range(tries):
        t = result_text(device)
        if t is not None:
            return t
        time.sleep(0.5)
    problems.append(
        "`compose_result` was not in the tree — the probe answered but the "
        "screen it described has no result node, so nothing below was "
        "measured against anything"
    )
    return None


def smix(binary, device, port, *args):
    subprocess.run([binary, *args, "--device", device, "--port", str(port)],
                   capture_output=True, text=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--device", required=True)
    ap.add_argument("--port", default="22095")
    ap.add_argument("--binary", default="./target/release/smix")
    a = ap.parse_args()
    d = a.device

    adb(d, "am", "start", "-n", f"{APP}/.ComposeActivity")
    time.sleep(2)
    if tree(d) is None:
        print("a-semantics-action-is-not-a-touch: CANNOT RUN — the probe did "
              "not answer. Is it in the fixture's debug build?")
        return 2

    marker = "staged-for-the-gate"
    smix(a.binary, d, a.port, "fill", "id:compose_input", "--text", marker)
    time.sleep(1)
    before = result_text(d)
    if before == marker:
        problems.append(
            "the result already reads the marker before anything was "
            "submitted — the screen is not in its starting state"
        )

    # Put a scrim between the world and the button.
    smix(a.binary, d, a.port, "tap", "id:compose_open_dialog")
    time.sleep(2)
    node = find(tree(d), "compose_submit")
    if node is None:
        problems.append("compose_submit is not in the tree with the dialog open")
        return report()
    l, t, r, b = node["bounds"]
    adb(d, "input", "tap", str((l + r) // 2), str((t + b) // 2))
    time.sleep(2)
    after_touch = read_result_or_wait(d)
    # The presence half: if a real touch DID land, the scrim is not covering
    # anything and the rest of this gate is asking about nothing.
    if after_touch == marker:
        problems.append(
            "a real touch reached the button under the dialog — the fixture "
            "is not exhibiting an unreachable node, so the refusal below is "
            "being checked against nothing"
        )

    # And the thing that is refused, doing it anyway.
    out = probe(d, "act-unsafe-for-gates", "compose_submit",
                {"action": "OnClick"})
    time.sleep(2)
    after_semantics = read_result_or_wait(d)
    if after_semantics != marker:
        problems.append(
            f"semantics OnClick did not fire on the covered node "
            f"(result={after_semantics!r}, probe said {out.strip()[:80]!r}). "
            f"If Compose started hit-testing its actions, this gate's whole "
            f"subject has changed and the refusal may no longer be needed"
        )

    # The refusal itself, on the surface a caller reaches.
    refused = probe(d, "act", "compose_submit", {"action": "OnClick"})
    if "refuses" not in refused:
        problems.append(
            f"the offered surface did not refuse OnClick: {refused.strip()[:120]}"
        )
    if "tap" not in refused:
        problems.append(
            "the refusal does not say what to use instead — a refusal a "
            "caller cannot act on is a dead end"
        )

    adb(d, "input", "keyevent", "KEYCODE_BACK")
    return report(after_touch, after_semantics)


def report(after_touch=None, after_semantics=None):
    if problems:
        print("a-semantics-action-is-not-a-touch: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(
        f"a-semantics-action-is-not-a-touch: a real touch on the covered node "
        f"left {after_touch!r}; semantics OnClick made it {after_semantics!r}; "
        f"the offered surface refuses it and names the alternative"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
