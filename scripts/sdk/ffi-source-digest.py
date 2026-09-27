#!/usr/bin/env python3
"""Print the digest of the sources smix-ffi's native library is built from.

The shipped xcframework and Android libraries are committed binaries, and
nothing tied them to the source they came from: for two months every
release carried the core of 2026-07-18. The build scripts put this digest
into the library (scripts/sdk/build-xcframework.sh, build-android-aar.sh)
and scripts/dev/ffi-bindings-fresh.sh compares what the libraries carry
with what the tree gives now.

Nothing here lists files. What goes in is read from two authorities:

* the dep-info cargo writes beside the host library
  (`target/release/libsmix_ffi.d`): every source file the compiler read to
  build it and the crates below it, and nothing it did not — so tests,
  benches and fixtures do not move the digest;
* `cargo metadata`: the manifest of every workspace crate in that graph,
  the workspace manifest, and the name, version and source of every
  registry crate in it — what the dep-info cannot see.

Usage:
    scripts/sdk/ffi-source-digest.py [--no-build] [--list]

--no-build uses the dep-info already on disk; the caller has just built.
--list prints the inputs instead of the digest.
"""
from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DEP_INFO = os.path.join(ROOT, "target", "release", "libsmix_ffi.d")


def fail(msg: str) -> None:
    print(f"ffi-source-digest: {msg}", file=sys.stderr)
    sys.exit(1)


def dep_info_files() -> list[str]:
    try:
        text = open(DEP_INFO, encoding="utf-8").read()
    except FileNotFoundError:
        fail(f"no {os.path.relpath(DEP_INFO, ROOT)} — build smix-ffi first, or drop --no-build")
    # `target: dep dep ...`, with `\ ` for a space inside a path
    _, _, deps = text.partition(": ")
    files = []
    for raw in deps.replace("\\ ", "\0").split():
        path = os.path.normpath(raw.replace("\0", " "))
        rel = os.path.relpath(path, ROOT)
        # generated into target/ from inputs that are themselves listed
        if rel.startswith("target" + os.sep) or rel.startswith(".."):
            continue
        files.append(rel)
    if not files:
        fail(f"{os.path.relpath(DEP_INFO, ROOT)} names no source file")
    return files


def graph() -> tuple[list[str], list[str]]:
    """Manifests of the workspace crates smix-ffi builds from, and one line
    per registry crate in its graph."""
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=ROOT, capture_output=True, text=True,
    )
    if out.returncode != 0:
        fail(f"cargo metadata failed: {out.stderr.strip()}")
    meta = json.loads(out.stdout)
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    roots = [p["id"] for p in meta["packages"] if p["name"] == "smix-ffi"]
    if len(roots) != 1:
        fail(f"expected one smix-ffi package, found {len(roots)}")
    seen, stack = set(), list(roots)
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes[pid]["deps"]:
            # normal and build dependencies make the library; dev ones do not
            if any(k["kind"] in (None, "build") for k in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    manifests, registry = [], []
    for pid in seen:
        p = packages[pid]
        if p["source"] is None:
            manifests.append(os.path.relpath(p["manifest_path"], ROOT))
        else:
            registry.append(f"{p['name']} {p['version']} {p['source']}")
    return sorted(manifests), sorted(registry)


def main() -> None:
    args = sys.argv[1:]
    if "--no-build" not in args:
        build = subprocess.run(
            ["cargo", "build", "-q", "-p", "smix-ffi", "--release"], cwd=ROOT,
        )
        if build.returncode != 0:
            fail("cargo build -p smix-ffi --release failed")
    manifests, registry = graph()
    files = sorted(set(dep_info_files()) | set(manifests) | {"Cargo.toml"})
    if "--list" in args:
        print("\n".join(files + registry))
        return
    h = hashlib.sha256()
    for rel in files:
        h.update(rel.encode() + b"\0")
        with open(os.path.join(ROOT, rel), "rb") as fh:
            h.update(fh.read())
        h.update(b"\0")
    for line in registry:
        h.update(line.encode() + b"\n")
    print(h.hexdigest())


if __name__ == "__main__":
    main()
