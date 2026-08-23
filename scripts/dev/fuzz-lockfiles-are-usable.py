#!/usr/bin/env python3
"""Every fuzz lockfile still satisfies the manifests above it.

A fuzz crate depends on this workspace by path, so a version bump in a
crate's `Cargo.toml` can make the fuzz lockfile beside it unsatisfiable.
Nothing said so. `kevy-embedded 5.3 -> 5.4.1` left four fuzz lockfiles
pinning 5.3.0 against a requirement that now reads 5.4.1, and they sat
that way through a full green CI run: no gate reads these files, and
`--locked` appeared nowhere in the workflow. The bump before it, 5.1 ->
5.3, left the same wreck and was tidied up by the next release
regenerating everything — which reads like a convention and is actually
just the next person's cargo command rewriting a file nobody checked.

An unsatisfiable lockfile is not a stale lockfile. It is not a lockfile
at all: the first `cargo fuzz` run silently resolves something else and
writes it back, so the pinning the file exists to provide was never in
force. That is worth a second of CI.

The check is `cargo metadata --locked`, which is the whole predicate —
it fails when the lockfile would have to change to build, and passes
otherwise. It deliberately does NOT compare versions against the
workspace lockfile: the fuzz trees resolve independently and 332
transitive packages differ between them today, every one of them a
valid resolution. A gate that red-lined those would be noise wearing a
gate's clothes.

Two of the fifteen fuzz crates shipped with cargo-fuzz's default
`.gitignore`, which ignores `Cargo.lock`. An ignored lockfile cannot be
checked by anyone but the machine that wrote it, so that is red too —
otherwise the way to pass this gate is to stop tracking the file.

Usage:
  scripts/dev/fuzz-lockfiles-are-usable.py [repo-root]
"""

import glob
import os
import subprocess
import sys

ROOT = os.path.abspath(
    sys.argv[1]
    if len(sys.argv) > 1
    else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
)


def in_git_repo(path):
    return (
        subprocess.run(
            ["git", "-C", path, "rev-parse", "--is-inside-work-tree"],
            capture_output=True,
        ).returncode
        == 0
    )


def ignored(path):
    # `git check-ignore` exits 0 when the path IS ignored.
    return (
        subprocess.run(
            ["git", "-C", ROOT, "check-ignore", "-q", path], capture_output=True
        ).returncode
        == 0
    )


def main():
    manifests = sorted(glob.glob(os.path.join(ROOT, "crates", "*", "fuzz", "Cargo.toml")))
    if not manifests:
        # A check that passes having read nothing is not a check.
        print(f"fuzz-lockfiles: CANNOT RUN — no fuzz manifests under {ROOT}/crates")
        return 1

    git = in_git_repo(ROOT)
    problems = []

    for manifest in manifests:
        rel = os.path.relpath(manifest, ROOT)
        lock = os.path.join(os.path.dirname(manifest), "Cargo.lock")

        if git and ignored(lock):
            problems.append(
                f"{os.path.relpath(lock, ROOT)}: ignored by a .gitignore, so no "
                f"checkout but the author's has a lockfile to honour"
            )
            continue

        if not os.path.exists(lock):
            problems.append(f"{os.path.relpath(lock, ROOT)}: missing")
            continue

        done = subprocess.run(
            [
                "cargo",
                "metadata",
                "--manifest-path",
                manifest,
                "--locked",
                "--format-version",
                "1",
            ],
            capture_output=True,
            text=True,
        )
        if done.returncode != 0:
            detail = " ".join(
                line.strip()
                for line in done.stderr.splitlines()
                if line.strip().startswith("error")
            )
            problems.append(f"{rel}: {detail or 'cargo metadata --locked failed'}")

    if problems:
        print("fuzz-lockfiles: RED — a lockfile the next cargo command would rewrite\n")
        for p in problems:
            print(f"  {p}")
        print(
            "\nRegenerate it rather than editing it:\n"
            "  cargo update --manifest-path <crate>/fuzz/Cargo.toml -p <package>\n"
            "A version is a whole token and a string replace does not know that."
        )
        return 1

    print(f"fuzz-lockfiles: clean — {len(manifests)} fuzz lockfiles satisfy their manifests")
    return 0


if __name__ == "__main__":
    sys.exit(main())
