#!/usr/bin/env python3
"""Does the stamp notice the things it exists to notice?

The one that matters is the third: a probe source edited after the build.
That is the shape that cost a day — nothing under `test-fixtures/`
changed, the apk looked perfectly present, and C7 measured a probe that
was not this one.

Each case builds a small tree with the same shape as the repository's,
because a self-test that hashes a different set of paths than the real
run is two instruments (§14.4).
"""

import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
STAMP = os.path.join(HERE, "fixture-apk-stamp.py")

APP_SRC = ("test-fixtures", "android-app", "app", "src", "main", "Fixture.kt")
PROBE_SRC = ("android-runner", "probe", "src", "main", "Probe.kt")
APK = ("test-fixtures", "android-app", "app", "build", "outputs", "apk", "debug",
       "app-debug.apk")
GRADLE = [
    ("test-fixtures", "android-app", "app", "build.gradle.kts"),
    ("test-fixtures", "android-app", "settings.gradle.kts"),
    ("android-runner", "probe", "build.gradle.kts"),
]


def write(root, parts, text):
    path = os.path.join(root, *parts)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(text)
    return path


def tree(root):
    write(root, APP_SRC, "class Fixture\n")
    write(root, PROBE_SRC, "class Probe\n")
    for g in GRADLE:
        write(root, g, "plugins {}\n")
    write(root, APK, "not really an apk\n")


def run(root, mode):
    return subprocess.run(
        [sys.executable, STAMP, mode, "--root", root],
        capture_output=True, text=True,
    )


def case(name, fn):
    with tempfile.TemporaryDirectory() as root:
        tree(root)
        ok, why = fn(root)
    print(("  ok   " if ok else "  FAIL ") + name + ("" if ok else f" — {why}"))
    return ok


def built_here(root):
    run(root, "--write")
    r = run(root, "--check")
    return r.returncode == 0, f"exit {r.returncode}: {r.stderr.strip()}"


def fixture_source_edited(root):
    run(root, "--write")
    write(root, APP_SRC, "class Fixture // one more word\n")
    r = run(root, "--check")
    named = "Fixture.kt" in r.stderr
    return (r.returncode == 1 and named), f"exit {r.returncode}, names it: {named}"


def probe_source_edited(root):
    """The one that cost a day: the apk's own tree is untouched."""
    run(root, "--write")
    write(root, PROBE_SRC, "class Probe // reports two rectangles now\n")
    r = run(root, "--check")
    named = "Probe.kt" in r.stderr
    return (r.returncode == 1 and named), f"exit {r.returncode}, names it: {named}"


def no_stamp_at_all(root):
    r = run(root, "--check")
    return r.returncode == 1, f"exit {r.returncode} — an unvouched-for apk must not pass"


def no_apk(root):
    run(root, "--write")
    os.remove(os.path.join(root, *APK))
    r = run(root, "--check")
    return r.returncode == 1, f"exit {r.returncode}"


def source_removed(root):
    run(root, "--write")
    os.remove(os.path.join(root, *PROBE_SRC))
    r = run(root, "--check")
    return (r.returncode == 1 and "removed" in r.stderr), \
        f"exit {r.returncode}: {r.stderr.strip()[:80]}"


def nothing_to_hash(root):
    """An input set that has moved hashes nothing, and nothing agrees with
    everything. It has to say so instead of passing."""
    import shutil
    shutil.rmtree(os.path.join(root, "test-fixtures"))
    shutil.rmtree(os.path.join(root, "android-runner"))
    r = run(root, "--check")
    return (r.returncode == 1 and "no sources" in r.stderr), \
        f"exit {r.returncode}: {r.stderr.strip()[:80]}"


def stamp_is_json(root):
    run(root, "--write")
    with open(os.path.join(root, *APK) + ".sources.sha256", encoding="utf-8") as fh:
        got = json.load(fh)["files"]
    # Five inputs: two sources and three gradle files.
    return len(got) == 5, f"{len(got)} digests, expected 5"


def main() -> int:
    cases = [
        ("an apk built from these sources passes", built_here),
        ("a fixture source edited after the build is caught", fixture_source_edited),
        ("a PROBE source edited after the build is caught", probe_source_edited),
        ("an apk with no stamp does not pass", no_stamp_at_all),
        ("a stamp with no apk does not pass", no_apk),
        ("a source file removed after the build is caught", source_removed),
        ("an input set that hashes nothing is a failure", nothing_to_hash),
        ("the stamp records every input", stamp_is_json),
    ]
    bad = [name for name, fn in cases if not case(name, fn)]
    if bad:
        print(f"fixture-apk-stamp.test: FAIL — {len(bad)} of {len(cases)}")
        return 1
    print(f"fixture-apk-stamp.test: clean — {len(cases)} cases, including a probe "
          "edit that leaves the fixture's own tree untouched")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
