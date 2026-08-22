#!/usr/bin/env python3
"""Does the ordering gate go red on the shape that cost 6.3.0 four rounds?

Its subject is a file the ship writes, so the cases here are profiles
rather than trees: a good ordering, the one that actually happened, a
profile too short to mean anything, and none at all.
"""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "cheap-gates-come-first.py")


def profile(rows):
    fd, path = tempfile.mkstemp(suffix=".tsv")
    with os.fdopen(fd, "w") as fh:
        for secs, name in rows:
            fh.write(f"{secs}\t{name}\n")
    return path


def run(path):
    p = subprocess.run([sys.executable, GATE, path], capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


def case(name, rows, want_code, must_say):
    code, out = run(profile(rows))
    if code != want_code:
        print(f"  FAIL {name}: exit {code}, wanted {want_code}\n{out}")
        return False
    if "Traceback" in out:
        print(f"  FAIL {name}: red by raising, not by judging\n{out}")
        return False
    if must_say not in out:
        print(f"  FAIL {name}: does not say {must_say!r}\n{out}")
        return False
    print(f"  ok   {name}")
    return True


def main():
    ok = True
    cheap = [(1, f"scan {i}") for i in range(20)]
    dear = [(240, "cargo test"), (600, "corpus gate")]

    ok &= case("cheap first, expensive after", cheap + dear, 0, "no seconds-long")

    # What 6.3.0 was: a one-second version check behind an hour.
    ok &= case(
        "a one-second judgement behind minutes of work",
        dear + cheap,
        1,
        "sits behind",
    )

    # A build read as cheap because the cache was warm. Its position is
    # a dependency, not a choice, and `cargo build --release` measured
    # 0s on a warm run and 28s on a cold one — this check asked for the
    # 0s one to move to the front.
    ok &= case(
        "a warm-cache build late in the run is not a misplaced judgement",
        cheap + dear + [(0, "cargo build -p smix-cli --release (for corpus gate)")],
        0,
        "no seconds-long",
    )

    # The other half, so the exemption above is a rule rather than a
    # hole: an ordinary judgement in the same position is still flagged.
    ok &= case(
        "an ordinary judgement in that same position still is",
        cheap + dear + [(0, "some other scan")],
        1,
        "sits behind",
    )

    # A profile short enough to agree with anything.
    ok &= case(
        "a profile too short to mean anything",
        [(1, "a"), (2, "b"), (3, "c")],
        1,
        "fewer than",
    )

    # No profile: the apparatus, not the subject. Refusing rather than
    # agreeing — inside a ship the profile always exists, so its
    # absence means the writing broke.
    code, out = run("/nonexistent-profile-for-this-test.tsv")
    if code != 2 or "CANNOT RUN" not in out:
        print(f"  FAIL no profile at all: exit {code}\n{out}")
        ok = False
    else:
        print("  ok   no profile at all refuses rather than agreeing")

    print("=== cheap-gates-come-first.test:", "PASS" if ok else "FAIL", "===")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
