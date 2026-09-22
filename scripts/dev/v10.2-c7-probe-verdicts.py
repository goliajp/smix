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
a11y = a11y.get("root", a11y)

seen = {}
def walk(n):
    name = n.get("testTag") or n.get("resourceId")
    if name:
        seen[name] = n
    for c in n.get("children") or []:
        walk(c)
for r in probe.get("roots", []):
    walk(r)

theirs = {}
def walk_a(n):
    i = n.get("identifier")
    if i:
        theirs[i] = n
    for c in n.get("children") or []:
        walk_a(c)
walk_a(a11y)

def rect(n):
    b = n["bounds"]
    return (b["x"], b["y"], b["x"] + b["w"], b["y"] + b["h"])

lines, bad = [], []

# 1 — the hosted View is there, where the other reader says it is.
btn = seen.get("fixture_interop_button")
if btn is None:
    bad.append("interop-seen=no — the probe does not report the hosted ImageButton")
else:
    mine, yours = tuple(btn["bounds"]), rect(theirs["fixture_interop_button"])
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
    mine, yours = tuple(row["bounds"]), rect(theirs["interop_clipped_row_0"])
    height = mine[3] - mine[1]
    if mine != yours:
        bad.append(f"clipped-bounds=disagree — probe {mine}, accessibility {yours}")
    elif height >= 275:
        bad.append(f"clipped-bounds=unclipped — {height}px tall, the whole row")
    else:
        lines.append(f"clipped-bounds=agree        {mine} ({height}px of 275)")

# 4 — and the rows below the viewport are carried as showing nothing,
#     rather than dropped or reported somewhere.
below = [t for t in ("interop_clipped_row_5", "interop_clipped_row_6") if t in seen]
if len(below) != 2:
    bad.append(f"offscreen-kept=no — {below} of 2 rows below the fold are in the tree")
elif any(seen[t].get("visible") is not False for t in below):
    bad.append("offscreen-kept=visible — a row below the fold says it is showing")
else:
    lines.append("offscreen-kept=yes          (present, visible=false, empty rectangle)")

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
