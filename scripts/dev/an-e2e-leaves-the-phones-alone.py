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
6. `smix sim list` (other than `--registered`, which reads the registry
   alone) runs against an isolated machine directory. Against the real
   ledger it reads the release of every registered Android phone — the
   owner's Samsung is registered there — so a listing is a command sent
   to a phone (SL2, 2026-09-25).
7. A ledger is found by asking smix (`e2e_ledger_path`), not at a relative
   `.smix/leases/`. That is the checkout's old book, no longer written
   since 4.0: v2.3-c7 asserted `runner up` had recorded a session by
   finding an August file there, and rewrote it to play a live holder
   (2026-09-25). A scratch workspace built on purpose (`$W/.smix/leases`)
   is not this.
8. An emulator is started by hand through `e2e_start_emulator` and
   stopped through `e2e_stop_emulator`. Started with a plain `&` it joins
   the script's process group, and whatever ends that group signals the
   launcher; stopped with a bare `emu kill` the script moves on while it
   is still quitting; killed by pid (`kill "$…PID"`) it aborts. Each of
   these left a "quit unexpectedly" dialog on the owner's desktop
   (2026-09-25, `skin_winsys_quit_request` from `_sigtramp`).
9. `smix down` runs against an isolated machine directory, and a
   `runner up` / `runner down` names its port or device or runs in a
   script that took a port of its own (`gate-port.sh`). `smix down`
   settles every device the ledger says smix booted: against the real one
   it shut down the simulator and emulator a release was using
   (2026-09-25). 22087 is every smix's default port on this machine.

10. Whether a simulator is up is asked through `simulator_state` (the
   library), never by reading `simctl list devices` in the script. Twenty
   scripts carried their own copy of that question; one of them grepped
   for `UDID (Booted)`, which simctl never prints (it is `(UDID) (Booted)`),
   so it always found the device down, recorded that it had booted it, and
   shut the release's simulator on its way out — every script after it
   that did not boot its own then found no device and reported it could
   not judge (2026-09-25, v10.2-c12). A listing that chooses a device or
   reads something other than a device's state is a different question and
   is exempted by line, with its reason.
11. A script stands down for a device by asking the ledger who holds it
   (`e2e_yield_if_held`), never by sweeping process names
   (`pgrep -f 'runner.ts|smix run|supervise'`). The sweep answered "is
   anything smix-shaped running anywhere": a consumer's batch on their own
   emulator kept eight scripts from running for a whole night, though they
   drive our devices on ports of their own (2026-09-26).

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

# 122 shell scripts under scripts/ on 2026-09-25 (the library excepted). A
# walk that finds a handful is looking at the wrong directory.
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
# A device listing, run: the binary names it, and the words are not
# inside a quoted pattern (`grep 'smix sim list|UDID'` reads text).
SIM_LIST = re.compile(r"""(\$SMIX"?|(?<![`'"])\bsmix)\s+sim\s+list\b(?!.*--registered)""")
# The checkout's old ledger directory named from the working directory.
CHECKOUT_LEDGER = re.compile(r"""(^|[\s"'=(])\.smix/leases\b""")
LEDGER_ASKED = re.compile(r"\be2e_ledger_path\b")
# An emulator launched in the script's own process group.
HAND_START = re.compile(r"""(\$EMULATOR|/emulator/emulator)"?\s+-avd\b""")
# `emu kill` issued as a command (not quoted inside another command's text).
BARE_KILL = re.compile(r"""(^\s*|[;&|(]\s*|\bwith_deadline\s+\S+\s+)adb\s+-s\s+\S+\s+emu\s+kill\b""")
# A launcher pid signalled directly.
LAUNCHER_KILL = re.compile(r"""\bkill\s+(-\w+\s+)?"?\$\{?\w*(BLOCKER|EMU|AVD|EMULATOR|LAUNCHER)\w*PID\b""")
EMULATOR_HELPERS = re.compile(r"\be2e_(start|stop)_emulator\b")
# `smix down` with nothing after it but redirections: the machine-wide one.
MACHINE_DOWN = re.compile(r"""(\$SMIX"?|\bsmix)\s+down\b(?!\s+--)""")
RUNNER_UPDOWN = re.compile(r"""(\$SMIX"?|(?<![`'"])\bsmix)\s+runner\s+(up|down)\b""")
PORT_NAMED = re.compile(r"--runner-port\b|--device\b|SMIX_RUNNER_PORT=|--help\b")
OWN_PORT = re.compile(r"\bgate-port\.sh\b")
# Remote federation nodes: the roster addresses a node's runner at the
# default port, so these runners are on it by design (open-items FED1).
DEFAULT_PORT_BY_DESIGN = {
    "scripts/dev/v2.12-c3-federation-single-node-e2e.sh": "remote node; the federation roster addresses its runner at the default port (FED1)",
    "scripts/dev/v2.12-c4-federation-two-node-e2e.sh": "remote node; the federation roster addresses its runner at the default port (FED1)",
    "scripts/dev/v2.12-c5-federation-cli-e2e.sh": "remote node; the federation roster addresses its runner at the default port (FED1)",
}
# `simctl list devices` run as a command (not words inside another
# command's string): at the start of a line or a pipeline, after `$(`, an
# `if`/`!`, or as the remote half of `rssh`.
SIMCTL_LISTING = re.compile(
    r"""(^\s*|[;&|(!]\s*|\$\(\s*|\bif\s+!?\s*|\brssh\s+"?)xcrun\s+simctl\s+list\s+devices\b"""
)
STATE_ASKED = re.compile(r"\bsimulator_state\b")
# Yielding by process name: `pgrep` over a batch owner's command text.
BATCH_SWEEP = re.compile(r"""\bpgrep\s+-f\s+['"][^'"]*\b(runner\.ts|smix run|supervise)\b""")
LEDGER_YIELD = re.compile(r"\be2e_yield_if_held\b")
# Listings that ask a different question than "is this one up", each named
# by the text of its line. Every entry must match a line, or it is excusing
# nothing and has to go.
LISTING_NOT_A_STATE = {
    ("scripts/dev/pick-dev-sim.sh", 'BOOTED="$(xcrun simctl list devices'):
        "chooses among booted smix simulators by name; the answer is a device, not a state",
    ("scripts/dev/v3.1-c2-machine-lease-e2e.sh", 'DEVICES="$(xcrun simctl list devices'):
        "looks for any busy simulator to stand in as an occupied one",
    ("scripts/dev/v2.3-c15-addressability-e2e.sh", 'SIM_UDID="$(xcrun simctl list devices'):
        "chooses an available simulator nobody registered",
    ("scripts/dev/v2.14-c1-fill-replaces-e2e.sh", 'UDID="$(xcrun simctl list devices'):
        "chooses an available sim-smix-* simulator when none is named",
    ("scripts/dev/v6.1-c5-two-devices-one-is-not-yours-e2e.sh", 'THEIRS_IOS="$(xcrun simctl list devices'):
        "chooses a shut-down sim-smix-* to stand in for somebody else's",
    ("scripts/release/corpus-gate.sh", 'SIM_RUNTIME="$(xcrun simctl list devices'):
        "reads the device's runtime for the log, not whether it is up",
}
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


def code_lines(text: str) -> list[tuple[int, str]]:
    """Lines that start outside any string, heredoc or comment.

    The quoting state is carried across lines. It was counted a line at a
    time, which let a multi-line `python3 -c '…'` program — whose body has
    double quotes of its own — flip the state and hide every command after
    it: v2.12-c5's two `sim list` calls were invisible to this gate.
    """
    out: list[tuple[int, str]] = []
    heredoc_end: str | None = None
    quote: str | None = None
    for n, line in enumerate(text.splitlines(), 1):
        if heredoc_end is not None:
            if line.strip() == heredoc_end:
                heredoc_end = None
            continue
        starts_in_code = quote is None
        stripped = line.strip()
        if starts_in_code and (not stripped or stripped.startswith("#")):
            continue
        prev = ""
        for ch in line:
            if quote is None:
                if ch == "#" and (prev == "" or prev.isspace()):
                    break
                if ch in ("'", '"') and prev != "\\":
                    quote = ch
            elif ch == quote and (quote == "'" or prev != "\\"):
                quote = None
            prev = ch
        if starts_in_code:
            m = re.search(r"<<-?\s*['\"]?(\w+)['\"]?", line)
            if m and quote is None:
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
    own_port = bool(OWN_PORT.search(text))
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
        if SIM_LIST.search(line) and not speech:
            if "SMIX_MACHINE_DIR=" in line or isolated_at is not None:
                seen["isolated-list"] += 1
            else:
                problems.append(
                    f"{rel}:{n}: lists devices against the machine's real ledger — "
                    f"`sim list` reads every registered phone's release; run it with "
                    f"SMIX_MACHINE_DIR isolated, or read the registry with --registered"
                )
        if MACHINE_DOWN.search(line) and not speech:
            if "SMIX_MACHINE_DIR=" in line or isolated_at is not None:
                seen["isolated-down"] += 1
            else:
                problems.append(
                    f"{rel}:{n}: runs `smix down` against the machine's real ledger — it "
                    f"settles every device that ledger says smix booted, whoever is using it"
                )
        if RUNNER_UPDOWN.search(line) and not speech and not PORT_NAMED.search(line):
            # An exported port does not cross ssh: a remote smix gets its own default.
            if own_port and not re.search(r"\brssh\b|\bssh\s", line):
                seen["own-port"] += 1
            elif rel in DEFAULT_PORT_BY_DESIGN:
                seen["default-port-by-design"] = seen.get("default-port-by-design", 0) + 1
                seen.setdefault("exempt-hit", set()).add(rel)
            else:
                problems.append(
                    f"{rel}:{n}: acts on a runner at the machine's default port (22087, "
                    f"every smix's) — name --runner-port / --device, or take a port with gate-port.sh"
                )
        if STATE_ASKED.search(line):
            seen["state-asked"] += 1
        if LEDGER_YIELD.search(line):
            seen["ledger-yield"] += 1
        if BATCH_SWEEP.search(line) and not speech:
            problems.append(
                f"{rel}:{n}: yields to anything smix-shaped on the machine — ask who holds "
                f"this script's device with e2e_yield_if_held (scripts/lib/e2e-devices.sh)"
            )
        if SIMCTL_LISTING.search(line) and not speech:
            key = next(
                (k for k in LISTING_NOT_A_STATE if k[0] == rel and k[1] in line), None
            )
            if key is not None:
                seen.setdefault("listing-exempt-hit", set()).add(key)
            else:
                problems.append(
                    f"{rel}:{n}: reads `simctl list devices` itself to learn a device's "
                    f"state — ask simulator_state (scripts/lib/e2e-devices.sh); a copy of "
                    f"that question is how v10.2-c12 shut the release's simulator"
                )
        if EMULATOR_HELPERS.search(line):
            seen["emulator-helper"] += 1
        if HAND_START.search(line) and not speech:
            problems.append(
                f"{rel}:{n}: starts an emulator in this script's process group — "
                f"whatever ends the script signals it; use e2e_start_emulator"
            )
        if BARE_KILL.search(line) and not speech:
            problems.append(
                f"{rel}:{n}: stops an emulator without waiting for it to quit — "
                f"use e2e_stop_emulator"
            )
        if LAUNCHER_KILL.search(line) and not speech:
            problems.append(
                f"{rel}:{n}: signals an emulator launcher — it aborts and leaves a crash "
                f"dialog; stop it with e2e_stop_emulator"
            )
        if LEDGER_ASKED.search(line):
            seen["ledger-asked"] += 1
        if CHECKOUT_LEDGER.search(line) and not speech:
            problems.append(
                f"{rel}:{n}: names the checkout's ledger directory — smix stopped "
                f"writing there at 4.0, so what is found is old; ask with e2e_ledger_path"
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
        "ledger-yield": 0,
        "isolated-write": 0,
        "consented-discovery": 0,
        "fabricated-id": 0,
        "cleared-suite": 0,
        "isolated-list": 0,
        "ledger-asked": 0,
        "emulator-helper": 0,
        "isolated-down": 0,
        "own-port": 0,
        "state-asked": 0,
    }
    problems: list[str] = []
    for p in sorted(scripts):
        with open(p, encoding="utf-8", errors="replace") as fh:
            problems += scan(fh.read(), os.path.relpath(p, root), seen)
    listing_hit = seen.pop("listing-exempt-hit", set())
    for key, why in LISTING_NOT_A_STATE.items():
        if key not in listing_hit and os.path.isfile(os.path.join(root, key[0])):
            problems.append(
                f"{key[0]} is exempted from the simulator-state rule for "
                f"`{key[1]}` ({why}) and no line of it matches — the exemption "
                f"excuses nothing; remove it"
            )
    hit = seen.pop("exempt-hit", set())
    by_design = seen.pop("default-port-by-design", 0)
    for rel in DEFAULT_PORT_BY_DESIGN:
        exists = os.path.isfile(os.path.join(root, rel))
        if exists and rel not in hit:
            problems.append(
                f"{rel} is exempted from the default-port rule and runs no runner "
                f"up/down at the default port — the exemption excuses nothing; remove it"
            )
        elif not exists and root == REPO:
            problems.append(f"{rel} is exempted from the default-port rule and does not exist")
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
        f"{seen['cleared-suite']} suite loop(s) clearing SMIX_E2E_PHYSICAL_*, "
        f"{seen['isolated-list']} device listing(s) isolated, "
        f"{seen['ledger-asked']} ledger(s) asked of smix, "
        f"{seen['emulator-helper']} emulator start/stop(s) through the helpers, "
        f"{seen['isolated-down']} machine-wide down(s) isolated, "
        f"{seen['own-port']} runner up/down(s) on a port of the script's own, "
        f"{seen['state-asked']} simulator state(s) asked of the library, "
        f"{by_design} on the default port by design"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
