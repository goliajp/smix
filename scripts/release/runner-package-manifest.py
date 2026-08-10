#!/usr/bin/env python3
"""The Package.swift the runner tarball ships: what the runner builds.

`swift-bridge/Package.swift` describes the whole workspace — the runner,
the Swift SDK, the UniFFI bindings, a fixture executable. The runner
project asks for exactly one product of it (`project.yml`:
`package: SmixRunnerCore, product: SmixRunnerCore`), and
`SmixRunnerCore` depends only on FlyingFox.

Shipping the whole manifest sent a `.binaryTarget` pointing at
`SmixCoreFFI.xcframework` to every user, and the tarball deliberately
excludes that file — it is 49 MB against a 0.25 MB archive that is
compiled into the CLI with `include_bytes!`. SwiftPM resolves the entire
package graph before building anything, so a target the runner never
links stopped the runner from starting:

    local binary target 'SmixCoreFFI' … does not contain …

On this machine it worked, because earlier builds had left an
xcframework in `~/.local/share/smix/runner/`. On a clean checkout — a CI
runner, or anyone who ran `cargo install smix-cli` — it did not.

The tarball was assembled by exclusion: take `swift-bridge/`, drop the
binaries and the caches, ship the rest. That shape decides nothing; what
is left over is whatever nobody thought to remove, and a stale
declaration survives it. This decides instead: name the products the
runner builds, and emit a manifest containing those and their
dependencies.

Emits to stdout. `--check` compares against the workspace manifest and
lists what would be dropped, which is how the gate reads it.
"""

import argparse
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SOURCE = os.path.join(ROOT, "swift-bridge", "Package.swift")

# What `swift-bridge/project.yml` asks the package for. Read from there
# rather than restated, so a runner that starts needing another product
# does not silently get a manifest without it.
def products_the_runner_needs() -> set[str]:
    yml = os.path.join(ROOT, "swift-bridge", "project.yml")
    with open(yml, encoding="utf-8") as fh:
        text = fh.read()
    return set(re.findall(r"^\s*product:\s*(\S+)\s*$", text, re.M))


def blocks(src: str, kind: str) -> list[tuple[int, int, str]]:
    """Every `.<kind>(...)` entry, as (start, end, name), paren-balanced.

    Brace matching rather than a regex: a target block spans lines,
    contains nested calls and commas, and the first attempt at this with
    a pattern silently kept everything it did not understand — which for
    a manifest means shipping the declaration that caused the bug.
    """
    out = []
    for m in re.finditer(r"\.\s*" + kind + r"\(", src):
        i = m.end() - 1
        depth, j = 0, i
        while j < len(src):
            if src[j] == "(":
                depth += 1
            elif src[j] == ")":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        body = src[m.start() : j + 1]
        name = re.search(r'name:\s*"([^"]+)"', body)
        out.append((m.start(), j + 1, name.group(1) if name else ""))
    return out


def dependencies_of(src: str, target: str) -> set[str]:
    """Target names this target depends on, within this package."""
    for start, end, name in blocks(src, "target") + blocks(src, "testTarget"):
        if name != target:
            continue
        body = src[start:end]
        dep = body.split("dependencies:", 1)
        if len(dep) < 2:
            return set()
        # Only in-package names: `.product(name: "FlyingFox", package: …)`
        # is an external dependency and stays as written.
        seg = dep[1]
        return {
            n
            for n in re.findall(r'"([A-Za-z][A-Za-z0-9_]*)"', seg)
            if not re.search(r'\.product\(\s*name:\s*"' + re.escape(n), seg)
        }
    return set()


def keep_set(src: str, wanted: set[str]) -> set[str]:
    """`wanted` plus everything they depend on, transitively."""
    keep, queue = set(), list(wanted)
    while queue:
        t = queue.pop()
        if t in keep:
            continue
        keep.add(t)
        queue.extend(dependencies_of(src, t))
    return keep


def emit(src: str, keep: set[str]) -> str:
    """The manifest with every product and target outside `keep` removed."""
    cuts: list[tuple[int, int]] = []
    for kind in ("library", "executable"):
        for start, end, name in blocks(src, kind):
            if name and name not in keep:
                cuts.append((start, end))
    for kind in ("target", "testTarget", "executableTarget", "binaryTarget"):
        for start, end, name in blocks(src, kind):
            if name and name not in keep:
                cuts.append((start, end))
    out = src
    for start, end in sorted(cuts, reverse=True):
        # Take the trailing comma and blank line with it.
        tail = end
        while tail < len(out) and out[tail] in ",\n":
            tail += 1
            if out[tail - 1] == "\n":
                break
        head = start
        while head > 0 and out[head - 1] in " \t":
            head -= 1
        out = out[:head] + out[tail:]
    # Drop comments left orphaned by the cuts.
    #
    # A removed target leaves the paragraph that explained it, and those
    # name the things that are gone. Harmless to SwiftPM and confusing
    # to a reader — and they cost three rounds of verification here,
    # because `grep -c SmixCoreFFI` counted them and read as "the
    # trimming did not work" when `binaryTarget` had been zero all
    # along.
    kept: list[str] = []
    pending: list[str] = []
    for line in out.splitlines(keepends=True):
        if line.strip().startswith("//"):
            pending.append(line)
            continue
        if line.strip() in ("", "]", ")"):
            # A comment block followed by nothing was explaining
            # something that is no longer here.
            if pending and line.strip() != "":
                pending = []
            kept.append(line)
            continue
        kept.extend(pending)
        pending = []
        kept.append(line)
    return re.sub(r"\n{3,}", "\n\n", "".join(kept))


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--check", action="store_true", help="list what would be dropped")
    args = ap.parse_args()

    with open(SOURCE, encoding="utf-8") as fh:
        src = fh.read()

    wanted = products_the_runner_needs()
    if not wanted:
        print(
            "runner-package-manifest: no `product:` line in project.yml — the "
            "runner's dependency shape changed and this would emit an empty "
            "manifest",
            file=sys.stderr,
        )
        return 1

    keep = keep_set(src, wanted)
    result = emit(src, keep)

    if args.check:
        dropped = sorted(
            {n for _, _, n in blocks(src, "target") + blocks(src, "binaryTarget") if n}
            - keep
        )
        print(f"runner-package-manifest: keeps {sorted(keep)}")
        print(f"runner-package-manifest: drops {dropped}")
        if "SmixCoreFFI" not in dropped:
            print(
                "runner-package-manifest: FAIL — SmixCoreFFI is still in the "
                "manifest. It is a 49 MB binary the tarball excludes, and "
                "SwiftPM resolves the whole graph before building, so leaving "
                "it declared is what stopped `runner up` on every machine but "
                "the one this was written on.",
                file=sys.stderr,
            )
            return 1
        return 0

    sys.stdout.write(result)
    return 0


if __name__ == "__main__":
    sys.exit(main())
