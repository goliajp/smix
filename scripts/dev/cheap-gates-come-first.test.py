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
SHIP = os.path.join(os.path.dirname(os.path.dirname(HERE)), "scripts", "release", "ship.sh")


# Every profile the ship writes ends with the publishing, and the gate
# refuses a publishing exemption that matched nothing — so a fixture
# without those rows is not the file this gate reads, and the cases below
# would be judging an instrument built differently from the real one.
# The step that comes after the release has gone out: seconds long, last
# by necessity, and a judgement rather than an action — so it reaches the
# profile and must be exempted by position. The publish lines themselves
# are logged with `note` and never appear here.
PUBLISHING_TAIL = [
    (3, "verify what the registries took"),
]


def profile(rows, publishing=True):
    fd, path = tempfile.mkstemp(suffix=".tsv")
    with os.fdopen(fd, "w") as fh:
        for secs, name in list(rows) + (PUBLISHING_TAIL if publishing else []):
            fh.write(f"{secs}\t{name}\n")
    return path


def run(path, ship=None, costs=None):
    """Drive the gate over one profile.

    `costs` is the cost history the gate compares against — always a
    file of this test's own, because the real one lives in the machine's
    data directory and a self-test that writes there would both change
    what the next ship judges and depend on what the last one left.

    A history that exists but holds nothing is not the same as none: the
    default gives the gate a file it has already "seen", so the cases
    below judge rather than report a first run. The first-run behaviour
    has a case of its own.
    """
    if costs is None:
        fd, costs = tempfile.mkstemp(suffix=".costs.tsv")
        with os.fdopen(fd, "w") as fh:
            fh.write("999999\tsomething this profile does not contain\n")
    argv = [sys.executable, GATE, path, ship or SHIP, costs]
    p = subprocess.run(argv, capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


def ship_without_device_steps():
    """A ship.sh whose steps no longer touch a build product or a device.

    The dependency exemption is derived by a regex over ship.sh. Nothing
    else notices when a reader stops matching, and a reader that matches
    nothing puts every device gate back into a budget it cannot be moved
    out of — while reading, from outside, as a stricter check.
    """
    real = os.path.join(os.path.dirname(HERE), "release", "ship.sh")
    src = open(real, encoding="utf-8").read()
    for token in ("cargo ", "./gradlew", "xcrun simctl", "adb ", "SMIX_BIN",
                  "target/release", "xcodebuild", "swift test", "--device",
                  "--serial", "_DEVICE", "_SERIAL", "_SIM"):
        src = src.replace(token, "REDACTED")
    fd, path = tempfile.mkstemp(suffix=".sh")
    with os.fdopen(fd, "w") as fh:
        fh.write(src)
    return path


def ship_without_publishing():
    """A ship.sh with no `cargo publish` call to find.

    The post-publish exemption is located by that call. Without it the
    gate cannot tell which steps come after the release, and would judge
    the verification that must come last as a gate in the wrong place.
    """
    real = os.path.join(os.path.dirname(HERE), "release", "ship.sh")
    src = open(real, encoding="utf-8").read().replace("cargo publish -p", "REDACTED")
    fd, path = tempfile.mkstemp(suffix=".sh")
    with os.fdopen(fd, "w") as fh:
        fh.write(src)
    return path


def case(name, rows, want_code, must_say, publishing=True, ship=None, costs=None):
    code, out = run(profile(rows, publishing), ship, costs)
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

    # The 10.1.0 misjudgement: the same ordering, on a machine under
    # load. Every step took forty times longer, the ci-green step that
    # is seven seconds read 275, and this gate failed a ship whose
    # ordering was correct. With a history to compare against, the
    # smallest reading of each step is what counts.
    import tempfile as _tf

    fd, hist = _tf.mkstemp(suffix=".costs.tsv")
    with os.fdopen(fd, "w") as fh:
        for secs, name in cheap + dear:
            fh.write(f"{secs}\t{name}\n")
    slow = [(secs * 40 + 7, name) for secs, name in cheap + dear]
    ok &= case(
        "a loaded machine does not turn a correct ordering red",
        slow,
        0,
        "no seconds-long",
        costs=hist,
    )

    # And the opposite: a step that is expensive in every reading still
    # puts what follows it in the wrong place.
    fd, hist2 = _tf.mkstemp(suffix=".costs.tsv")
    with os.fdopen(fd, "w") as fh:
        for secs, name in dear + cheap:
            fh.write(f"{secs}\t{name}\n")
    ok &= case(
        "a structurally expensive step is still judged expensive",
        dear + cheap,
        1,
        "sits behind",
        costs=hist2,
    )

    # A machine that has never run a ship has one reading of each step
    # and no way to tell load from cost. It says so instead of failing.
    fd, empty = _tf.mkstemp(suffix=".costs.tsv")
    os.close(fd)
    os.unlink(empty)
    ok &= case(
        "the first run on a machine establishes a baseline",
        dear + cheap,
        2,
        "no cost history yet",
        costs=empty,
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

    # The publishing exemptions have to have excused something. They are
    # prefixes over thirty crates rather than names, so nothing else would
    # notice if a rename left them matching nothing — and an exemption
    # matching nothing still prints as one that looked and approved.
    ok &= case(
        "a release with no publish call to locate",
        cheap + dear,
        1,
        "come after the release",
        ship=ship_without_publishing(),
    )

    ok &= case(
        "the dependency reader stops matching",
        cheap + dear,
        1,
        "stopped matching",
        ship=ship_without_device_steps(),
    )

    # The profile a ship reads at the moment this gate runs: everything up
    # to here, and no publishing at all.
    ok &= case(
        "the partial profile a ship actually hands it",
        cheap + dear,
        0,
        "clean",
        publishing=False,
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
