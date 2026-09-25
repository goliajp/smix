#!/usr/bin/env python3
"""A script under scripts/ touches a phone only when a person named it.

On 2026-09-25 a release dry-run uninstalled smix's runner from the
owner's Samsung, took a screenshot through a runner somebody had up on
the owner's iPhone, tried `runner up` on that iPhone, opened a usbmux
tunnel to it, and listed the apps of a consumer's simulator. No script
did anything it was not written to do. Each of them looked at what was
attached, found a device "registered", and took that as permission —
and the registrations were written into the machine's real ledger by
the same suite, with the owner's device identifiers typed into the
scripts as literals.

"Attached" and "registered" say a device can be reached. Neither says
whose it is or whether anyone agreed. So, for every shell script under
scripts/ (the library that defines the switches excepted):

1. A call that writes the device registry (`sim register`,
   `unregister`, `allow-destructive`, `forbid-destructive`) comes after
   the script isolated its machine directory (`e2e_isolate_machine`, or
   `SMIX_MACHINE_DIR` set for that command or exported before it). The
   real ledger belongs to everyone on the machine.
2. Finding a physical device — usbmux's `first_device` / `forward_probe`,
   `devicectl list devices`, an `adb devices` filter that drops
   `emulator-` — comes after `e2e_physical_or_skip`, which reads a
   variable a person sets and never looks at the bus.
3. A physical device's identifier written as a literal has the
   fabricated shape: an Apple UDID whose sixteen-digit tail starts with
   ten zeros, an Android serial that starts with FAKE. A literal real
   identifier is a device, whatever registry it is written into.
4. A loop that runs every `*-e2e.sh` clears SMIX_E2E_PHYSICAL_* first:
   a suite is a place nobody names a device.
5. `smix down` is not given flags. It takes none — it is the machine-wide
   sweep — and four teardowns wrote `down --device X` meaning `runner down`:
   refused as a usage error inside `2>&1 || true`, so the runner they
   started was never stopped, and nothing said so.

Each rule must also have a member that obeys it, or it is checking
nothing (gate/absence-needs-presence).

Usage:
  scripts/dev/an-e2e-leaves-the-phones-alone.py [--root DIR]
"""

from __future__ import annotations

import argparse
import os
import re
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# 164 shell scripts under scripts/ today, 78 of them device e2e. A walk
# that finds a handful is looking at the wrong directory.
MIN_SCRIPTS = 100

LIBRARY = os.path.join("scripts", "lib", "e2e-devices.sh")

REGISTRY_WRITE = re.compile(
    r"\bsim\s+(register|unregister|allow-destructive|forbid-destructive)\b"
)
ISOLATION = re.compile(r"\be2e_isolate_machine\b|\bSMIX_MACHINE_DIR=")
DISCOVERY = [
    re.compile(r"\bfirst_device\b"),
    re.compile(r"\bforward_probe\b"),
    # Picking a device out of the listing. Asking the listing about a
    # UDID the caller named (scripts/dev/lib/devicectl-e2e-device.sh) is
    # a lookup, not a choice, and is not this.
    re.compile(r"\b\w*(UDID|DEVICE|SERIAL|PHONE)\w*=\"?\$\(\s*xcrun\s+devicectl\s+list\s+devices"),
    re.compile(r"!~\s*/\^emulator-/"),
    # smix-usbmux's live tests run against whatever is on the bus.
    re.compile(r"cargo\s+test\s+-p\s+smix-usbmux\s+--test\s+live\b"),
]
CONSENT = re.compile(r"\be2e_physical_or_skip\b")
# `smix down` with anything after it: a usage error that a teardown swallows.
DOWN_WITH_ARGS = re.compile(r"""(\$SMIX|\bsmix)"?\s+down\s+--""")
# A loop that runs every e2e script is a place nobody names a device.
SUITE_LOOP = re.compile(r"\bfor\s+\w+\s+in\s+.*\*-e2e\.sh")
CLEARED = re.compile(
    r"\bunset\b.*\bSMIX_E2E_PHYSICAL_ANDROID\b.*\bSMIX_E2E_PHYSICAL_IOS\b"
)
APPLE_UDID = re.compile(r"\b[0-9A-Fa-f]{8}-([0-9A-Fa-f]{16})\b")
ANDROID_LITERAL = re.compile(r"--udid\s+['\"]?([A-Za-z0-9._-]+)['\"]?")
# A line that only says something: the words may name a command without
# running it ("register it: smix sim register …").
SPEECH = re.compile(r"^\s*(log|echo|printf|step|fail|bad|ok|cannot_judge|note|say)\b")


def unescaped_quotes(line: str) -> int:
    """Double quotes that open or close a string, outside single quotes."""
    count, in_single, prev = 0, False, ""
    for ch in line:
        if ch == "'" and not in_single and prev != "\\":
            in_single = True
        elif ch == "'" and in_single:
            in_single = False
        elif ch == '"' and not in_single and prev != "\\":
            count += 1
        prev = ch
    return count


def code_lines(text: str) -> list[tuple[int, str]]:
    """Lines that run, with heredoc bodies, comments and the continuation
    lines of a multi-line string left out — `cannot_judge "… \n smix sim
    register …"` names a command in a message and runs nothing."""
    out: list[tuple[int, str]] = []
    heredoc_end: str | None = None
    in_string = False
    for n, line in enumerate(text.splitlines(), 1):
        if heredoc_end is not None:
            if line.strip() == heredoc_end:
                heredoc_end = None
            continue
        quotes = unescaped_quotes(line)
        if in_string:
            if quotes % 2 == 1:
                in_string = False
            continue
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if quotes % 2 == 1:
            in_string = True
        m = re.search(r"<<-?\s*['\"]?(\w+)['\"]?", line)
        if m:
            heredoc_end = m.group(1)
        out.append((n, line))
    return out


def fabricated_apple(tail: str) -> bool:
    return tail.upper().startswith("0000000000")


def fabricated_android(serial: str) -> bool:
    return serial.upper().startswith("FAKE") or serial.startswith("$")


def scan(text: str, rel: str, seen: dict[str, int]) -> list[str]:
    problems: list[str] = []
    isolated_at: int | None = None
    consent_at: int | None = None
    cleared_at: int | None = None
    for n, line in code_lines(text):
        if CLEARED.search(line):
            cleared_at = cleared_at or n
        if SUITE_LOOP.search(line):
            if cleared_at is not None:
                seen["cleared-suite"] += 1
            else:
                problems.append(
                    f"{rel}:{n}: runs every e2e script without first clearing "
                    f"SMIX_E2E_PHYSICAL_* — a caller's environment would name a phone "
                    f"for a whole suite"
                )
        if ISOLATION.search(line) and not REGISTRY_WRITE.search(line):
            isolated_at = isolated_at or n
        if DOWN_WITH_ARGS.search(line):
            problems.append(
                f"{rel}:{n}: `smix down` takes no flags — this is a usage error, and in a "
                f"teardown it leaves the runner running; `runner down --device … --runner-port …` "
                f"stops one runner"
            )
        if CONSENT.search(line):
            consent_at = consent_at or n
        speech = bool(SPEECH.match(line))
        if REGISTRY_WRITE.search(line) and not speech:
            if "SMIX_MACHINE_DIR=" in line or isolated_at is not None:
                seen["isolated-write"] += 1
            else:
                problems.append(
                    f"{rel}:{n}: writes the device registry with no isolated machine "
                    f"directory — this is the ledger every smix on the machine reads"
                )
        if any(p.search(line) for p in DISCOVERY) and not speech:
            if consent_at is not None:
                seen["consented-discovery"] += 1
            else:
                problems.append(
                    f"{rel}:{n}: looks for a physical device before e2e_physical_or_skip — "
                    f"being attached is not consent"
                )
        for m in APPLE_UDID.finditer(line):
            if fabricated_apple(m.group(1)):
                seen["fabricated-id"] += 1
            else:
                problems.append(
                    f"{rel}:{n}: {m.group(0)} is written as a literal and does not have the "
                    f"fabricated shape (tail starting 0000000000) — it may be a real device"
                )
        if "physical-android" in line:
            for m in ANDROID_LITERAL.finditer(line):
                if fabricated_android(m.group(1)):
                    seen["fabricated-id"] += 1
                else:
                    problems.append(
                        f"{rel}:{n}: {m.group(1)} is registered as a physical Android device "
                        f"and is not a FAKE… serial — it may be a real device"
                    )
    return problems


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=REPO)
    ap.add_argument("--min-scripts", type=int, default=MIN_SCRIPTS)
    args = ap.parse_args()
    root = os.path.abspath(args.root)
    scripts = []
    for d, _, files in os.walk(os.path.join(root, "scripts")):
        for f in files:
            p = os.path.join(d, f)
            if f.endswith(".sh") and os.path.relpath(p, root) != LIBRARY:
                scripts.append(p)
    name = "an-e2e-leaves-the-phones-alone"
    if len(scripts) < args.min_scripts:
        print(f"{name}: FAIL")
        print(
            f"  - {len(scripts)} shell scripts under scripts/, fewer than "
            f"{args.min_scripts} — a scan that lost its subject reads like a clean one"
        )
        return 1
    seen = {
        "isolated-write": 0,
        "consented-discovery": 0,
        "fabricated-id": 0,
        "cleared-suite": 0,
    }
    problems: list[str] = []
    for p in sorted(scripts):
        with open(p, encoding="utf-8", errors="replace") as fh:
            problems += scan(fh.read(), os.path.relpath(p, root), seen)
    for rule, count in seen.items():
        if count == 0:
            problems.append(
                f"no script has a {rule} — the rule it stands for matched nothing, "
                f"so it is not being checked"
            )
    if problems:
        print(f"{name}: FAIL — {len(problems)} place(s)")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(
        f"{name}: clean — {len(scripts)} scripts; {seen['isolated-write']} registry "
        f"write(s) isolated, {seen['consented-discovery']} physical lookup(s) behind a "
        f"named device, {seen['fabricated-id']} device literal(s) fabricated, "
        f"{seen['cleared-suite']} suite loop(s) clearing SMIX_E2E_PHYSICAL_*"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
