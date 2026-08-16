#!/usr/bin/env python3
"""No script in this tree drives "whichever device happens to be there".

Two release gates took the first emulator adb listed, and one day that
was somebody else's; six smoke scripts defaulted to emulator-5554 and
stopped it on teardown. Nobody did anything wrong. Nothing asked whose
device it was, and the machine had two people on it.

The rule is the one pick-dev-sim.sh and pick-dev-emulator.sh embody:
a script that acts on a device either asks the ledger which one is
smix's, or is handed a serial by the caller and says so. It does not
scan `adb devices` for the first match, and it does not fall back to a
port number.

Two halves. Accidental selection must be absent; a deliberate resolver
or an explicit env must be present in every script that touches a
device. A script that touches no device is not this gate's business and
says so by not matching the touch patterns at all.
"""

import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
# `SMIX_GATE_ROOT` lets the harness point this at a fixture tree. Without
# it the only tree this gate has ever judged is the one it lives in, and
# every accident it exists to catch has been removed from that tree.
REPO = os.path.abspath(os.environ.get("SMIX_GATE_ROOT") or os.path.join(HERE, "..", ".."))

SCAN_DIRS = ["scripts/dev", "scripts/release", "android-runner/scripts"]

# Touching an Android device.
TOUCHES = re.compile(r"\badb\s+(-s\s+\S+\s+)?(shell|install|emu|forward|uninstall|push|pull)\b")

# The accidents.
ACCIDENTS = [
    (re.compile(r"adb\s+devices[^\n]*\|\s*awk[^\n]*emulator"), "scans `adb devices` for the first emulator"),
    (re.compile(r"adb\s+devices[^\n]*\|\s*(head|grep)[^\n]*emulator"), "scans `adb devices` for an emulator"),
    (re.compile(r":-emulator-5554\b"), "falls back to emulator-5554"),
    (re.compile(r"^\s*SERIAL=[\"']?emulator-\d+"), "hard-codes a serial"),
]

# What a deliberate choice looks like.
DELIBERATE = [
    re.compile(r"pick-dev-emulator\.sh"),
    re.compile(r"\bSMIX_ANDROID_SERIAL\b|\bANDROID_SERIAL\b|\bADB_SERIAL\b"),
    # `smix sim resolve <alias>` asks the registry; `"$SMIX" sim resolve`
    # is the same call through a variable, and the first version of this
    # pattern saw only the bare word and called the C1 e2e — which does
    # exactly the right thing — an accident.
    re.compile(r"sim\s+(boot|resolve)\s+\S"),
    # Sourcing the shared emulator lifecycle: its `smoke_emulator_up`
    # boots through smix on a registered alias and sets $SERIAL from
    # that. A script that takes its device from the lifecycle has
    # chosen it exactly as deliberately as one that calls smix itself.
    re.compile(r"lib/emulator-lifecycle\.sh"),
]

# Scripts whose subject IS the accident — a guard test feeding it inputs
# is not committing it. Each says why.
NOT_A_SUBJECT = {
    "adb-guard.test.sh": "feeds adb command lines to the guard under test; it runs none of them",
    "hook-command.test.py": "same — the strings are the guard's inputs, not commands",
    "no-script-picks-a-device-by-accident.test.py": "this gate's own harness: its fixtures ARE the accidents, written down to be refused",
    "v6.1-c5-two-devices-one-is-not-yours-e2e.sh": "two devices are its subject: it starts one by hand and reads `adb devices` to prove it stays up after a refusal, never to pick one to drive — ours comes from `sim resolve`",
}


def main() -> int:
    problems, touched, careful = [], 0, 0
    for d in SCAN_DIRS:
        base = os.path.join(REPO, d)
        if not os.path.isdir(base):
            continue
        for name in sorted(os.listdir(base)):
            if not (name.endswith(".sh") or name.endswith(".py")):
                continue
            if name in NOT_A_SUBJECT:
                continue
            path = os.path.join(base, name)
            with open(path, encoding="utf-8") as fh:
                body = fh.read()
            code = "\n".join(l for l in body.splitlines() if not l.lstrip().startswith("#"))
            if not TOUCHES.search(code):
                continue
            touched += 1
            rel = f"{d}/{name}"
            hits = [why for pat, why in ACCIDENTS if pat.search(code)]
            for why in hits:
                problems.append(f"{rel} {why} — a device is either the ledger's answer or the caller's, never the first one adb lists")
            if not hits and not any(p.search(code) for p in DELIBERATE):
                problems.append(f"{rel} touches a device and neither asks the ledger nor takes a serial from the caller — where does its device come from?")
            elif not hits:
                careful += 1

    if touched == 0:
        problems.append("no script touches a device — the touch pattern stopped matching and this gate is reading air")

    if problems:
        print("no-script-picks-a-device-by-accident: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(f"no-script-picks-a-device-by-accident: clean — {touched} scripts touch a device, all {careful} choose it deliberately")
    return 0


if __name__ == "__main__":
    sys.exit(main())
