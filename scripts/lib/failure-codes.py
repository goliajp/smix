#!/usr/bin/env python3
"""Which failure codes are a verdict about the screen, read from smix-error.

`FailureCode::judges_the_screen` is the one place that says which codes
judge the screen (the element is not there, the touch missed) and which
say smix could not look (the runner answered garbage, the app is gone).
The flow runner's `optional:` and the e2e scripts ask the same question;
this reads the answer out of the source instead of keeping a second list
beside it.

    failure-codes.py verdicts     one wire name per line, e.g. ELEMENT_NOT_FOUND
    failure-codes.py others       the codes that are not verdicts

Exits non-zero when the source does not parse into two non-empty sides
that together cover every variant of the enum: an instrument that read
nothing must not answer "no code is a verdict".
"""

import re
import sys
from pathlib import Path

SOURCE = Path(__file__).resolve().parents[2] / "crates/smix-error/src/lib.rs"


def wire_name(variant: str) -> str:
    return re.sub(r"(?<!^)(?=[A-Z])", "_", variant).upper()


def enum_variants(src: str) -> list[str]:
    m = re.search(r"pub enum FailureCode \{(.*?)\n\}", src, re.S)
    if not m:
        raise SystemExit("failure-codes: no `pub enum FailureCode` in " + str(SOURCE))
    body = "\n".join(
        line for line in m.group(1).splitlines() if not line.strip().startswith("//")
    )
    return re.findall(r"^\s*([A-Z][A-Za-z]+),", body, re.M)


def sides(src: str) -> tuple[list[str], list[str]]:
    m = re.search(r"pub fn judges_the_screen\(self\) -> bool \{(.*?)\n    \}", src, re.S)
    if not m:
        raise SystemExit("failure-codes: no `judges_the_screen` in " + str(SOURCE))
    arms = re.findall(r"((?:\|?\s*FailureCode::\w+\s*)+)=>\s*(true|false)", m.group(1))
    verdicts, others = [], []
    for names, value in arms:
        (verdicts if value == "true" else others).extend(re.findall(r"FailureCode::(\w+)", names))
    return verdicts, others


def main() -> int:
    if len(sys.argv) != 2 or sys.argv[1] not in ("verdicts", "others"):
        print(__doc__, file=sys.stderr)
        return 2
    src = SOURCE.read_text()
    verdicts, others = sides(src)
    variants = enum_variants(src)
    if not verdicts or not others:
        print(f"failure-codes: one side is empty (verdicts={verdicts}, others={others})", file=sys.stderr)
        return 1
    if sorted(verdicts + others) != sorted(variants):
        print(
            f"failure-codes: the two sides {sorted(verdicts + others)} are not the enum's "
            f"variants {sorted(variants)}",
            file=sys.stderr,
        )
        return 1
    for v in verdicts if sys.argv[1] == "verdicts" else others:
        print(wire_name(v))
    return 0


if __name__ == "__main__":
    sys.exit(main())
