#!/usr/bin/env python3
"""Every mention of Simulator.app says which Xcode it is talking about.

Xcode 27 removed Simulator.app; the simulator UI there is Device Hub.
On Xcode 26 and earlier, Simulator.app is still what pops a window for
every boot. A sentence that says "Simulator.app" and nothing else is
true on one generation and false on the other, and nothing about it
looks wrong on either — which is how the capsule guard came to watch
for a process that no longer existed while every test around it stayed
green.

So: in the outward docs, the plugin, the README and the CLI's own source,
a line that names Simulator.app must also name the generation on that
line (`Xcode <= 26`, `Xcode 26`, `Xcode 27`, `on 26`). And the guard's
source must mention Device Hub at all — the other half of "which".

Two things this refuses to do (§14.7): pass over an empty set (zero
mentions means the scan read the wrong tree, not that every mention is
qualified), and pass over a capsule that never heard of Device Hub.

Usage:  an-app-that-is-gone.py [repo-root]
Exit:   0 clean · 1 findings · 2 cannot run
"""

import os
import re
import sys

ROOT = os.path.abspath(
    sys.argv[1]
    if len(sys.argv) > 1
    else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
)

SCANNED = ["docs", "plugin", "README.md", os.path.join("crates", "smix-cli", "src")]
GUARD = os.path.join("crates", "smix-cli", "src", "capsule.rs")

MENTION = re.compile(r"Simulator\.app")
QUALIFIED = re.compile(r"Xcode\s*(<=|≤)?\s*2[67]\b|\bon 26\b")
TEXT_SUFFIXES = (".md", ".rs", ".txt", ".yaml", ".yml", ".sh", ".py", ".ts", ".json")


def files_under(path: str):
    if os.path.isfile(path):
        yield path
        return
    for dirpath, dirnames, filenames in os.walk(path):
        dirnames[:] = [d for d in dirnames if d not in ("node_modules", "target", ".build")]
        for name in filenames:
            if name.endswith(TEXT_SUFFIXES):
                yield os.path.join(dirpath, name)


def main() -> int:
    missing = [p for p in SCANNED + [GUARD] if not os.path.exists(os.path.join(ROOT, p))]
    if missing:
        print("an-app-that-is-gone: CANNOT RUN")
        for p in missing:
            print(f"  - {p} is not in this tree")
        return 2

    mentions = 0
    problems: list[str] = []
    for rel in SCANNED:
        for path in files_under(os.path.join(ROOT, rel)):
            with open(path, encoding="utf-8", errors="replace") as fh:
                for lineno, line in enumerate(fh, 1):
                    if not MENTION.search(line):
                        continue
                    mentions += 1
                    if not QUALIFIED.search(line):
                        problems.append(
                            f"{os.path.relpath(path, ROOT)}:{lineno}: names Simulator.app "
                            f"without saying which Xcode — true on 26, false on 27, and "
                            f"the line cannot tell the reader which it is"
                        )

    if mentions == 0:
        problems.append(
            "no mention of Simulator.app anywhere in the scanned tree — this scan "
            "read the wrong tree; passing on an empty set is not passing"
        )

    with open(os.path.join(ROOT, GUARD), encoding="utf-8") as fh:
        if "DeviceHub" not in fh.read():
            problems.append(
                f"{GUARD} never mentions DeviceHub — the guard knows one generation "
                f"of the simulator UI and not the one that replaced it"
            )

    if problems:
        print("an-app-that-is-gone: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(
        f"an-app-that-is-gone: clean — {mentions} mention(s) of Simulator.app, each "
        f"saying which Xcode; the guard knows Device Hub"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
