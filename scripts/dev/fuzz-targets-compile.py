#!/usr/bin/env python3
"""The fuzz targets still compile against the crates they fuzz.

A fuzz crate is outside the workspace: `cargo check`, `clippy` and
`cargo test` never look at it. The only thing that compiles a fuzz target
is `cargo fuzz`, and the only thing that runs `cargo fuzz` is the ship's
fuzz smoke -- two and a half hours in.

Measured 2026-08-29: `A11yNode` gained a field during v10; a fuzz target
hand-writes all fifteen of them; nothing said so until the eighth dry
run reached the fuzz smoke and stopped there. The error was one missing
line, and it had been sitting in the tree for days.

This asks the cheap question early: does each fuzz crate typecheck? Five
seconds each, no nightly, no sanitizer, no corpus. It does not replace
the fuzz smoke -- that one runs them.

The list of crates comes from the filesystem, so a new fuzz crate is
covered the day it is added rather than the day someone remembers to
name it here.
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CRATES = os.path.join(ROOT, "crates")

manifests = []
for name in sorted(os.listdir(CRATES)):
    manifest = os.path.join(CRATES, name, "fuzz", "Cargo.toml")
    if os.path.isfile(manifest):
        manifests.append(manifest)

if not manifests:
    print("fuzz-targets-compile: FAIL")
    print(
        "  - no fuzz crate was found under crates/*/fuzz — either they moved "
        "or this scan is reading air; a check with nothing to check agrees "
        "with every tree there is"
    )
    sys.exit(1)

problems: list[str] = []
for manifest in manifests:
    rel = os.path.relpath(manifest, ROOT)
    proc = subprocess.run(
        ["cargo", "check", "--manifest-path", manifest, "--message-format", "short"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        said = [
            ln for ln in proc.stderr.splitlines()
            if "error" in ln and not ln.startswith("error: could not compile")
        ]
        problems.append(
            f"{rel} does not compile. Its targets are built by nothing else "
            f"until the ship's fuzz smoke, hours in.\n      "
            + "\n      ".join(said[:3] or proc.stderr.splitlines()[-3:])
        )

if problems:
    print("fuzz-targets-compile: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)

print(f"fuzz-targets-compile: clean — {len(manifests)} fuzz crates typecheck")
