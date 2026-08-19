#!/usr/bin/env python3
"""Did a named fill put its text into the app, or only say so?

Reads the fixture's *result* label rather than the field it typed into.
The field's own value comes back along the path under test; the label
only changes when Submit hands the app what it actually received, so it
reports what arrived instead of what smix believes it sent.
"""

import json
import sys


def main() -> int:
    tree_path, marker = sys.argv[1], sys.argv[2]
    tree = json.load(open(tree_path))

    found: dict[str, str] = {}

    def walk(node: dict) -> None:
        ident = node.get("identifier") or ""
        for key in ("fixture_result", "fixture_input"):
            if key in ident:
                found[key] = node.get("text") or ""
        for child in node.get("children", []):
            walk(child)

    walk(tree)

    if "fixture_result" not in found:
        print(
            "A5: the fixture's result label is not in the tree, so nothing "
            "here can say what the app received",
            file=sys.stderr,
        )
        return 1

    got = found["fixture_result"]
    if got != marker:
        print(
            f"A5: the fill reported success and the app received {got!r}, not "
            f"{marker!r} (the field itself holds "
            f"{found.get('fixture_input')!r}). A fill that types nothing and "
            "says ok is worse than one that fails: every flow built on it "
            "goes green having done nothing.",
            file=sys.stderr,
        )
        return 1

    print(f"  A5: a named fill put {marker!r} into the app, first try")
    return 0


if __name__ == "__main__":
    sys.exit(main())
