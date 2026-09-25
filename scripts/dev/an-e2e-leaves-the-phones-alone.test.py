#!/usr/bin/env python3
"""The phones gate goes red on each thing it exists to catch.

Each case writes a small tree, runs the gate against it, and checks the
exit code and that the sentence names the file. A case that expects
green carries the three members the gate requires to be present, so a
green here means "nothing to object to", not "nothing was looked at".
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "an-e2e-leaves-the-phones-alone.py")

# One of each rule's compliant member, so the presence check is satisfied
# and a case isolates the one thing it varies.
BASELINE = """\
e2e_isolate_machine "$WORK"
"$SMIX" sim register ok --udid 00008120-0000000000C0FFEE --kind physical-ios
e2e_physical_or_skip ios base
SMIX_E2E_PHYSICAL_IOS="$E2E_PHYSICAL" cargo test -p smix-usbmux --test live
unset SMIX_E2E_PHYSICAL_ANDROID SMIX_E2E_PHYSICAL_IOS SMIX_E2E_PHYSICAL_IOS_PORT
for e2e in "$ROOT"/scripts/dev/*-e2e.sh; do bash "$e2e"; done
"""


def run(files: dict[str, str]) -> tuple[int, str]:
    with tempfile.TemporaryDirectory() as root:
        os.makedirs(os.path.join(root, "scripts", "dev"))
        for name, body in files.items():
            with open(os.path.join(root, "scripts", "dev", name), "w") as fh:
                fh.write(body)
        p = subprocess.run(
            [sys.executable, GATE, "--root", root, "--min-scripts", "1"],
            capture_output=True,
            text=True,
        )
        return p.returncode, p.stdout + p.stderr


CASES = [
    # (label, files, want_rc, want_substring)
    ("the baseline is clean", {"base.sh": BASELINE}, 0, "clean"),
    (
        "a registry write with no isolation is named",
        {"base.sh": BASELINE, "bad.sh": '"$SMIX" sim register x --udid emulator-5554 --kind emulator\n'},
        1,
        "bad.sh:1: writes the device registry",
    ),
    (
        "isolation after the write does not count",
        {
            "base.sh": BASELINE,
            "late.sh": 'smix sim allow-destructive x\ne2e_isolate_machine "$WORK"\n',
        },
        1,
        "late.sh:1",
    ),
    (
        "a per-command SMIX_MACHINE_DIR is isolation",
        {"base.sh": BASELINE, "inline.sh": 'SMIX_MACHINE_DIR="$M" smix sim register x --udid emulator-5554 --kind emulator\n'},
        0,
        "clean",
    ),
    (
        "picking the first attached device is named",
        {"base.sh": BASELINE, "pick.sh": 'UDID="$(cargo run -q -p smix-usbmux --example first_device)"\n'},
        1,
        "pick.sh:1: looks for a physical device",
    ),
    (
        "an adb filter that drops emulators is a physical lookup",
        {"base.sh": BASELINE, "adb.sh": "S=\"$(adb devices | awk '$1 !~ /^emulator-/ {print $1}')\"\n"},
        1,
        "adb.sh:1",
    ),
    (
        "a device picked out of the devicectl listing is named",
        {"base.sh": BASELINE, "dc.sh": 'UDID="$(xcrun devicectl list devices | head -1)"\n'},
        1,
        "dc.sh:1",
    ),
    (
        "a real-looking Apple UDID literal is named",
        {"base.sh": BASELINE, "lit.sh": "X=00008120-001410C11A42201E\n"},
        1,
        "lit.sh:1: 00008120-001410C11A42201E",
    ),
    (
        "a real-looking Android serial registered as a phone is named",
        {
            "base.sh": BASELINE,
            "ser.sh": 'e2e_isolate_machine "$W"\nsmix sim register c --udid R5XX00000AA --kind physical-android\n',
        },
        1,
        "ser.sh:2: R5XX00000AA",
    ),
    (
        "a command named inside a multi-line message runs nothing",
        {
            "base.sh": BASELINE,
            "msg.sh": 'cannot_judge "not registered — register one first:\n  smix sim register a --udid emulator-5554 --kind emulator"\n',
        },
        0,
        "clean",
    ),
    (
        "a suite loop that does not clear the phone variables is named",
        {"base.sh": BASELINE, "tier.sh": 'for e in scripts/dev/*-e2e.sh; do bash "$e"; done\n'},
        1,
        "tier.sh:1: runs every e2e script",
    ),
    (
        "clearing them after the loop does not count",
        {
            "base.sh": BASELINE,
            "late.sh": 'for e in scripts/dev/*-e2e.sh; do bash "$e"; done\nunset SMIX_E2E_PHYSICAL_ANDROID SMIX_E2E_PHYSICAL_IOS\n',
        },
        1,
        "late.sh:1",
    ),
    (
        "smix down given a flag is named",
        {"base.sh": BASELINE, "td.sh": 'd="$("$SMIX" down --device "$U" 2>&1)" || true\n'},
        1,
        "td.sh:1: `smix down` takes no flags",
    ),
    (
        "runner down with flags is fine",
        {"base.sh": BASELINE, "rd.sh": '"$SMIX" runner down --device "$U" --runner-port "$P"\n'},
        0,
        "clean",
    ),
    (
        "a rule with no compliant member anywhere is reported",
        {"only.sh": 'e2e_isolate_machine "$W"\nsmix sim register a --udid emulator-5554 --kind emulator\n'},
        1,
        "no script has a consented-discovery",
    ),
]


def main() -> int:
    fails = 0
    for label, files, want_rc, want in CASES:
        rc, out = run(files)
        if rc != want_rc or want not in out:
            fails += 1
            print(f"FAIL {label}: exit {rc} (wanted {want_rc}), output lacks {want!r}:\n{out}")
    # And the floor: a tree with fewer scripts than asked for is red.
    with tempfile.TemporaryDirectory() as root:
        os.makedirs(os.path.join(root, "scripts"))
        p = subprocess.run(
            [sys.executable, GATE, "--root", root, "--min-scripts", "5"],
            capture_output=True,
            text=True,
        )
        if p.returncode != 1 or "fewer than 5" not in p.stdout:
            fails += 1
            print(f"FAIL an emptied scripts/ reads as clean: exit {p.returncode}\n{p.stdout}")
    if fails:
        print(f"an-e2e-leaves-the-phones-alone.test: FAIL ({fails})")
        return 1
    print(f"an-e2e-leaves-the-phones-alone.test: {len(CASES) + 1} cases pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
