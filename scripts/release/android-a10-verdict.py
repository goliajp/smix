#!/usr/bin/env python3
"""A named fill goes to the field it names, and leaves the others alone.

`AndroidDriver::fill` resolves the selector, taps it to move focus, then
asks the runner to clear and type — and the runner acts on whatever
holds focus at that instant. Focus in Compose does not move
synchronously with the tap, so both halves can still reach the field
that had focus before: the characters land in the wrong field and the
wrong field is emptied first.

The wait that was supposed to prevent this asked whether *some* editable
node had focus. One already did. A predicate that is true before the
action it guards is not guarding it.

Two assertions, because either alone passes for the wrong reason: a
fill that does nothing at all leaves the other field intact, and a fill
that types into the right field could still have erased the other one
on the way.
"""

import json
import sys


def main() -> int:
    tree = json.load(open(sys.argv[1]))
    named, expected = sys.argv[2], sys.argv[3]
    bystander, bystander_len = sys.argv[4], int(sys.argv[5])
    seen: dict[str, str] = {}

    def walk(node: dict) -> None:
        ident = (node.get("identifier") or "").split("/")[-1]
        if ident in (named, bystander):
            seen[ident] = node.get("text") or ""
        for child in node.get("children", []):
            walk(child)

    walk(tree)

    for want in (named, bystander):
        if want not in seen:
            print(f"A10: no {want} in the tree — nothing here is asserting", file=sys.stderr)
            return 1

    if seen[named] != expected:
        print(
            f"A10: the fill named {named} and it holds {seen[named]!r}, not "
            f"{expected!r}. The characters went to whatever held focus when "
            f"the runner looked, which is not what the caller named.",
            file=sys.stderr,
        )
        return 1

    if len(seen[bystander]) != bystander_len:
        print(
            f"A10: filling {named} changed {bystander} — it held "
            f"{bystander_len} characters and now holds {len(seen[bystander])}. "
            f"The clear that precedes a fill ran against the previously "
            f"focused field, so a fill of one field emptied another.",
            file=sys.stderr,
        )
        return 1

    print(
        f"  A10: a named fill reached {named}, and left {bystander}'s "
        f"{bystander_len} characters alone"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
