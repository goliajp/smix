#!/usr/bin/env python3
"""Can A4 say what it found, in every shape it can find it?

The case this exists for is the one that happened: on 2026-08-24 the
7.0.0 ship reached A4, the fixture's window was attached with an
unreadable root, and the verdict died with

    TypeError: '<' not supported between instances of 'NoneType' and 'str'

A red that arrives as a stack trace is not a verdict — it says the gate
broke, not what the gate found, and the two want opposite responses. So
every branch here is checked for the sentence it prints as well as the
code it returns, and the payload that crashed is a fixture below, byte
for byte as `/windows` returned it.

The `unreadable_window_is_not_reported_as_simply_absent` case is the
second half of the same defect: readability was only ever asked of
windows already matched by package, and an unreadable root is precisely
what leaves a window with no package to match — so the branch written to
tell the two failures apart could never be reached.
"""

import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
VERDICT = os.path.join(
    os.path.dirname(HERE), "release", "android-a4-verdict.py"
)
APP = "dev.smix.fixture"


def run(windows):
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump({"windows": windows}, f)
        path = f.name
    try:
        done = subprocess.run(
            [sys.executable, VERDICT, path, APP], capture_output=True, text=True
        )
        return done.returncode, done.stdout + done.stderr
    finally:
        os.unlink(path)


# Exactly what `/windows` answered when the ship stopped: two SystemUI
# windows, the IME, and one application window whose root could not be
# read — and which therefore has no package at all.
THE_PAYLOAD_THAT_CRASHED = [
    {"package": "com.android.systemui", "rootReadable": True, "type": 3},
    {"package": "com.android.systemui", "rootReadable": True, "type": 3},
    {"package": "com.android.inputmethod.latin", "rootReadable": True, "type": 2},
    {"package": None, "rootReadable": False, "type": 1},
]

CASES = [
    (
        "the payload that crashed the ship",
        THE_PAYLOAD_THAT_CRASHED,
        1,
        "root unreadable",
    ),
    (
        "a readable window of ours",
        [
            {"package": "com.android.systemui", "rootReadable": True, "type": 3},
            {"package": APP, "rootReadable": True, "type": 1},
        ],
        0,
        "A4a",
    ),
    (
        "ours is attached and unreadable, and says so by name",
        [{"package": APP, "rootReadable": False, "type": 1}],
        1,
        "absent from the tree while present on screen",
    ),
    (
        "ours is simply not there",
        [{"package": "com.android.systemui", "rootReadable": True, "type": 3}],
        1,
        "not attached for accessibility",
    ),
    (
        "no windows at all is not a pass",
        [],
        1,
        "nothing here proves anything",
    ),
]


def main() -> int:
    ok = True
    for label, windows, want_code, want_text in CASES:
        code, out = run(windows)
        if "Traceback" in out:
            print(f"FAIL [{label}]: the verdict crashed instead of judging:\n{out}")
            ok = False
            continue
        if code != want_code:
            print(f"FAIL [{label}]: exit {code}, wanted {want_code}\n{out}")
            ok = False
            continue
        if want_text not in out:
            print(f"FAIL [{label}]: right code, wrong sentence — wanted "
                  f"{want_text!r} in:\n{out}")
            ok = False
            continue
        print(f"ok   [{label}]")

    print("android-a4 verdict self-test: " + ("PASS" if ok else "FAIL"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
