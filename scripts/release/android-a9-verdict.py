#!/usr/bin/env python3
"""Did a fill into a masked field land — judged by what a mask can tell you?

A password field's accessibility node reports one bullet per character
and never the characters. So the read-back predicate 6.4.0 shipped —
does the node's text contain what I typed — is asking a question this
field cannot answer, and its answer is no for every fill that ever
worked. A consumer's twenty-flow Android suite stopped at the first
flow, which signs in.

This asserts both halves, because only the pair is meaningful:

  - the field grew by exactly as many characters as were dispatched
  - the plaintext is genuinely NOT in the node

The second is what makes the first the only available evidence. Drop it
and this file would keep passing against a field that stopped masking,
which is the state in which the old content predicate also works — and
then nothing here would be testing the masked case at all.
"""

import json
import sys


def main() -> int:
    tag, secret, before_len = sys.argv[2], sys.argv[3], int(sys.argv[4])
    tree = json.load(open(sys.argv[1]))
    found: dict[str, object] = {}

    def walk(node: dict) -> None:
        if (node.get("identifier") or "").endswith(tag):
            found["text"] = node.get("text") or ""
        for child in node.get("children", []):
            walk(child)

    walk(tree)

    if "text" not in found:
        print(
            f"A9: no {tag} in the tree — the Compose screen is not in front, "
            f"so nothing here is asserting about it",
            file=sys.stderr,
        )
        return 1

    got = str(found["text"])

    if secret in got:
        print(
            f"A9: the field is not masking — it reads back {got!r}, which "
            f"contains the plaintext. This assertion is about a masked field; "
            f"against an unmasked one it proves nothing, so it fails rather "
            f"than passing for the wrong reason.",
            file=sys.stderr,
        )
        return 1

    grew = len(got) - before_len
    if grew != len(secret):
        print(
            f"A9: dispatched {len(secret)} characters into a masked field and "
            f"it grew by {grew} (from {before_len} to {len(got)}). The length "
            f"difference is the only evidence a mask can give, and it does not "
            f"add up.",
            file=sys.stderr,
        )
        return 1

    print(
        f"  A9: a fill into a masked field grew it by exactly {len(secret)}, "
        f"and the plaintext is not readable from the node"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
