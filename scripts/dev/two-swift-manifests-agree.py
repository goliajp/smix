#!/usr/bin/env python3
"""The root Package.swift builds what swift-bridge/Package.swift builds.

The root manifest is the one SwiftPM consumers resolve; the one under
swift-bridge/ is the one this repository's own tests build. They are two
copies of one target graph, and they drifted: SmixIndigoHID gained a
dependency on SmixDeveloperDir in the second and not in the first, so the
published package declared a target that could not compile, for two
releases, while every test here stayed green.

Checked, for every target the root manifest declares:
  - swift-bridge declares a target of the same name, with the same
    dependencies;
  - each dependency that is not an external product is a target the root
    manifest declares too.

Usage:
    python3 scripts/dev/two-swift-manifests-agree.py [--root DIR] [--root-manifest FILE]
"""

from __future__ import annotations

import argparse
import os
import re
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

TARGET = re.compile(r"\.(?:target|executableTarget|testTarget|binaryTarget)\(\s*name:\s*\"([^\"]+)\"")
PRODUCT_DEP = re.compile(r"\.product\(\s*name:\s*\"([^\"]+)\"")


def targets(text: str) -> dict[str, tuple[set[str], set[str]]]:
    """Each target's name → (local dependencies, external product dependencies)."""
    out: dict[str, tuple[set[str], set[str]]] = {}
    starts = [m for m in TARGET.finditer(text)]
    for i, m in enumerate(starts):
        end = starts[i + 1].start() if i + 1 < len(starts) else len(text)
        body = text[m.end():end]
        deps = re.search(r"dependencies:\s*\[(.*?)\]", body, re.S)
        local: set[str] = set()
        external: set[str] = set()
        if deps:
            inner = deps.group(1)
            external = set(PRODUCT_DEP.findall(inner))
            stripped = PRODUCT_DEP.sub("", inner)
            local = set(re.findall(r"\"([^\"]+)\"", stripped)) - external
            # `.product(name: "X", package: "Y")` leaves the package name behind
            local -= set(re.findall(r"package:\s*\"([^\"]+)\"", inner))
        out[m.group(1)] = (local, external)
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=REPO)
    ap.add_argument("--root-manifest", help="read this file as the root Package.swift")
    args = ap.parse_args()
    root_path = args.root_manifest or os.path.join(args.root, "Package.swift")
    with open(root_path, encoding="utf-8") as fh:
        published = targets(fh.read())
    with open(os.path.join(args.root, "swift-bridge", "Package.swift"), encoding="utf-8") as fh:
        bridge = targets(fh.read())

    if not published or not bridge:
        print(f"two-swift-manifests-agree: read {len(published)} root and {len(bridge)} bridge targets — "
              "the reader found nothing, which proves nothing")
        return 1

    failures: list[str] = []
    for name, (local, external) in sorted(published.items()):
        if name not in bridge:
            failures.append(f"{name}: in the root manifest, not in swift-bridge's")
            continue
        b_local, b_external = bridge[name]
        if (local, external) != (b_local, b_external):
            failures.append(
                f"{name}: root depends on {sorted(local | external)}, "
                f"swift-bridge on {sorted(b_local | b_external)}"
            )
        for dep in sorted(local - set(published)):
            failures.append(f"{name}: depends on {dep}, which the root manifest does not declare")

    for f in failures:
        print(f"two-swift-manifests-agree: {f}")
    if failures:
        return 1
    print(f"two-swift-manifests-agree: {len(published)} root targets match swift-bridge's")
    return 0


if __name__ == "__main__":
    sys.exit(main())
