#!/usr/bin/env python3
"""Did a fill into a Compose field land, and was it read back as landed?

A Compose text field publishes to its accessibility node asynchronously,
and the node the framework hands out carries the values it had when it
was fetched. 6.4.0 shipped a read-back that did neither a refresh nor a
wait, so it reported zero characters about a field holding all
seventeen, and refused every fill that had worked.

This reads the tree, which refreshes each node — a second opinion that
does not come from the path under test.
"""

import json
import sys


def main() -> int:
    tree, marker = json.load(open(sys.argv[1])), sys.argv[2]
    found: dict[str, object] = {}

    def walk(node: dict) -> None:
        if (node.get("identifier") or "").endswith("compose_input"):
            found["text"] = node.get("text")
        for child in node.get("children", []):
            walk(child)

    walk(tree)

    if "text" not in found:
        print(
            "A7: no compose_input in the tree — the Compose screen is not in "
            "front, so nothing here is asserting about it",
            file=sys.stderr,
        )
        return 1

    got = found["text"]
    if got != marker:
        print(
            f"A7: the fill reported success and the Compose field holds "
            f"{got!r}, not {marker!r}",
            file=sys.stderr,
        )
        return 1

    print(f"  A7: a fill into a Compose field put {marker!r} there, and said so")
    return 0


if __name__ == "__main__":
    sys.exit(main())
