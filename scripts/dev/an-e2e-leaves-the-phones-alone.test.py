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
SIMS="$(SMIX_MACHINE_DIR="$WORK/m" "$SMIX" sim list --json)"
LEDGER="$(e2e_ledger_path "$UDID")"
mkdir -p "$W/.smix/leases"
e2e_start_emulator "$WORK/e.log" -avd sim-smix-android-03 -port 5640
e2e_stop_emulator emulator-5640
"$SMIX" down >/dev/null 2>&1 || true
. "$ROOT/scripts/lib/gate-port.sh"
"$SMIX" runner down >/dev/null 2>&1 || true
if [ "$(simulator_state "$UDID")" != Booted ]; then "$SMIX" sim boot "$UDID"; fi
e2e_yield_if_held "$UDID"
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
        "a device listing against the real ledger is named",
        {"base.sh": BASELINE, "list.sh": 'L="$(target/release/smix sim list 2>/dev/null)"\n'},
        1,
        "list.sh:1: lists devices against the machine's real ledger",
    ),
    (
        "a machine-wide down against the real ledger is named",
        {"base.sh": BASELINE, "down.sh": '"$SMIX" down >/dev/null 2>&1 || true\n'},
        1,
        "down.sh:1: runs `smix down` against the machine's real ledger",
    ),
    (
        "a runner stopped on the machine's default port is named",
        {"base.sh": BASELINE, "port.sh": 'if ! out="$("$SMIX" runner down 2>&1)"; then :; fi\n'},
        1,
        "port.sh:1: acts on a runner at the machine's default port",
    ),
    (
        "a port taken here does not reach a runner stopped over ssh",
        {
            "base.sh": BASELINE,
            "ssh.sh": '. "$ROOT/scripts/lib/gate-port.sh"\n'
            "rssh \"cd /r && target/release/smix runner down\" || true\n",
        },
        1,
        "ssh.sh:2: acts on a runner at the machine's default port",
    ),
    (
        "a runner named by device or port is not the default port",
        {
            "base.sh": BASELINE,
            "named.sh": '"$SMIX" runner down --device "$UDID" --runner-port "$P"\n'
            '"$SMIX" runner down --platform android --device "$SERIAL"\n'
            'SMIX_RUNNER_PORT="$P" "$SMIX" runner down\n',
        },
        0,
        "clean",
    ),
    (
        "an emulator started in the script's process group is named",
        {"base.sh": BASELINE, "emu.sh": '"$EMULATOR" -avd x -port 5600 -no-boot-anim > l 2>&1 &\n'},
        1,
        "emu.sh:1: starts an emulator in this script's process group",
    ),
    (
        "an emulator stopped without waiting for it to quit is named",
        {"base.sh": BASELINE, "kill.sh": 'with_deadline 20 adb -s "$S" emu kill >/dev/null 2>&1 || true\n'},
        1,
        "kill.sh:1: stops an emulator without waiting",
    ),
    (
        "an indented stop in a teardown is still a stop",
        {"base.sh": BASELINE, "ind.sh": 'cleanup() {\n    adb -s "$SERIAL" emu kill >/dev/null 2>&1 || true\n}\n'},
        1,
        "ind.sh:2: stops an emulator without waiting",
    ),
    (
        "a launcher killed by signal is named",
        {"base.sh": BASELINE, "sig.sh": '[ -n "$BLOCKER_PID" ] && kill "$BLOCKER_PID" 2>/dev/null\n'},
        1,
        "sig.sh:1: signals an emulator launcher",
    ),
    (
        "the words in a guard test's quoted command are not a stop",
        {"base.sh": BASELINE, "q.sh": 'feed $ALLOW "adb -s emulator-5554 emu kill"\n'},
        0,
        "clean",
    ),
    (
        "a ledger found in the checkout by a relative path is named",
        {"base.sh": BASELINE, "led.sh": 'LEDGER=".smix/leases/$UDID.json"\n'},
        1,
        "led.sh:1: names the checkout's ledger directory",
    ),
    (
        "a multi-line single-quoted program does not hide the lines after it",
        {
            "base.sh": BASELINE,
            "py.sh": "pid=\"$(python3 -c '\nimport os\nprint(os.environ.get(\"X\"))\n' 2>/dev/null)\"\n"
            'L="$(target/release/smix sim list)"\n',
        },
        1,
        "py.sh:5: lists devices against the machine's real ledger",
    ),
    (
        "the registry alone may be read, and a grep for the words is not a call",
        {
            "base.sh": BASELINE,
            "ok.sh": '"$SMIX" sim list --registered --json\n' "grep -q 'smix sim list|UDID' out.txt\n",
        },
        0,
        "clean",
    ),
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
        "a hand-written boot check that can never match is named (v10.2-c12)",
        {"base.sh": BASELINE, "c12.sh": '  if ! xcrun simctl list devices | grep -q "$IOS_UDID (Booted)"; then\n'},
        1,
        "c12.sh:1: reads `simctl list devices` itself",
    ),
    (
        "a grep for UDID.*Booted is named",
        {"base.sh": BASELINE, "g.sh": 'xcrun simctl list devices 2>/dev/null | grep -q "$UDID.*Booted" && WAS=yes\n'},
        1,
        "g.sh:1: reads `simctl list devices` itself",
    ),
    (
        "the remote copy through rssh is named",
        {"base.sh": BASELINE, "r.sh": 'rssh "xcrun simctl list devices 2>/dev/null | grep -q \\"$U.*Booted\\"" && W=yes\n'},
        1,
        "r.sh:1: reads `simctl list devices` itself",
    ),
    (
        "an inline JSON state check is named",
        {"base.sh": BASELINE, "j.sh": 'S="$(xcrun simctl list devices -j | python3 -c "print(1)")"\n'},
        1,
        "j.sh:1: reads `simctl list devices` itself",
    ),
    (
        "the words inside another command's string are not a listing",
        {"base.sh": BASELINE, "w.sh": 'feed $ALLOW "xcrun simctl list devices"\nlog "ran xcrun simctl list devices"\n'},
        0,
        "clean",
    ),
    (
        "a listing that chooses a device is exempt by its line",
        {"base.sh": BASELINE, "pick-dev-sim.sh": 'BOOTED="$(xcrun simctl list devices -j | python3 -c "print(1)")"\n'},
        0,
        "clean",
    ),
    (
        "an exemption whose line is gone is named",
        {"base.sh": BASELINE, "pick-dev-sim.sh": 'echo nothing to choose\n'},
        1,
        "excuses nothing",
    ),
    (
        "no script asking the library is reported",
        {"only.sh": BASELINE.replace('if [ "$(simulator_state "$UDID")" != Booted ]; then "$SMIX" sim boot "$UDID"; fi\n', "")},
        1,
        "no script has a state-asked",
    ),
    (
        "a machine-wide batch sweep is named",
        {"base.sh": BASELINE, "y.sh": "pgrep -f 'runner.ts|smix run|supervise' >/dev/null && cannot_judge \"busy\"\n"},
        1,
        "y.sh:1: yields to anything smix-shaped on the machine",
    ),
    (
        "the sweep's words in a message are not a sweep",
        {"base.sh": BASELINE, "y.sh": 'log "used to run pgrep -f runner.ts|smix run here"\n'},
        0,
        "clean",
    ),
    (
        "no script asking the ledger is reported",
        {"only.sh": BASELINE.replace('e2e_yield_if_held "$UDID"\n', "")},
        1,
        "no script has a ledger-yield",
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
