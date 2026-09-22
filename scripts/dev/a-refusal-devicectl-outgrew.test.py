#!/usr/bin/env python3
"""a-refusal-devicectl-outgrew.py must go red on the shapes it exists for.

Trees built in a temp dir — a minimal ACTION_PLATFORMS table and a fake
`xcrun` whose `devicectl … --help` prints canned SUBCOMMANDS sections —
judged by the real gate through SMIX_DEVICECTL_XCRUN.
"""

import os
import stat
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "a-refusal-devicectl-outgrew.py")

TABLE = """
pub const ACTION_PLATFORMS: &[(&str, [Availability; 4])] = {
    use Availability::{RefusedByName as No, Works as Yes};
    const NO_DEVICECTL_VERB: &str = "devicectl has no verb for it";
    &[
        ("platform", [Yes, Yes, Yes, Yes]),
        ("screenshot", [Yes, Yes, Yes, Yes]),
        ("pasteboard_set", [Yes, Yes, No { why: "devicectl has `device pasteboard copy`; undriven", instead: "x" }, Yes]),
        ("pasteboard_get", [Yes, Yes, No { why: "devicectl has `device pasteboard paste`; undriven", instead: "x" }, Yes]),
        ("location_set", [Yes, Yes, No { why: "devicectl has `device simulate location coordinate`; undriven", instead: "x" }, Yes]),
        ("location_start", [Yes, Yes, No { why: "devicectl has `device simulate location route`; undriven", instead: "x" }, Yes]),
        ("set_animations_quiet", [Yes, Yes, No { why: NO_DEVICECTL_VERB, instead: "x" }, Yes]),
        (
            "add_media",
            [
                Yes,
                Yes,
                No {
                    why: "%(media_why)s",
                    instead: "x",
                },
                Yes,
            ],
        ),
        ("set_permission", [Yes, Yes, No { why: "no equivalent of simctl privacy", instead: "x" }, Yes]),
        ("erase", [Yes, Yes, No { why: "a phone is not erased from a host", instead: "x" }, Yes]),
        ("boot", [Yes, Yes, No { why: "a phone is switched on by hand", instead: "x" }, Yes]),
    ]
};
"""

EMPTY_TABLE = """
pub const ACTION_PLATFORMS: &[(&str, [Availability; 4])] = {
    use Availability::{RefusedByName as No, Works as Yes};
    &[
        ("platform", [Yes, Yes, Yes, Yes]),
        ("screenshot", [Yes, Yes, Yes, Yes]),
    ]
};
"""

# Canned `devicectl <parent> --help` SUBCOMMANDS sections, one file per
# parent path; the fake `xcrun` cats the one its arguments name.
# `with_media` decides whether `device` lists a media subcommand. The row
# this varies has to be one the gate still checks: it was `screenshot` until
# 10.2 drove that verb and its refusal left the table.
def fake_xcrun(path: str, with_media: bool) -> None:
    d = os.path.dirname(path)
    top = "OVERVIEW: x\n\nSUBCOMMANDS:\n  capture                 Capture the device's screen.\n  pasteboard  x\n  settings  x\n  simulate  x\n"
    if with_media:
        top += "  media                   Add media to the device.\n"
    top += "\n  See help.\n"
    canned = {
        "device_pasteboard": "SUBCOMMANDS:\n  copy  x\n  paste  x\n  info  x\n\n",
        "device_simulate_location": "SUBCOMMANDS:\n  clear  x\n  coordinate  x\n  route  x\n\n",
        "device_simulate": "SUBCOMMANDS:\n  biometrics  x\n  location  x\n  statusBar  x\n\n",
        "device_settings": "SUBCOMMANDS:\n  appearance  x\n  audio  x\n  reset  x\n\n",
        "device": top,
    }
    for name, text in canned.items():
        with open(os.path.join(d, f"help_{name}.txt"), "w") as fh:
            fh.write(text)
    script = f"""#!/bin/sh
# $1 = devicectl, then the parent path, then --help (or --version)
shift
case "$*" in
  --version) echo "fake devicectl 1.0"; exit 0 ;;
esac
key=""
for a in "$@"; do
  [ "$a" = "--help" ] && break
  key="${{key}}${{key:+_}}$a"
done
f="{d}/help_$key.txt"
[ -f "$f" ] && cat "$f" || printf 'no such\n'
"""
    with open(path, "w") as fh:
        fh.write(script)
    os.chmod(path, os.stat(path).st_mode | stat.S_IEXEC)


def build(root: str, table: str, with_media: bool) -> str:
    os.makedirs(os.path.join(root, "crates", "smix-sdk", "src"))
    with open(os.path.join(root, "crates", "smix-sdk", "src", "device_control.rs"), "w") as fh:
        fh.write(table)
    xcrun = os.path.join(root, "xcrun")
    fake_xcrun(xcrun, with_media)
    return xcrun


def run(root: str, xcrun: str) -> subprocess.CompletedProcess:
    env = dict(os.environ, SMIX_DEVICECTL_XCRUN=xcrun)
    return subprocess.run([sys.executable, GATE, root], capture_output=True, text=True, env=env)


def main() -> int:
    fail = 0

    with tempfile.TemporaryDirectory() as tmp:
        x = build(tmp, TABLE % {"media_why": "devicectl has no verb for it"}, with_media=False)
        r = run(tmp, x)
        if r.returncode != 0:
            print(f"① verb absent, refusal says absent: expected green, exit {r.returncode}\n{r.stdout}")
            fail = 1

    with tempfile.TemporaryDirectory() as tmp:
        x = build(tmp, TABLE % {"media_why": "devicectl has no verb for it"}, with_media=True)
        r = run(tmp, x)
        if r.returncode != 1 or "add_media" not in r.stdout or "device media" not in r.stdout:
            print(f"② verb exists, refusal denies it: expected red naming the verb, exit {r.returncode}\n{r.stdout}")
            fail = 1

    with tempfile.TemporaryDirectory() as tmp:
        x = build(tmp, TABLE % {"media_why": "devicectl has `device media`; smix does not drive it yet"}, with_media=True)
        r = run(tmp, x)
        if r.returncode != 0:
            print(f"③ verb exists and the refusal names it: expected green, exit {r.returncode}\n{r.stdout}")
            fail = 1

    with tempfile.TemporaryDirectory() as tmp:
        build(tmp, TABLE % {"media_why": "x"}, with_media=False)
        r = run(tmp, os.path.join(tmp, "no-such-xcrun"))
        if r.returncode != 2 or "cannot run" not in r.stdout:
            print(f"④ no xcrun: expected exit 2 'cannot run', exit {r.returncode}\n{r.stdout}")
            fail = 1

    with tempfile.TemporaryDirectory() as tmp:
        x = build(tmp, EMPTY_TABLE, with_media=False)
        r = run(tmp, x)
        if r.returncode != 1 or "read nothing" not in r.stdout:
            print(f"⑤ no refusals on PhysicalIos: expected red 'read nothing', exit {r.returncode}\n{r.stdout}")
            fail = 1

    if fail:
        return 1
    print("a-refusal-devicectl-outgrew.test: absent verb, outgrown refusal, named verb, no devicectl, and an empty column are each judged as they should be")
    return 0


if __name__ == "__main__":
    sys.exit(main())
