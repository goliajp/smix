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

TYPE_INPUT_METHOD = 2


def main() -> int:
    when, path = sys.argv[1], sys.argv[2]
    windows = json.load(open(path))["windows"]
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
