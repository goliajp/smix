#!/usr/bin/env python3
"""Every publishable crate is in the ship's publish list, in an order that works.

`Cargo.toml` says `members = ["crates/*"]`, so a new crate joins the
workspace by existing. It joins the *release* by being typed into a
hand-written array in `ship.sh` — thirty names, in dependency order,
that nothing compares against the workspace.

The two ways that array goes wrong are not equally loud:

- A crate that something else depends on and is missing from the list
  fails at `cargo publish` time, about forty minutes into a ship, with
  a message about an unresolvable dependency.
- A crate that nothing depends on yet — every crate, on the release it
  is introduced — is simply never published. Nothing fails. It is on
  crates.io in the reader's mind and absent in fact, which is the
  failure this file exists to prevent, and the reason it was written
  the week a new crate was about to be added.

Order matters as much as membership: `cargo publish` resolves a
dependency from the registry, so a crate must be published after every
workspace crate it depends on. That is a topological property and it is
checkable, so it is checked rather than trusted to the comment above
the array.

Usage:
  scripts/dev/publish-dag-is-complete.py [repo-root]
"""

import json
import os
import re
import subprocess
import sys

ROOT = (
    sys.argv[1]
    if len(sys.argv) > 1
    else os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
)
SHIP = os.path.join("scripts", "release", "ship.sh")

# The array, read from the shell rather than by running it. Anchored on
# `CRATES=(` so a second array elsewhere in the file cannot be mistaken
# for this one.
ARRAY = re.compile(r"^CRATES=\(\s*$(.*?)^\)\s*$", re.MULTILINE | re.DOTALL)

# A tree with no crates and a ship with no array would otherwise agree
# perfectly. Both sides have a floor: the comparison is only meaningful
# when there is something on each.
MIN_CRATES = 10


def read_listed(ship_text: str) -> list[str]:
    m = ARRAY.search(ship_text)
    if m is None:
        return []
    names: list[str] = []
    for line in m.group(1).splitlines():
        line = line.split("#", 1)[0]
        names.extend(line.split())
    return names


def workspace_crates(root: str) -> dict[str, set[str]]:
    """Publishable workspace crates → their workspace dependencies."""
    out = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=root,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    meta = json.loads(out)
    members = {p["name"]: p for p in meta["packages"] if p.get("publish") != []}
    all_names = {p["name"] for p in meta["packages"]}
    graph: dict[str, set[str]] = {}
    for name, pkg in members.items():
        graph[name] = {
            d["name"]
            for d in pkg.get("dependencies", [])
            # Dev-dependencies do not have to be on crates.io for a
            # publish to resolve, so they do not constrain the order.
            if d.get("kind") != "dev" and d["name"] in all_names and d["name"] != name
        }
    return graph


def main() -> int:
    ship_path = os.path.join(ROOT, SHIP)
    if not os.path.isfile(ship_path):
        print("publish-dag-is-complete: CANNOT RUN")
        print(f"  - {SHIP} is not in this tree")
        return 2

    listed = read_listed(open(ship_path, encoding="utf-8").read())
    graph = workspace_crates(ROOT)

    problems: list[str] = []

    if len(listed) < MIN_CRATES:
        problems.append(
            f"{SHIP} lists {len(listed)} crate(s). Below {MIN_CRATES} this is not a "
            f"publish list — either the array moved and the anchor `CRATES=(` no "
            f"longer finds it, or the file is not the one this checks. An empty "
            f"list agrees with an empty workspace, so neither side may be empty."
        )
    if len(graph) < MIN_CRATES:
        problems.append(
            f"cargo metadata reports {len(graph)} publishable crate(s), fewer than "
            f"{MIN_CRATES}. This is not the smix workspace."
        )

    if not problems:
        missing = sorted(set(graph) - set(listed))
        for name in missing:
            dependents = sorted(n for n, deps in graph.items() if name in deps)
            tail = (
                f" — {', '.join(dependents)} depend(s) on it, so the ship would fail "
                f"at cargo publish"
                if dependents
                else " — nothing depends on it yet, so the ship would say COMPLETE "
                "and this crate would simply not be on crates.io"
            )
            problems.append(f"{name} is a publishable crate and is not in {SHIP}{tail}")

        stale = sorted(set(listed) - set(graph))
        for name in stale:
            problems.append(
                f"{SHIP} lists {name}, which is not a publishable crate in this "
                f"workspace — it was renamed, removed, or marked publish = false"
            )

        dupes = sorted({n for n in listed if listed.count(n) > 1})
        for name in dupes:
            problems.append(f"{SHIP} lists {name} more than once")

        # Order: a crate is published after everything it depends on.
        position = {name: i for i, name in enumerate(listed)}
        for name in listed:
            for dep in sorted(graph.get(name, ())):
                if dep in position and position[dep] > position[name]:
                    problems.append(
                        f"{SHIP} publishes {name} (position {position[name]}) before "
                        f"{dep} (position {position[dep]}), which it depends on. "
                        f"cargo resolves dependencies from the registry, so the "
                        f"earlier one cannot build."
                    )

    if problems:
        print("publish-dag-is-complete: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    print(
        f"publish-dag-is-complete: clean — {len(graph)} publishable crates, all in "
        f"{SHIP}, each after everything it depends on"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
