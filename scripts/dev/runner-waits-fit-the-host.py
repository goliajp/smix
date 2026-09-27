#!/usr/bin/env python3
"""No runner route can take longer than the host waits for it.

The host waited 15 s for every route while some routes' own waits added up
to more: an Android `/input-text` of long text, an iOS `/hide-keyboard` on a
slow simulator, an iOS `/back` with 20.5 s of settles. Each time the host
gave up while the runner was still working and reported a step that "may
have acted". Three times, found one at a time, in CI.

So each route's longest wait is written once, in
`crates/smix-runner-client/src/route_limits.rs`, and the host's wait is
derived from it. Each runner states the same number beside the route:

    // LONGEST WAIT /back: 20500 ms — ...
    // LONGEST WAIT /input-text: from the request — ...

This gate holds the three against each other:

- every route a runner registers carries exactly one statement, and every
  statement names a route that runner registers;
- the table has the route for that platform, with the same number (or
  both say "from the request");
- the table lists nothing a runner does not register;
- every route the host calls is in the table;
- a route whose wait the request sets is sent by a method that passes its
  own wait (`json_post_within`), since the table gives it none;
- the burst defaults the host assumes are the runners' own.

A statement can still be wrong about its handler; the number is read from
the code by a person. What this makes impossible is the host and the
runner disagreeing about it without a red.
"""

from __future__ import annotations

import argparse
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

TABLE = "crates/smix-runner-client/src/route_limits.rs"
HOST_DIR = "crates/smix-runner-client/src"
IOS = "swift-bridge/Sources/SmixRunnerCore/SmixRunnerServer.swift"
ANDROID = "android-runner/app/src/androidTest/kotlin/dev/smix/runner/RunnerTest.kt"
TIMELINE = "swift-bridge/Sources/SmixRunnerCore/TouchTimeline.swift"
BURST = "android-runner/app/src/main/kotlin/dev/smix/runner/TapBurst.kt"

# Fewer than this many routes parsed means the parse broke, not that the
# runner shrank; an empty reading must not pass as "nothing disagrees".
MIN_ROUTES = {"ios": 30, "android": 30}
MIN_TABLE_ROWS = 40

STATEMENT = re.compile(r"//\s*LONGEST WAIT (/[^:\s]+):\s*(from the request|(\d+) ms)")


def read(root: str, rel: str) -> str:
    with open(os.path.join(root, rel), encoding="utf-8") as fh:
        return fh.read()


def code_only(text: str) -> str:
    out = []
    for line in text.split("\n"):
        if line.strip().startswith("//"):
            continue
        out.append(re.sub(r"//.*", "", line))
    return "\n".join(out)


def statements(text: str) -> tuple[dict[str, str], list[str]]:
    """Each stated route → "req" or its milliseconds; and repeats."""
    said: dict[str, str] = {}
    twice: list[str] = []
    for m in STATEMENT.finditer(text):
        path, value = m.group(1), ("req" if m.group(3) is None else m.group(3))
        if path in said:
            twice.append(path)
        said[path] = value
    return said, twice


def ios_routes(text: str) -> set[str]:
    code = code_only(text)
    found = set(re.findall(r'appendRoute\("(?:GET|POST) (/[^"]+)"\)', code))
    # routes registered from a list of ("METHOD /path", …) pairs
    found |= set(re.findall(r'\("(?:GET|POST) (/[^"]+)",', code))
    return found


def android_routes(text: str) -> set[str]:
    return set(re.findall(r'uri == "(/[^"]+)" && session\.method', code_only(text)))


def table(text: str) -> dict[str, dict[str, str | None]]:
    """path → {"ios": "req"/ms/None, "android": …}."""
    consts = {"XCUI_APP_CALL_MS": int(re.search(r"XCUI_APP_CALL_MS: u64 = ([\d_]+)", text).group(1).replace("_", ""))}
    rows: dict[str, dict[str, str | None]] = {}

    def side(expr: str) -> str | None:
        expr = expr.strip()
        if expr == "NONE":
            return None
        if expr == "REQ":
            return "req"
        m = re.fullmatch(r"ms\((.+)\)", expr)
        if not m:
            raise ValueError(f"cannot read {expr!r}")
        inner = m.group(1)
        for k, v in consts.items():
            inner = inner.replace(k, str(v))
        inner = inner.replace("_", "")
        if not re.fullmatch(r"[\d\s*+]+", inner):
            raise ValueError(f"cannot read {expr!r}")
        return str(eval(inner))  # digits, * and + only, checked above

    # rustfmt wraps a long row over several lines, so whitespace is free
    for m in re.finditer(r'\br\(\s*"(/[^"]+)",\s*([^,]+?),\s*([^,]+?),\s*(true|false),?\s*\)', code_only(text)):
        rows[m.group(1)] = {"ios": side(m.group(2)), "android": side(m.group(3))}
    return rows


def host_calls(root: str) -> dict[str, str]:
    """Each path the host sends to → the text of the file that sends it."""
    calls: dict[str, str] = {}
    d = os.path.join(root, HOST_DIR)
    for name in sorted(os.listdir(d)):
        if not name.endswith(".rs") or name == os.path.basename(TABLE):
            continue
        text = code_only(read(root, os.path.join(HOST_DIR, name)).split("#[cfg(test)]")[0])
        for path in re.findall(r'"(/[a-z][a-zA-Z0-9/_-]*)"', text):
            calls.setdefault(path, text)
    return calls


def sent_with_its_own_wait(path: str, text: str) -> bool:
    for m in re.finditer(re.escape(f'"{path}"'), text):
        before = text[max(0, m.start() - 200) : m.start()]
        call = re.findall(r"\.(json_post_within|json_post|json_get|send_with_retry)\s*\(", before)
        if call and call[-1] == "json_post_within":
            return True
    return False


def judge(root: str) -> list[str]:
    problems: list[str] = []
    try:
        rows = table(read(root, TABLE))
    except (ValueError, AttributeError) as e:
        return [f"{TABLE} could not be read: {e}"]
    if len(rows) < MIN_TABLE_ROWS:
        return [
            f"{TABLE}: read only {len(rows)} routes — the reading broke, and a short "
            f"table would pass for the routes it lost"
        ]
    sides = {
        "ios": (read(root, IOS), ios_routes),
        "android": (read(root, ANDROID), android_routes),
    }
    for platform, (text, registered_of) in sides.items():
        registered = registered_of(text)
        said, twice = statements(text)
        if len(registered) < MIN_ROUTES[platform]:
            problems.append(
                f"{platform}: read only {len(registered)} registered routes — the "
                f"reading broke, and an empty reading would pass"
            )
        for p in twice:
            problems.append(f"{platform}: {p} states its longest wait twice")
        for p in sorted(registered - set(said)):
            problems.append(f"{platform}: {p} is registered and states no longest wait")
        for p in sorted(set(said) - registered):
            problems.append(f"{platform}: states a longest wait for {p}, which it does not register")
        for p in sorted(registered):
            row = rows.get(p)
            if row is None:
                problems.append(f"{platform}: {p} is registered and missing from {TABLE}")
                continue
            want = row[platform]
            if want is None:
                problems.append(f"{TABLE}: {p} says {platform} has no such route, and it does")
            elif p in said and said[p] != want:
                problems.append(
                    f"{p}: {platform} says {said[p]} and {TABLE} says {want} — the host "
                    f"derives its wait from the table"
                )
        for p, row in sorted(rows.items()):
            if row[platform] is not None and p not in registered:
                problems.append(f"{TABLE}: {p} is listed for {platform}, which does not register it")

    for path, text in sorted(host_calls(root).items()):
        row = rows.get(path)
        if row is None:
            problems.append(f"the host calls {path}, which {TABLE} does not list")
            continue
        if "req" in (row["ios"], row["android"]) and not sent_with_its_own_wait(path, text):
            problems.append(
                f"{path}: its wait is set by the request, and the host sends it without "
                f"one of its own (json_post_within) — it would get the plain 15 s"
            )

    table_text = read(root, TABLE)
    host_interval = re.search(r"BURST_INTERVAL_MS: u32 = (\d+)", table_text)
    host_hold = re.search(r"BURST_HOLD_MS: u32 = (\d+)", table_text)
    ios_interval = re.search(r"defaultIntervalMs: Int = (\d+)", read(root, TIMELINE))
    ios_hold = re.search(r"defaultHoldMs: Int = (\d+)", read(root, TIMELINE))
    android_interval = re.search(r"INTERVAL_MS: Long = (\d+)", read(root, BURST))
    pairs = [
        ("burst interval", host_interval, ios_interval, "iOS TouchTimeline"),
        ("burst interval", host_interval, android_interval, "Android TapBurst"),
        ("burst hold", host_hold, ios_hold, "iOS TouchTimeline"),
    ]
    for what, host, runner, where in pairs:
        if not host or not runner:
            problems.append(f"could not read the {what} from the host or {where}")
        elif host.group(1) != runner.group(1):
            problems.append(f"{what}: the host assumes {host.group(1)} ms and {where} uses {runner.group(1)} ms")
    return problems


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=ROOT)
    args = ap.parse_args()
    problems = judge(args.root)
    if problems:
        print("runner-waits-fit-the-host: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1
    rows = table(read(args.root, TABLE))
    print(f"runner-waits-fit-the-host: clean — {len(rows)} routes, each runner's statement matches the table the host waits by")
    return 0


if __name__ == "__main__":
    sys.exit(main())
