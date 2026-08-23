#!/usr/bin/env python3
"""Does the fuzz-lockfile gate go red, and for the right reason?

Four shapes, because the gate makes four different judgements and a
green run proves none of them. Each case falsifies exactly one: the
lockfile is missing an entry the manifest needs, the lockfile is not
there at all, the lockfile is ignored so no other checkout has it, and
there are no fuzz manifests to read — the last one because a check that
passes having read nothing is the shape this repo refuses everywhere
else.

The fixture depends only by path, so `cargo metadata` needs no registry
and this runs on a machine with no network.
"""

import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "fuzz-lockfiles-are-usable.py")

CRATE = """[package]
name = "demo"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"""

FUZZ = """[package]
name = "demo-fuzz"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
demo = { path = ".." }

[lib]
path = "src/lib.rs"
"""


def build_tree(with_fuzz=True):
    d = tempfile.mkdtemp()
    if not with_fuzz:
        os.makedirs(os.path.join(d, "crates"))
        return d
    fuzz = os.path.join(d, "crates", "demo", "fuzz")
    os.makedirs(os.path.join(fuzz, "src"))
    os.makedirs(os.path.join(d, "crates", "demo", "src"))
    with open(os.path.join(d, "crates", "demo", "Cargo.toml"), "w") as f:
        f.write(CRATE)
    with open(os.path.join(d, "crates", "demo", "src", "lib.rs"), "w") as f:
        f.write("pub fn f() {}\n")
    with open(os.path.join(fuzz, "Cargo.toml"), "w") as f:
        f.write(FUZZ)
    with open(os.path.join(fuzz, "src", "lib.rs"), "w") as f:
        f.write("pub fn g() {}\n")
    # Let cargo write the lockfile the gate will later be asked to honour.
    done = subprocess.run(
        ["cargo", "metadata", "--manifest-path", os.path.join(fuzz, "Cargo.toml"),
         "--format-version", "1", "--offline"],
        capture_output=True, text=True,
    )
    if done.returncode != 0:
        print("SETUP FAILED — cargo could not resolve the fixture:")
        print(done.stderr)
        sys.exit(2)
    return d


def run(root):
    done = subprocess.run(
        [sys.executable, GATE, root], capture_output=True, text=True
    )
    return done.returncode, done.stdout + done.stderr


def expect_red(label, root, must_say):
    rc, out = run(root)
    if rc == 0:
        print(f"FAIL [{label}]: gate stayed green")
        return False
    if must_say not in out:
        print(f"FAIL [{label}]: red, but not for the stated reason — wanted "
              f"{must_say!r} in:\n{out}")
        return False
    if "Traceback" in out:
        print(f"FAIL [{label}]: red by crash, not by judgement:\n{out}")
        return False
    print(f"ok   [{label}]")
    return True


def main():
    ok = True

    # The gate must be green on a tree that is right, or every red below
    # proves nothing.
    clean = build_tree()
    rc, out = run(clean)
    if rc != 0:
        print(f"FAIL [clean]: gate red on a correct tree:\n{out}")
        ok = False
    else:
        print("ok   [clean]")

    # 1. The lockfile no longer covers the manifests above it.
    stale = build_tree()
    lock = os.path.join(stale, "crates", "demo", "fuzz", "Cargo.lock")
    text = open(lock).read()
    cut = text.replace('name = "demo"\nversion = "0.1.0"\n', 'name = "demo"\nversion = "0.9.9"\n')
    if cut == text:
        print("FAIL [setup]: could not mutate the fixture lockfile")
        return 1
    open(lock, "w").write(cut)
    ok &= expect_red("lockfile does not satisfy the manifest", stale, "fuzz/Cargo.toml")

    # 2. No lockfile at all.
    gone = build_tree()
    os.remove(os.path.join(gone, "crates", "demo", "fuzz", "Cargo.lock"))
    ok &= expect_red("lockfile missing", gone, "missing")

    # 3. A lockfile no other checkout receives.
    hidden = build_tree()
    subprocess.run(["git", "init", "-q", hidden], check=True)
    with open(os.path.join(hidden, ".gitignore"), "w") as f:
        f.write("crates/*/fuzz/Cargo.lock\n")
    ok &= expect_red("lockfile ignored", hidden, "ignored by a .gitignore")

    # 4. Nothing to read is not a pass.
    empty = build_tree(with_fuzz=False)
    ok &= expect_red("no fuzz manifests", empty, "CANNOT RUN")

    for d in (clean, stale, gone, hidden, empty):
        shutil.rmtree(d, ignore_errors=True)

    print("fuzz-lockfile gate self-test: " + ("PASS" if ok else "FAIL"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
