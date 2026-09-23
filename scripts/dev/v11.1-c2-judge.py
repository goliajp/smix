#!/usr/bin/env python3
"""Judge a failure's text for C2: does it say whose screen it happened on?

Shared by the Android and the iOS e2e so both platforms are held to the same
sentence. Reads the failure text on stdin.

    v11.1-c2-judge.py <leg> <package> [--no-system-first <raw /tree json file>]

Checks, each printed as a verdict line:
  on-screen=yes     the failure names <package> as the focused application
  of-n=yes (a of b) the element list says how many it was cut from
  system-first=0    (with --no-system-first) none of the first ten listed
                    elements sits only in a window some other package owns —
                    the consumer's symptom was that all ten did

Whose an element is comes from the runner's raw /tree read right after the
failure, by the window each id sits under. Not from the id's spelling: the
first version of this judge looked for `systemui` / `status_bar` in the
listed ids, and the ids are short resource names — it passed a list of ten
navigation-bar elements (`navigation_bar_frame`, `back`, `home`) as clean.

Exit 0 when every check holds, 1 when one does not. The text it was given is
printed on failure, because a verdict without its evidence cannot be read.
"""

import re
import sys


def ids_by_window(path):
    """{short id: {package of each window it appears under}}, or None when
    the tree carries no window identity at all."""
    import json

    with open(path, encoding="utf-8") as fh:
        root = json.load(fh)
    owners = {}
    seen_window = False

    def walk(n, pkg):
        ident = n.get("identifier")
        if ident and pkg:
            owners.setdefault(ident.split("/")[-1], set()).add(pkg)
        for c in n.get("children", []):
            walk(c, pkg)

    for w in root.get("children", []):
        info = w.get("window")
        if info:
            seen_window = True
        walk(w, (info or {}).get("package"))
    return owners if seen_window else None


def main() -> int:
    if len(sys.argv) < 3:
        print("usage: v11.1-c2-judge.py <leg> <package> [--no-system-first]", file=sys.stderr)
        return 2
    leg, package = sys.argv[1], sys.argv[2]
    rest = sys.argv[3:]
    tree_file = None
    if "--no-system-first" in rest:
        i = rest.index("--no-system-first")
        if i + 1 >= len(rest):
            print("--no-system-first needs the raw /tree json file", file=sys.stderr)
            return 2
        tree_file = rest[i + 1]
    no_system_first = tree_file is not None
    text = sys.stdin.read()
    problems = []

    if f"on screen: {package} (application, focused)" in text:
        print(f"[{leg}]   on-screen=yes ({package})")
    else:
        problems.append(f"no line naming {package} as the focused application")

    m = re.search(r"visible elements \((\d+) of (\d+)", text)
    if m:
        print(f"[{leg}]   of-n=yes ({m.group(1)} of {m.group(2)})")
    else:
        problems.append("the element list does not say how many it was cut from")

    if no_system_first:
        lines = text.splitlines()
        start = next((i for i, l in enumerate(lines) if "visible elements (" in l), None)
        if start is None:
            problems.append("no element list at all")
        else:
            listed = [l for l in lines[start + 1 : start + 11] if l.startswith("    - ")]
            owners = ids_by_window(tree_file)
            if not listed:
                problems.append("the element list is empty")
            elif owners is None:
                problems.append(
                    "the runner's tree does not say which window is whose, so whose the "
                    "listed elements are cannot be told"
                )
            else:
                foreign = []
                for l in listed:
                    m2 = re.search(r'id="([^"]+)"', l)
                    if not m2:
                        continue
                    pkgs = owners.get(m2.group(1), set())
                    if pkgs and package not in pkgs:
                        foreign.append(f"{m2.group(1)} ({', '.join(sorted(pkgs))})")
                if foreign:
                    problems.append(
                        f"{len(foreign)} of the first ten sit only in other packages' windows: {foreign[:4]}"
                    )
                else:
                    print(f"[{leg}]   system-first=0 (of {len(listed)} listed)")

    if problems:
        for p in problems:
            print(f"[{leg}] FAIL: {p}")
        print(f"[{leg}] the failure text was:\n{text}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
