#!/usr/bin/env python3
"""The release smix names for the probe source matches the probe source.

smix tells a user their app's smix-probe is behind only when the probe
has changed since that app's version. It knows when the probe last
changed from two constants in the Rust host: the first release with
today's probe source (`PROBE_SOURCE_SINCE`) and a digest of that source
(`PROBE_SOURCE_DIGEST`). This checks the digest against the source in
the tree, so a change to the probe cannot ship while smix still says
older probes are current.

When it is red: set `PROBE_SOURCE_SINCE` to the workspace version the
change ships in and `PROBE_SOURCE_DIGEST` to the digest printed here.

The source is every file under the probe's `src/main` plus its build
script. Tests are left out: they do not change what an app runs.

`--root DIR` reads another tree, for the gate's own test.
"""

import hashlib
import os
import re
import sys

CONSTANTS = "crates/smix-runner-client/src/screen_read/probe_source.rs"
PROBE = "android-runner/probe"


def probe_files(root):
    base = os.path.join(root, PROBE)
    files = [os.path.join(base, "build.gradle.kts")]
    for dirpath, _, names in os.walk(os.path.join(base, "src", "main")):
        files.extend(os.path.join(dirpath, n) for n in names)
    return sorted(f for f in files if os.path.isfile(f))


def digest(root, files):
    h = hashlib.sha256()
    for f in files:
        h.update(os.path.relpath(f, root).encode())
        h.update(b"\0")
        with open(f, "rb") as fh:
            h.update(fh.read())
        h.update(b"\0")
    return h.hexdigest()


def const(text, name):
    m = re.search(name + r':\s*&str\s*=\s*"([^"]*)"', text)
    return m.group(1) if m else None


def workspace_version(root):
    with open(os.path.join(root, "Cargo.toml")) as fh:
        text = fh.read()
    m = re.search(r'^\[workspace\.package\][^\[]*?^version\s*=\s*"([^"]+)"', text, re.M | re.S)
    return m.group(1) if m else None


def parts(v):
    try:
        out = tuple(int(p) for p in v.split("."))
    except ValueError:
        return None
    return out if len(out) == 3 else None


def main():
    root = os.getcwd()
    if len(sys.argv) == 3 and sys.argv[1] == "--root":
        root = sys.argv[2]
    files = probe_files(root)
    sources = [f for f in files if not f.endswith("build.gradle.kts")]
    if not sources:
        print(f"probe-source: no files under {PROBE}/src/main — nothing was read")
        return 1
    with open(os.path.join(root, CONSTANTS)) as fh:
        text = fh.read()
    since = const(text, "PROBE_SOURCE_SINCE")
    recorded = const(text, "PROBE_SOURCE_DIGEST")
    if since is None or recorded is None:
        print(f"probe-source: {CONSTANTS} has no PROBE_SOURCE_SINCE / PROBE_SOURCE_DIGEST")
        return 1
    ours = workspace_version(root)
    if parts(since) is None or ours is None or parts(ours) is None:
        print(f"probe-source: cannot compare PROBE_SOURCE_SINCE {since!r} with workspace {ours!r}")
        return 1
    if parts(since) > parts(ours):
        print(f"probe-source: PROBE_SOURCE_SINCE {since} is later than the workspace version {ours}")
        return 1
    actual = digest(root, files)
    if actual != recorded:
        print(
            f"probe-source: {PROBE} changed since {since} — set PROBE_SOURCE_SINCE to {ours} "
            f"and PROBE_SOURCE_DIGEST to {actual} in {CONSTANTS}"
        )
        return 1
    print(f"probe-source: clean — {len(files)} files read, the probe source is the one from {since}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
