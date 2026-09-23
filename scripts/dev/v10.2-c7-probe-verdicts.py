#!/usr/bin/env python3
"""What the probe says about the interop screen, judged line by line.

Its own file rather than a heredoc inside the shell script: macOS ships
bash 3.2, which cannot parse a heredoc inside `$( )` when the body has
parentheses in it, and the script simply ends with `unexpected EOF`.

Reads `PROBE_JSON` and `A11Y_JSON` from the environment, prints one line
per verdict, and exits 1 with the disagreements named.
"""

import json
import os
import sys

probe = json.loads(os.environ["PROBE_JSON"])
a11y = json.loads(os.environ["A11Y_JSON"])

# Both sides arrive from `smix tree --json --reader …` now, so both are
# the same envelope and the same node shape: a Compose testTag and a
# hosted View's resource id both land in `identifier`. They were two
# shapes while the probe could only be reached by curl (I1).
#
# Which reader answered is asserted rather than assumed — `--reader`
# refusing and `--reader` answering from the other tree would look the
# same here, and this file's whole job is telling the two apart.
for name, payload, want in (("probe", probe, "semantics"), ("a11y", a11y, "a11y")):
    got = payload.get("source")
    if got != want:
        sys.exit(f"the {name} side came back from the {got!r} reader, not {want!r}")

probe = probe["root"]
a11y = a11y["root"]


def index(tree):
    found = {}

    def walk(n):
        i = n.get("identifier")
        if i:
            found[i.split("/")[-1]] = n
        for c in n.get("children") or []:
            walk(c)

    walk(tree)
    return found


seen = index(probe)
theirs = index(a11y)

def rect(n):
    """Where the node is: the rectangle it occupies, clipped or not."""
    b = n["bounds"]
    return (b["x"], b["y"], b["x"] + b["w"], b["y"] + b["h"])


def shown(n):
    """How much of it can be seen — the other question, the other rectangle.

    A reader that answers both with one rectangle cannot be asked how
    much of a row shows, and the scroll rule that divides one by the
    other then reads every sliver as a whole row. `visibleBounds` when
    the reader sends it; the accessibility path only ever reports the
    part that shows, so for it the two are the same.
    """
    b = n.get("visibleBounds")
    if not b:
        return rect(n)
    return (b["x"], b["y"], b["x"] + b["w"], b["y"] + b["h"])

lines, bad = [], []

# 1 — the hosted View is there, where the other reader says it is.
btn = seen.get("fixture_interop_button")
if btn is None:
    bad.append("interop-seen=no — the probe does not report the hosted ImageButton")
else:
    mine, yours = rect(btn), rect(theirs["fixture_interop_button"])
    if mine != yours:
        bad.append(f"interop-seen=misplaced — probe {mine}, accessibility {yours}")
    else:
        lines.append(f"interop-seen=yes            {mine} role={btn.get('role')}")

# 2 — the node nobody placed is not in the tree. Its absence is only
#     worth asserting because the binary before the fix DID report it, at
#     [0,323,213,368]; that is recorded in this file's header.
if "interop_unplaced" in seen:
    bad.append(f"unplaced-omitted=no — reported at {seen['interop_unplaced']['bounds']}")
else:
    lines.append("unplaced-omitted=yes        (it was reported at [0,323,213,368] before)")

# 3 — a row cut by the top of its viewport is reported cut.
row = seen.get("interop_clipped_row_0")
if row is None:
    bad.append("clipped-bounds=missing — the first row is not in the probe's tree")
else:
    mine, yours = shown(row), shown(theirs["interop_clipped_row_0"])
    height = mine[3] - mine[1]
    whole = rect(row)[3] - rect(row)[1]
    if mine != yours:
        bad.append(f"clipped-bounds=disagree — probe {mine}, accessibility {yours}")
    elif height >= 275:
        bad.append(f"clipped-bounds=unclipped — {height}px showing, the whole row")
    elif whole < 275:
        # The row's own rectangle must stay whole: it is what the scroll
        # rule divides by, and clipping it there is what made every
        # partly-visible row read as fully visible.
        bad.append(f"clipped-bounds=lost-the-row — the node itself reads {whole}px of 275")
    else:
        lines.append(
            f"clipped-bounds=agree        {mine} ({height}px of {whole} showing)"
        )

# 4 — and the rows below the viewport are carried as showing nothing,
#     rather than dropped or reported somewhere.
below = [t for t in ("interop_clipped_row_5", "interop_clipped_row_6") if t in seen]
if len(below) != 2:
    bad.append(f"offscreen-kept=no — {below} of 2 rows below the fold are in the tree")
elif any(seen[t].get("visible") is not False for t in below):
    bad.append("offscreen-kept=visible — a row below the fold says it is showing")
elif any(shown(seen[t])[3] - shown(seen[t])[1] > 0 for t in below):
    bad.append("offscreen-kept=shows-something — a row below the fold has a visible part")
elif any(rect(seen[t])[3] - rect(seen[t])[1] <= 0 for t in below):
    # It knows where it is even though none of it shows. Without that,
    # a scroll cannot tell "below the fold" from "not laid out".
    bad.append("offscreen-kept=placeless — a row below the fold lost its own rectangle")
else:
    lines.append(
        "offscreen-kept=yes          (present, visible=false, its rectangle kept, nothing showing)"
    )

# 5 — the role a flow would match on, named by the same table the other
#     reader uses.
if btn is not None and btn.get("role") != theirs["fixture_interop_button"].get("role"):
    bad.append(
        f"role-through=no — probe says {btn.get('role')}, accessibility says "
        f"{theirs['fixture_interop_button'].get('role')}"
    )
elif btn is not None:
    lines.append(f"role-through=yes            role={btn.get('role')} on both readers")

print("\n".join(lines))
if bad:
    print("BAD")
    print("\n".join(bad))
    sys.exit(1)
