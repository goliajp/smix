#!/usr/bin/env python3
"""The fixture apk carries the sources it was built from, and a gate can ask.

`[ -f "$APK" ]` is a proxy that is only right once. The file being there
says a build happened; it says nothing about which sources it happened
over, and the device gates install whatever is at that path.

That cost a day: C13 changed the probe, which is compiled into the
fixture's debug variant, and C7 then ran against an apk built before it.
Two of its verdicts went red — `clipped-bounds=lost-the-row` and
`offscreen-kept=placeless` — and they read as the probe being wrong
about the screen, which is exactly what C7 exists to catch. Rebuilt from
the same tree the same hour, all five verdicts were green (open-items
O1).

So the build writes what it built from, and a gate compares. The hash
covers the probe's sources as well as the fixture's own: the probe
arrives through `debugImplementation("jp.golia.smix:smix-probe")`, which
`settings.gradle.kts` substitutes with the local project, so a probe
edit changes the apk while leaving every file under
`test-fixtures/` untouched. Hashing only the fixture's own sources would
have missed precisely the change that caused this.

One recipe, two callers: `--write` from the build script, `--check` from
every gate that installs the thing. A second copy of the file list is
the copy that goes stale (`code/derive-dont-copy`).

    fixture-apk-stamp.py --write [--root DIR]
    fixture-apk-stamp.py --check [--root DIR]

Exit: 0 the apk is current, 1 it is not (and the verdict names why).
"""

import argparse
import hashlib
import json
import os
import sys

# What ends up inside the apk, in the two trees it comes from. Each entry
# is a directory walked for source files, or a single file.
def source_inputs(root: str) -> list[str]:
    """Every path whose content decides what the apk contains."""
    return [
        os.path.join(root, "test-fixtures", "android-app", "app", "src", "main"),
        os.path.join(root, "test-fixtures", "android-app", "app", "build.gradle.kts"),
        os.path.join(root, "test-fixtures", "android-app", "settings.gradle.kts"),
        # The probe is compiled in, not depended on as a published
        # artifact — see the module docstring.
        os.path.join(root, "android-runner", "probe", "src", "main"),
        os.path.join(root, "android-runner", "probe", "build.gradle.kts"),
    ]


def apk_path(root: str) -> str:
    return os.path.join(
        root, "test-fixtures", "android-app", "app", "build", "outputs", "apk",
        "debug", "app-debug.apk",
    )


def stamp_path(root: str) -> str:
    return apk_path(root) + ".sources.sha256"


def hashes(root: str) -> dict[str, str]:
    """Every source file, by its path relative to the root, with its digest."""
    out: dict[str, str] = {}
    for entry in source_inputs(root):
        if os.path.isfile(entry):
            files = [entry]
        elif os.path.isdir(entry):
            files = []
            for dirpath, _dirs, names in os.walk(entry):
                files.extend(os.path.join(dirpath, n) for n in names)
        else:
            # A path that is not there is a change like any other: the
            # stamp records its absence by not listing it, and the check
            # below reports it as removed.
            continue
        for path in files:
            rel = os.path.relpath(path, root)
            with open(path, "rb") as fh:
                out[rel] = hashlib.sha256(fh.read()).hexdigest()
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")
    ap.add_argument("--root", default=os.path.dirname(os.path.dirname(
        os.path.dirname(os.path.abspath(__file__)))))
    args = ap.parse_args()
    root = os.path.abspath(args.root)

    now = hashes(root)
    # An empty file list would agree with every apk ever built. The
    # inputs moving is a real possibility — this walks two trees by name
    # — and a scan that hashes nothing must say so rather than pass.
    if not now:
        print("fixture-apk-stamp: FAIL — hashed no sources at all; the paths in "
              "`source_inputs` do not exist under " + root, file=sys.stderr)
        return 1

    if args.write:
        with open(stamp_path(root), "w", encoding="utf-8") as fh:
            json.dump({"files": now}, fh, indent=0, sort_keys=True)
        print(f"fixture-apk-stamp: wrote {len(now)} source digests beside the apk")
        return 0

    apk = apk_path(root)
    if not os.path.exists(apk):
        print(f"fixture-apk-stamp: FAIL — no fixture apk at {apk}. "
              "Build it: bash scripts/dev/build-android-fixture.sh", file=sys.stderr)
        return 1
    stamp = stamp_path(root)
    if not os.path.exists(stamp):
        # Not "no stamp, carry on": an apk from before stamps existed is
        # exactly an apk nobody can vouch for.
        print(f"fixture-apk-stamp: FAIL — {os.path.basename(apk)} carries no record of "
              "the sources it was built from. Rebuild it: "
              "bash scripts/dev/build-android-fixture.sh", file=sys.stderr)
        return 1
    with open(stamp, encoding="utf-8") as fh:
        was = json.load(fh).get("files", {})

    changed = sorted(p for p in now if p in was and now[p] != was[p])
    added = sorted(p for p in now if p not in was)
    removed = sorted(p for p in was if p not in now)
    if not (changed or added or removed):
        print(f"fixture-apk-stamp: clean — the apk is the one these {len(now)} "
              "source files build")
        return 0

    print("fixture-apk-stamp: FAIL — the fixture apk was built from other sources. "
          "Rebuild it: bash scripts/dev/build-android-fixture.sh", file=sys.stderr)
    for label, paths in (("changed", changed), ("added", added), ("removed", removed)):
        for path in paths[:4]:
            print(f"  {label}: {path}", file=sys.stderr)
        if len(paths) > 4:
            print(f"  {label}: … and {len(paths) - 4} more", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
