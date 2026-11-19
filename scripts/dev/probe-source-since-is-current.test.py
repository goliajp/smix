#!/usr/bin/env python3
"""Does the probe-source gate go red, and for the right reason?

One case per judgement: the source matches (green), a probe file changed,
a probe file was added, no probe source at all, and a recorded release
later than the workspace. Each red must name its reason, not just exit 1.
"""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "probe-source-since-is-current.py")
CONSTANTS = "crates/smix-runner-client/src/screen_read/probe_source.rs"


def write(root, rel, text):
    path = os.path.join(root, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as fh:
        fh.write(text)


def run(root):
    p = subprocess.run(
        [sys.executable, GATE, "--root", root], capture_output=True, text=True
    )
    return p.returncode, p.stdout + p.stderr


def tree(root, since="22.1.0", digest="x", workspace="26.6.0"):
    write(root, "Cargo.toml", f'[workspace.package]\nversion = "{workspace}"\n')
    write(root, "android-runner/probe/build.gradle.kts", "plugins {}\n")
    write(root, "android-runner/probe/src/main/kotlin/P.kt", "class P\n")
    write(
        root,
        CONSTANTS,
        f'pub(crate) const PROBE_SOURCE_SINCE: &str = "{since}";\n'
        f'pub(crate) const PROBE_SOURCE_DIGEST: &str =\n    "{digest}";\n',
    )


def digest_of(root):
    code, out = run(root)
    assert code == 1 and "PROBE_SOURCE_DIGEST to " in out, out
    return out.split("PROBE_SOURCE_DIGEST to ")[1].split()[0]


def case(name, setup, want_code, want_text):
    with tempfile.TemporaryDirectory() as root:
        tree(root)
        good = digest_of(root)
        tree(root, digest=good)
        setup(root)
        code, out = run(root)
    ok = code == want_code and want_text in out
    print(f"{'ok  ' if ok else 'FAIL'} {name}: exit {code}: {out.strip()}")
    return ok


def main():
    results = [
        case("matching source", lambda r: None, 0, "clean — 2 files read"),
        case(
            "a changed file",
            lambda r: write(r, "android-runner/probe/src/main/kotlin/P.kt", "class Q\n"),
            1,
            "changed since 22.1.0",
        ),
        case(
            "an added file",
            lambda r: write(r, "android-runner/probe/src/main/kotlin/R.kt", "class R\n"),
            1,
            "changed since 22.1.0",
        ),
        case(
            "no probe source",
            lambda r: os.remove(os.path.join(r, "android-runner/probe/src/main/kotlin/P.kt")),
            1,
            "nothing was read",
        ),
        case(
            "a release later than the workspace",
            lambda r: write(r, "Cargo.toml", '[workspace.package]\nversion = "21.0.0"\n'),
            1,
            "later than the workspace version",
        ),
    ]
    return 0 if all(results) else 1


if __name__ == "__main__":
    sys.exit(main())
