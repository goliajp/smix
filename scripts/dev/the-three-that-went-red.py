#!/usr/bin/env python3
"""The three root causes that took a consumer's Android suite from 11 passing
to 20 red — each with something that goes red about it.

6.4.0 shipped a read-back predicate that misjudged every Compose screen. The
actions it called failures had all worked. Three causes, all one thing
underneath: the accessibility tree is a lossy, asynchronous projection of
what Compose knows.

  1. The read-back had no `refresh()`. A node handed over carries the value
     it had when it was handed over, and Compose commits asynchronously.
  2. `findFocus(FOCUS_INPUT)` is the wrong instrument. Compose keeps focus
     in its own semantics layer.
  3. `pressBack()`'s boolean says a key was injected, not that the keyboard
     left.

Each is asserted in two parts, and the first part is the one that matters:
the fixture must be SHOWN to produce the situation before any verdict about
it is read. A predicate about a condition the screen never reached excludes
the empty set and prints that it considered something.

`--without-probe` inverts the whole thing: with the probe gone all three
must go red. Two out of three is a failure, because the third one passing
would mean it never depended on the probe at all, and its green proves
nothing about what this version did.

Usage:
  the-three-that-went-red.py --device emulator-5554 --port 22095
  the-three-that-went-red.py --device emulator-5554 --port 22095 --without-probe
"""

import argparse
import json
import os
import subprocess
import sys
import time
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _probe_wire  # noqa: E402

APP = "dev.smix.fixture"
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
problems = []
shown = []


def adb(device, *args):
    return subprocess.run(["adb", "-s", device, "shell", *args],
                          capture_output=True, text=True).stdout


def smix(device, port, *args):
    return subprocess.run(
        [os.path.join(ROOT, "target/release/smix"), *args,
         "--device", device, "--port", str(port)],
        capture_output=True, text=True, cwd=ROOT)


def probe_node(device, tag):
    roots = _probe_wire.probe_tree(device, APP)
    if roots is None:
        return None
    stack = list(roots)
    while stack:
        n = stack.pop()
        if n.get("testTag") == tag:
            return n
        stack.extend(n.get("children") or [])
    return None


def a11y_tree(device, port):
    r = smix(device, port, "tree", "--json")
    body = "\n".join(l for l in r.stdout.splitlines() if not l.startswith("kevy:"))
    try:
        d = json.loads(body)
    except json.JSONDecodeError:
        return None, None
    return d.get("source"), d.get("root", d)


def a11y_node(root, ident):
    stack = [root] if root else []
    while stack:
        n = stack.pop()
        if n.get("identifier") == ident:
            return n
        stack.extend(n.get("children") or [])
    return None


# --- 1: a value read before the commit landed --------------------------

def cause_one(device, port, want_probe):
    """A masked field's real characters, which the projection cannot report."""
    typed = "r3d-one"
    smix(device, port, "fill", "id:compose_password", "--text", typed)
    time.sleep(1)
    n = probe_node(device, "compose_password")
    if not want_probe:
        return n is None or (n.get("inputText") or "") != typed
    # Shown: the field really holds it, and the projection really does not.
    if n is None or (n.get("inputText") or "") != typed:
        problems.append(
            f"1: the fixture did not take the text — nothing was measured "
            f"(inputText={None if n is None else n.get('inputText')!r})")
        return False
    if (n.get("editableText") or "") == typed:
        problems.append(
            "1: `editableText` carried the real characters, so this screen is "
            "not exhibiting a masked field and the assertion below is empty")
        return False
    shown.append(f"1: typed {typed!r}; editableText={n['editableText']!r}, "
                 f"inputText={n['inputText']!r}")
    return True


# --- 2: focus, asked of the wrong layer ---------------------------------

def cause_two(device, port, want_probe):
    """Compose keeps focus in semantics; the projection is not where it is."""
    smix(device, port, "tap", "id:compose_input")
    time.sleep(1)
    n = probe_node(device, "compose_input")
    if not want_probe:
        return n is None or not n.get("focused")
    if n is None or not n.get("focused"):
        problems.append(
            "2: the probe does not report the tapped field as focused — the "
            "tap did not land, so nothing below was measured")
        return False
    shown.append("2: the semantics layer reports compose_input focused")
    return True


# --- 3: the keyboard, asked of the key rather than the window -----------

def cause_three(device, port, want_probe):
    """`pressBack` returns whether a key went out, not whether it worked."""
    # Shown: a keyboard is actually up, or "it left" is about nothing.
    up = smix(device, port, "find", "role:keyboard").stdout
    if "exists=true" not in up:
        if not want_probe:
            return True
        problems.append(
            "3: no keyboard is on screen, so hiding it proves nothing")
        return False
    smix(device, port, "hide-keyboard")
    time.sleep(1)
    gone = smix(device, port, "find", "role:keyboard").stdout
    if "exists=false" not in gone:
        problems.append("3: the keyboard did not go away")
        return False
    if want_probe:
        shown.append("3: keyboard was up, hide-keyboard reported and it left")
    return True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--device", required=True)
    ap.add_argument("--port", default="22095")
    ap.add_argument("--without-probe", action="store_true")
    a = ap.parse_args()
    want = not a.without_probe

    adb(a.device, "am", "force-stop", APP)
    adb(a.device, "am", "start", "-n", f"{APP}/.ComposeActivity")
    time.sleep(2)

    present = _probe_wire.probe_tree(a.device, APP) is not None
    if want and not present:
        print("the-three-that-went-red: CANNOT RUN — the probe did not answer. "
              "Is `debugImplementation(\"jp.golia.smix:smix-probe\")` in the "
              "fixture's build?")
        return 2
    if not want and present:
        print("the-three-that-went-red: CANNOT RUN — --without-probe was asked "
              "for and the probe IS there. Remove it from the fixture's debug "
              "build first, or this measures nothing.")
        return 2

    ok = [cause_one(a.device, a.port, want),
          cause_two(a.device, a.port, want),
          cause_three(a.device, a.port, want)]

    if want:
        if problems:
            print("the-three-that-went-red: FAIL")
            for p in problems:
                print(f"  - {p}")
            return 1
        if not all(ok):
            print("the-three-that-went-red: FAIL")
            print(f"  - {sum(1 for x in ok if not x)} of the three did not hold")
            return 1
        for s in shown:
            print(f"the-three-that-went-red: {s}")
        print("the-three-that-went-red: all three hold, each on a situation "
              "the fixture was shown to produce")
        return 0

    # Inverted. Exactly three, not "at least one": a cause that stays green
    # without the probe never depended on it, and its green says nothing
    # about what this version did.
    red = sum(1 for x in ok if x)
    if red != 3:
        print("the-three-that-went-red: FAIL")
        print(f"  - without the probe only {red} of the three went red. The "
              f"{3 - red} that did not never depended on it, so their green "
              f"proves nothing about this version.")
        return 1
    print("the-three-that-went-red: without the probe all three go red, so all "
          "three are answers this version added rather than ones already there")
    return 0


if __name__ == "__main__":
    sys.exit(main())
