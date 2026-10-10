#!/usr/bin/env python3
"""Does this tree carry the software keyboard, or not?

`role:keyboard` answers on iOS — Apple types the software keyboard 19 and
the tree has always carried it, so `extendedWaitUntil { visible: { role:
keyboard } }` works there. On Android the same flow timed out with
ELEMENT_NOT_FOUND while the keyboard was unmistakably on screen: a
screenshot showed the QWERTY and `/windows` listed
`com.android.inputmethod.latin`. The runner knew — `keyboardIsUp()` reads
that window type and `hide-keyboard` has decided on it for releases — and
nothing lifted it into the tree, which is the one place every verb can
reach.

A consumer read the gap as "there is no way to wait for the keyboard" and
reached for a pause instead. Half of that was right, and the half that
was wrong is why this is asked on both platforms now.

Usage:  android-a12-verdict.py <tree.json> present|absent [windows.json]

With `windows.json` (the runner's /windows, read after the tree), a
missing keyboard is told apart from a keyboard the device never showed:
the emulator's IME has stalled in the window manager and not come up at
all (2026-10-11), and the old message said the window was attached
without having looked.
Exit:   0 with the finding on stdout; 1 with the reason on stderr.
"""

import json
import sys
from verdict_io import read_json


def has_keyboard(node) -> bool:
    if node.get("role") == "keyboard":
        return True
    return any(has_keyboard(c) for c in node.get("children", []) or [])


def main() -> int:
    tree = read_json(sys.argv[1], "the tree")
    want = sys.argv[2]
    if want not in ("present", "absent"):
        print(f"A12: bad expectation {want!r} — want present|absent", file=sys.stderr)
        return 1

    root = tree.get("tree") or tree
    found = has_keyboard(root)

    # A tree with nothing in it agrees with "absent" for the wrong
    # reason, and this assertion is asked once with each expectation, so
    # an empty answer would pass half of it by knowing nothing.
    if not (root.get("children") or []):
        print(
            "A12: /tree came back with no children at all — nothing here "
            "proves the keyboard present or absent",
            file=sys.stderr,
        )
        return 1

    if want == "present" and not found and len(sys.argv) > 3:
        windows = read_json(sys.argv[3], "the windows").get("windows") or []
        if not windows:
            print("A12: /windows came back empty — nothing says whether the keyboard is up",
                  file=sys.stderr)
            return 1
        if not any(w.get("type") == 2 for w in windows):
            listed = ", ".join(f"{w.get('package')}(type {w.get('type')})" for w in windows)
            print(
                f"A12: the keyboard never came up — /windows lists no input-method "
                f"window ({listed}). The device did not show it, so this cannot "
                f"judge whether the tree carries it; the device's logcat "
                f"(ImeTracker) says why it did not.",
                file=sys.stderr,
            )
            return 1
    if want == "present" and not found:
        print(
            "A12: no node in the tree has role=keyboard while the keyboard "
            "is up. The input-method window is attached — /windows lists "
            "it — so this is the tree not carrying what the runner already "
            "knows, and `role:keyboard` cannot answer on this platform."
            if len(sys.argv) > 3 else
            "A12: no node in the tree has role=keyboard (no /windows was "
            "given, so whether the keyboard was up is not known here).",
            file=sys.stderr,
        )
        return 1
    if want == "absent" and found:
        print(
            "A12: a node claims role=keyboard with no keyboard on screen. "
            "An assertion that the keyboard is gone would pass forever.",
            file=sys.stderr,
        )
        return 1

    print(f"  A12{'a' if want == 'present' else 'b'}: keyboard {want} in the tree")
    return 0


if __name__ == "__main__":
    sys.exit(main())
