#!/usr/bin/env python3
"""Is an input-method window present, and should it be?

`before`: assert one IS there, so the assertion that follows is about
the case it claims to be about. A dismissal asserted over a screen with
no keyboard passes without touching the behaviour under test.

`after`: assert it is gone.

TYPE_INPUT_METHOD is 2. It is the IME's own window, and the same list
the runner's no-op decision reads.
"""

import json
import sys
from verdict_io import read_json

TYPE_INPUT_METHOD = 2


def main() -> int:
    when, path = sys.argv[1], sys.argv[2]
    # An expectation this does not recognise used to fall through to the
    # `after` branch, so a typo asserted the opposite of what was meant
    # and said nothing about it.
    if when not in ("before", "after"):
        print(f"A8: bad expectation {when!r} — want before|after", file=sys.stderr)
        return 1

    doc = read_json(path, "/windows")
    windows = doc.get("windows")
    if windows is None:
        print(
            "A8: /windows answered without a `windows` list at all, so there "
            "is nothing here to read the keyboard's presence out of.",
            file=sys.stderr,
        )
        return 1
    # The `after` half asked "is the IME gone?" and an empty list said
    # yes — so a /windows that came back with nothing, which is what a
    # dying runner returns, read as a successful dismissal. Absence is
    # only evidence when something else was present to see.
    if not windows:
        print(
            "A8: /windows listed no windows at all. `absent` is not a "
            "finding here: nothing was observed, and a runner that has "
            "gone away answers exactly like a keyboard that went.",
            file=sys.stderr,
        )
        return 1

    present = any(w.get("type") == TYPE_INPUT_METHOD for w in windows)
    packages = [w.get("package") for w in windows]

    if when == "before":
        if not present:
            print(
                "A8: no input-method window before the dismissal, so this "
                f"asserts nothing about dismissing one. Windows: {packages}",
                file=sys.stderr,
            )
            return 1
        return 0

    if present:
        print(
            "A8: hideKeyboard answered ok and the input-method window is "
            f"still there. Windows: {packages}",
            file=sys.stderr,
        )
        return 1
    print("  A8: with a keyboard up, hideKeyboard answered ok and the IME went")
    return 0


if __name__ == "__main__":
    sys.exit(main())
