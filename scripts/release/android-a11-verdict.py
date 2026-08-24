#!/usr/bin/env python3
"""A field named by the layout around it takes the text, and only it.

Two assertions, because either alone passes for the wrong reason: a
fill that reached no field at all leaves the other one untouched, and a
fill that reached the right field could still have gone to the one that
held focus a moment earlier — which is what the first attempt at this
fix did, measured.
"""

import json
import sys
from verdict_io import read_json


def main() -> int:
    tree, expected = read_json(sys.argv[1], "the tree"), sys.argv[2]
    fields: list[dict] = []

    def walk(node: dict) -> None:
        if (node.get("rawType") or "").endswith("EditText"):
            fields.append(node)
        for child in node.get("children", []):
            walk(child)

    walk(tree)

    if len(fields) < 2:
        print(
            f"A11: found {len(fields)} text field(s) and this needs two — one named "
            f"from the outside and one to prove the fill did not go there instead",
            file=sys.stderr,
        )
        return 1

    fields.sort(key=lambda n: n["bounds"]["y"])
    first, wrapped = fields[0], fields[1]

    if (wrapped.get("text") or "") != expected:
        print(
            f"A11: the field inside the named layout holds "
            f"{wrapped.get('text')!r}, not {expected!r}. The fill named the "
            f"wrapper and the text went somewhere else.",
            file=sys.stderr,
        )
        return 1

    if expected in (first.get("text") or ""):
        print(
            f"A11: the text landed in the field above instead — the one that had "
            f"focus before. Naming a layout must not mean typing wherever focus "
            f"happened to be.",
            file=sys.stderr,
        )
        return 1

    print(
        f"  A11: a fill naming a layout reached the field inside it, and left the "
        f"one that had focus alone"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
