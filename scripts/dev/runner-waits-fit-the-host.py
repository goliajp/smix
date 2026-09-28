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
- the burst defaults the host assumes are the runners' own;
- the iOS server's own handler limit is the table's, which the table's
  tests hold above every wait the host makes;
- an Android route that polls is allowed one look past each poll
  (`.looks(n)`, `ANDROID_LOOK_MS` each), and n is the number of polls its
  handler reaches: `Poll.until` returns only after a look begun past its
  budget, and on a loaded emulator one look has taken seconds. The runner's
  statement is the total, and so is the limit it keeps to itself
  (`RouteLimits`), for every route that keeps one.

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
ANDROID_MAIN = "android-runner/app/src/main/kotlin/dev/smix/runner"
ROUTE_LIMITS = ANDROID_MAIN + "/RouteLimits.kt"

# Fewer than this many routes parsed means the parse broke, not that the
# runner shrank; an empty reading must not pass as "nothing disagrees".
MIN_ROUTES = {"ios": 30, "android": 30}
MIN_TABLE_ROWS = 40
# Android routes that poll today; fewer found means the call graph was not
# read, and a route with no polls found would pass with no looks allowed.
MIN_POLLING_ROUTES = 5

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


def table(text: str) -> dict[str, dict]:
    """path → {"ios": "req"/ms/None, "android": …, "looks": n}."""
    consts = {"XCUI_APP_CALL_MS": int(re.search(r"XCUI_APP_CALL_MS: u64 = ([\d_]+)", text).group(1).replace("_", ""))}
    rows: dict[str, dict] = {}

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
    row = r'\br\(\s*"(/[^"]+)",\s*([^,]+?),\s*([^,]+?),\s*(true|false),?\s*\)(?:\s*\.looks\(\s*(\d+)\s*\))?'
    # a test may build a row of its own; only the table's are the table
    for m in re.finditer(row, code_only(text.split("#[cfg(test)]")[0])):
        rows[m.group(1)] = {
            "ios": side(m.group(2)),
            "android": side(m.group(3)),
            "looks": int(m.group(5) or 0),
        }
    return rows


def look_ms(text: str) -> int:
    m = re.search(r"ANDROID_LOOK_MS: u64 = ([\d_]+)", text)
    if not m:
        raise ValueError("ANDROID_LOOK_MS is not there")
    return int(m.group(1).replace("_", ""))


def android_total(row: dict, look: int) -> str | None:
    """What the Android runner states: its own waits and one look per poll."""
    own = row["android"]
    if own is None or own == "req":
        return own
    return str(int(own) + row["looks"] * look)


def kotlin_functions(text: str) -> dict[str, str]:
    """Each `fun name(` → its body (a block, or an expression up to a blank line)."""
    out: dict[str, str] = {}
    for m in re.finditer(r"\bfun\s+(?:<[^>]*>\s*)?(?:[A-Za-z_.]+\.)?([A-Za-z_]\w*)\s*\(", text):
        i, depth = m.end(), 1
        while depth and i < len(text):
            depth += {"(": 1, ")": -1}.get(text[i], 0)
            i += 1
        k = i
        while k < len(text) and text[k] not in "{=":
            k += 1
        if k >= len(text):
            continue
        if text[k] == "=":
            end = text.find("\n\n", k)
            body = text[k : end if end > 0 else len(text)]
        else:
            depth, e = 0, k
            while e < len(text):
                depth += {"{": 1, "}": -1}.get(text[e], 0)
                if depth == 0:
                    break
                e += 1
            body = text[k : e + 1]
        out.setdefault(m.group(1), body)
    return out


def polls_in(body: str) -> int:
    """`Poll.until` calls, and hand-written loops that sleep between looks."""
    n = len(re.findall(r"\bPoll\.until\b", body))
    for m in re.finditer(r"\bwhile\s*\(", body):
        i = body.find("{", m.end())
        depth, e = 0, i
        while 0 <= e < len(body):
            depth += {"{": 1, "}": -1}.get(body[e], 0)
            if depth == 0:
                break
            e += 1
        if i >= 0 and re.search(r"\b(?:Thread|SystemClock)\.sleep\b", body[i:e]):
            n += 1
    return n


def android_polls(root: str) -> dict[str, int]:
    """Each Android route → the polls its handler reaches through calls.

    Poll.kt is left out: its loop is the one each `Poll.until` stands for."""
    text = code_only(read(root, ANDROID))
    funcs = kotlin_functions(text)
    main = os.path.join(root, ANDROID_MAIN)
    for name in sorted(os.listdir(main)):
        if name.endswith(".kt") and name != "Poll.kt":
            for k, v in kotlin_functions(code_only(read(root, os.path.join(ANDROID_MAIN, name)))).items():
                funcs.setdefault(k, v)

    def reached(start: str) -> set[str]:
        seen: set[str] = set()
        todo = [start]
        while todo:
            f = todo.pop()
            if f in seen or f not in funcs:
                continue
            seen.add(f)
            todo += [g for g in re.findall(r"\b([a-z]\w*)\s*\(", funcs[f]) if g in funcs]
        return seen

    out: dict[str, int] = {}
    for path, handler in re.findall(r'uri == "(/[^"]+)" && session\.method == Method\.\w+ ->\s*(\w+)\(', text):
        out[path] = sum(polls_in(funcs[f]) for f in reached(handler))
    return out


def route_limits(text: str) -> dict[str, str]:
    return {p: v.replace("_", "") for p, v in re.findall(r'"(/[^"]+)" to ([\d_]+)L', code_only(text))}


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
        look = look_ms(read(root, TABLE))
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
            want = row[platform] if platform == "ios" else android_total(row, look)
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

    polls = android_polls(root)
    polling = {p: n for p, n in polls.items() if n}
    if len(polling) < MIN_POLLING_ROUTES:
        problems.append(
            f"android: found polls in only {len(polling)} routes — the call graph was not "
            f"read, and a route with none found would pass with no looks allowed"
        )
    for p, n in sorted(polls.items()):
        row = rows.get(p)
        if row is not None and row["android"] is not None and row["looks"] != n:
            problems.append(
                f"{p}: the android handler reaches {n} poll(s) and {TABLE} allows "
                f"{row['looks']} look(s) past them — each poll can finish one look "
                f"after its budget"
            )

    limits_text = read(root, ROUTE_LIMITS)
    limits = route_limits(limits_text)
    clocked = set(re.findall(r'\brouteClock\("(/[^"]+)"\)', code_only(read(root, ANDROID))))
    if not clocked:
        problems.append("android: no route keeps a clock (routeClock) — the reading broke")
    for p in sorted(clocked - set(limits)):
        problems.append(f"android: {p} keeps a clock and {ROUTE_LIMITS} gives it no limit")
    for p in sorted(set(limits) - clocked):
        problems.append(f"{ROUTE_LIMITS}: {p} has a limit no route keeps")
    for p, v in sorted(limits.items()):
        row = rows.get(p)
        want = android_total(row, look) if row else None
        if want != v:
            problems.append(
                f"{p}: {ROUTE_LIMITS} keeps to {v} ms and {TABLE} says {want} — the "
                f"runner would stop on another limit than the host waits by"
            )

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
    # The iOS server answers 500 in a handler's place at its own limit; the
    # table's figure is checked there against every wait the host makes.
    server_rust = re.search(r"SERVER_HANDLER_TIMEOUT_MS: u64 = ([\d_]+)", table_text)
    server_swift = re.search(r"handlerTimeoutSeconds: TimeInterval = ([\d.]+)", read(root, IOS))
    if not server_rust or not server_swift:
        problems.append("could not read the iOS server's handler limit from the table or the Swift server")
    elif int(server_rust.group(1).replace("_", "")) != int(float(server_swift.group(1)) * 1000):
        problems.append(
            f"the iOS server lets a handler run {server_swift.group(1)} s and the table says "
            f"{server_rust.group(1)} ms — the server would cut off waits the host is still making"
        )
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
