#!/usr/bin/env python3
"""A wait that polls ends by when its last look began, not by the clock now.

Checking the clock before a look skips the look that would have seen the
change; checking it after lets a look that began in time and came back
late end the wait on a state that had already changed. On a loaded device
one look has taken seconds, and both spellings failed waits whose subject
had arrived. The wait is over only once a look that began after the
deadline still says "not yet" — `smix_adb::wait::Deadline` in synchronous
code, `smix_driver::poll::Budget` in the driver, a `began` taken before the
look where neither is in reach.

So no source compares the current time with a deadline.
"""

from __future__ import annotations

import os
import re
import sys

ROOT = os.environ.get("SMIX_GATE_ROOT") or os.path.dirname(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
)

NOW_VS_DEADLINE = re.compile(
    r"Instant::now\(\)\s*(?:<=?|>=?)\s*\w*deadline\b"
    r"|\b\w*deadline\s*(?:<=?|>=?)\s*(?:std::time::|tokio::time::)?Instant::now\(\)"
)


def main() -> int:
    crates = os.path.join(ROOT, "crates")
    read = 0
    problems = []
    for base, dirs, files in os.walk(crates):
        dirs[:] = [d for d in dirs if d not in ("target",)]
        for name in files:
            if not name.endswith(".rs"):
                continue
            path = os.path.join(base, name)
            read += 1
            with open(path, encoding="utf-8") as fh:
                for n, line in enumerate(fh, 1):
                    code = line.split("//", 1)[0]
                    if NOW_VS_DEADLINE.search(code):
                        rel = os.path.relpath(path, ROOT)
                        problems.append(f"{rel}:{n}: {line.strip()}")
    if read == 0:
        print("a-wait-ends-by-when-it-looked: FAIL")
        print(f"  - no Rust source under {crates} — this read nothing, which is not the same as finding nothing")
        return 1
    if problems:
        print("a-wait-ends-by-when-it-looked: FAIL")
        for p in problems:
            print(f"  - {p} — compares the clock now with the deadline; judge by when the look began")
        return 1
    print(f"a-wait-ends-by-when-it-looked: clean — {read} Rust files, none compares the clock now with a deadline")
    return 0


if __name__ == "__main__":
    sys.exit(main())
