#!/usr/bin/env python3
"""A refusal that says "devicectl has no verb for it" is checked against
the devicectl that is installed.

`ACTION_PLATFORMS` in crates/smix-sdk/src/device_control.rs refuses a
number of actions on a physical iPhone with a reason of the form
"devicectl cannot" / "devicectl has no verb". Those were true of the
devicectl that shipped with Xcode 26. Xcode 27's devicectl grew
`device capture screenshot`, `device pasteboard copy`, `device simulate
location route` and more — and nothing in the repository would have
noticed, because the reason is a string and strings do not go stale
visibly.

So this reads the PhysicalIos column, and for each action whose refusal
is of that form asks the installed `xcrun devicectl <parent> --help`
whether the verb is there. A verb that exists must be named in the
refusal (as `device <parent-tail> <verb>`): the refusal may stand — smix
may not drive the verb yet — but it may not say the verb is absent.

Where the verb would live is this gate's own table below, one row per
action, and that table is the only copy: it is what turns "devicectl
cannot X" into a question devicectl can answer.

Exit 2 when devicectl cannot be asked (no xcrun, `--help` not
answering, no SUBCOMMANDS section) — an unread devicectl is not a green
one. `xcrun devicectl help device nosuchverb` exits 0, so the exit code
is never the evidence; only the SUBCOMMANDS listing is.

Usage:  a-refusal-devicectl-outgrew.py [repo-root]
        SMIX_DEVICECTL_XCRUN=<path>   the xcrun to ask (the self-test's fake)
Exit:   0 clean · 1 findings · 2 cannot run
"""

import os
import re
import subprocess
import sys

ROOT = os.path.abspath(
    sys.argv[1]
    if len(sys.argv) > 1
    else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
)
TABLE = os.path.join("crates", "smix-sdk", "src", "device_control.rs")
XCRUN = os.environ.get("SMIX_DEVICECTL_XCRUN", "xcrun")

# PhysicalIos is the third column of `[Availability; 4]`, in
# `DeviceKind::ALL` order (Simulator, Emulator, PhysicalIos, PhysicalAndroid).
PHYSICAL_IOS = 2

# Minimum refusals in that column for the read to count as a read: the
# table has had well over this many since physical devices landed.
MIN_REFUSALS = 8

# action → (devicectl parent path, how the verb is recognised, why that
# verb is the counterpart). `("name", verb)` means a subcommand of exactly
# that name; `("contains", word)` means any subcommand whose name contains
# the word. A refusal that speaks of devicectl at all is checked; one that
# does not (`capture_bgra` talks about CoreSimulator's IOSurface,
# `send_push` about APNs, `terminate` about finding a pid) is left alone.
FALSIFIERS = {
    "screenshot": (["device", "capture"], ("name", "screenshot"), "a screenshot is a screenshot"),
    "start_recording": (["device", "capture"], ("name", "screen-record"), "recording the screen is what start_recording begins"),
    "pasteboard_set": (["device", "pasteboard"], ("name", "copy"), "copy puts host data on the device pasteboard"),
    "pasteboard_get": (["device", "pasteboard"], ("name", "paste"), "paste reads the device pasteboard back"),
    "location_set": (["device", "simulate", "location"], ("name", "coordinate"), "one coordinate is a set location"),
    "location_start": (["device", "simulate", "location"], ("name", "route"), "a route is a started location scenario"),
    "set_animations_quiet": (["device", "settings"], ("contains", "anim"), "animations would be a setting"),
    "add_media": (["device"], ("contains", "media"), "media would be its own subcommand"),
    "set_permission": (["device", "settings"], ("contains", "privacy"), "TCC would be a setting"),
}

DEVICECTL_REFUSAL = re.compile(r"devicectl\b.*\b(cannot|has no|no equivalent|no verb)", re.IGNORECASE)


def strip_comments(text: str) -> str:
    return re.sub(r"//[^\n]*", "", text)


def rust_consts(text: str) -> dict:
    return {
        m.group(1): m.group(2)
        for m in re.finditer(r'const\s+(\w+)\s*:\s*&str\s*=\s*"((?:[^"\\]|\\.)*)"', text)
    }


def balanced(text: str, start: int, open_ch: str, close_ch: str) -> int:
    depth = 0
    for i in range(start, len(text)):
        if text[i] == open_ch:
            depth += 1
        elif text[i] == close_ch:
            depth -= 1
            if depth == 0:
                return i
    raise ValueError("unbalanced")


def cells_of(row_body: str) -> list:
    """The four cells of a row body: 'Yes' or the text inside `No { … }`."""
    cells = []
    i = 0
    while i < len(row_body):
        if row_body.startswith("Yes", i):
            cells.append(("Yes", ""))
            i += 3
        elif row_body.startswith("No", i) and "{" in row_body[i : i + 10]:
            open_at = row_body.index("{", i)
            close_at = balanced(row_body, open_at, "{", "}")
            cells.append(("No", row_body[open_at + 1 : close_at]))
            i = close_at + 1
        else:
            i += 1
    return cells


def why_of(cell_body: str, consts: dict) -> str:
    """The refusal's text: a literal, a named const, or — for
    `undriven_devicectl_verb!("…")` — the verb the macro names, which is
    the part this gate reads."""
    m = re.search(
        r'why\s*:\s*(?:"((?:[^"\\]|\\.)*)"|([a-z_][a-z0-9_]*)!\(\s*"((?:[^"\\]|\\.)*)"\s*\)|([A-Z_][A-Z0-9_]*))',
        cell_body,
    )
    if not m:
        return ""
    if m.group(1) is not None:
        return m.group(1)
    if m.group(3) is not None:
        return f"devicectl has `{m.group(3)}`"
    return consts.get(m.group(4), "")


def physical_ios_refusals(source: str) -> dict:
    text = strip_comments(source)
    consts = rust_consts(text)
    start = text.find("ACTION_PLATFORMS")
    if start < 0:
        return {}
    refusals = {}
    for m in re.finditer(r'\(\s*"(\w+)"\s*,\s*\[', text[start:]):
        name = m.group(1)
        body_start = start + m.end() - 1
        body_end = balanced(text, body_start, "[", "]")
        cells = cells_of(text[body_start + 1 : body_end])
        if len(cells) == 4 and cells[PHYSICAL_IOS][0] == "No":
            refusals[name] = why_of(cells[PHYSICAL_IOS][1], consts)
    return refusals


def subcommands_of(parent: list):
    """Names in the SUBCOMMANDS section of `xcrun devicectl <parent> --help`, or None."""
    try:
        out = subprocess.run(
            [XCRUN, "devicectl", *parent, "--help"], capture_output=True, text=True, timeout=60
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    if out.returncode != 0 or "SUBCOMMANDS:" not in out.stdout:
        return None
    section = out.stdout.split("SUBCOMMANDS:", 1)[1]
    names = []
    for line in section.splitlines()[1:]:
        if not line.strip():
            break
        names.append(line.split()[0])
    return names


def main() -> int:
    path = os.path.join(ROOT, TABLE)
    if not os.path.isfile(path):
        print("a-refusal-devicectl-outgrew: CANNOT RUN")
        print(f"  - {TABLE} is not in this tree")
        return 2

    with open(path, encoding="utf-8") as fh:
        refusals = physical_ios_refusals(fh.read())

    problems = []
    if len(refusals) < MIN_REFUSALS:
        problems.append(
            f"read nothing: {len(refusals)} refusal(s) on PhysicalIos in {TABLE}, fewer than "
            f"{MIN_REFUSALS} — this gate is not reading the table it names"
        )
        print("a-refusal-devicectl-outgrew: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1

    version = subprocess.run([XCRUN, "devicectl", "--version"], capture_output=True, text=True).stdout.strip() if os.path.exists(XCRUN) or XCRUN == "xcrun" else ""
    checked = 0
    listings = {}
    for action, (parent, (how, needle), _reason) in FALSIFIERS.items():
        if action not in refusals:
            problems.append(
                f"{action}: this gate expects a refusal on PhysicalIos and the table has none — "
                f"gate and table have drifted apart"
            )
            continue
        why = refusals[action]
        if not DEVICECTL_REFUSAL.search(why) and "devicectl" not in why:
            # The refusal gives a reason that is not about devicectl's verbs.
            continue
        key = tuple(parent)
        if key not in listings:
            listings[key] = subcommands_of(parent)
        names = listings[key]
        if names is None:
            print("a-refusal-devicectl-outgrew: CANNOT RUN")
            print(
                f"  - cannot run `{XCRUN} devicectl {' '.join(parent)} --help` or it printed no "
                f"SUBCOMMANDS section; an unread devicectl is not one that lacks the verb"
            )
            return 2
        checked += 1
        if how == "name":
            found = [n for n in names if n == needle]
        else:
            found = [n for n in names if needle in n.lower()]
        for verb in found:
            spelled = " ".join(parent + [verb])
            if spelled not in why and " ".join(parent[1:] + [verb]) not in why:
                problems.append(
                    f"{action} on PhysicalIos: devicectl has `{spelled}` and the refusal says "
                    f'"{why}" — a refusal on a verb that exists must name it'
                )

    if problems:
        print("a-refusal-devicectl-outgrew: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(
        f"a-refusal-devicectl-outgrew: clean — {len(refusals)} refusals on PhysicalIos, "
        f"{checked} checked against devicectl {version or '(version unread)'}, none outgrown"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
