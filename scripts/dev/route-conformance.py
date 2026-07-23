#!/usr/bin/env python3
"""Check that no shipped source calls a runner route no runner serves.

Exits non-zero when one does, so it can gate a release.

Three of the four published SDKs shipped against a wire that never existed —
`/sim/launch` where the runner takes `/session/launch-app`, `/a11y/snapshot`
for `/tree`, `/input/tap` for `/tap`, and `/sim/screenshot` for a route that
does not exist at all, since screenshots are taken out of band. Thirteen
endpoints each, the same thirteen: the SDKs were ported from each other and
inherited one fiction. Every one of their tests injects a mock client, so
they proved the SDK agreed with itself and nothing compared the strings to
the runners' route tables.

This does the comparing.

The two registries are read from the runners' own source rather than
restated here, and the file list comes from git rather than a list of SDKs.
Both for the same reason: an enumeration written by hand is how the defect
survived. It was found while sweeping comments, and the entry recording it
said "two SDKs" — because whoever wrote the entry enumerated three
directories and forgot the Swift one.

The runners serve different routes on purpose, and that is not a defect. iOS
resolves selectors inside the runner and acts in one call (`/find`, `/tap`,
`/fill`); Android resolves on the host and acts by coordinate
(`/tap-at-norm-coord`, `/input-text`). So a route has to be served by *a*
runner, not by both.

Usage:
    python3 scripts/dev/route-conformance.py [--verbose]
"""

import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

# Areas that speak to something other than a runner. Each is printed on every
# run with its hit count, so a narrowed scope is stated rather than assumed.
EXCLUDED_AREAS = [
    ("crates/smix-server/", "the live-stream server, which has its own HTTP API"),
    ("dashboard/", "the dashboard SPA's client-side routes"),
    ("web/", "the marketing site's client-side routes"),
    (
        "swift-bridge/Tests/SmixIndigoHIDTests/",
        "dlsym fixtures name absolute paths that look like routes",
    ),
    (
        "crates/smix-selector/tests/perf_contract.rs",
        "names directories in this repo",
    ),
    (
        "crates/smix-ai-tier/tests/verdict.rs",
        "names a binary that is deliberately absent",
    ),
    (
        "swift-bridge/Tests/SmixRunnerCoreTests/RestartLoopTests.swift",
        "registers ad-hoc /ping + /bounce handlers on a test server to "
        "exercise the restart loop's grace flush — fixtures, not runner routes",
    ),
]

# Literals that are filesystem paths. A route never lives under these.
EXCLUDED_PREFIXES = [
    ("/usr/", "filesystem"),
    ("/bin/", "filesystem"),
    ("/dev/", "filesystem"),
    ("/tmp/", "filesystem"),
]

SOURCE_EXTS = (".rs", ".ts", ".tsx", ".kt", ".swift")
SKIP_PREFIXES = ("target/", "build/", ".build/", "node_modules/")

# `server.appendRoute("POST /tap")`, `("POST /select/resolve", .resolve)`
SWIFT_ROUTE = re.compile(r'"(?:GET|POST) (/[^"]+)"')
# `uri == "/tap-by-id" && session.method == Method.POST`
KOTLIN_ROUTE = re.compile(r'uri == "(/[a-z0-9][a-z0-9/_-]*)"')

# A route-shaped literal in either quote style.
ROUTE_LITERAL = re.compile(r"""['"](/[a-z0-9][a-zA-Z0-9/_-]*)['"]""")


def tracked_files():
    """Every file a reader gets: tracked, plus new ones not gitignored."""
    out = subprocess.run(
        ["git", "ls-files", "-c", "-o", "--exclude-standard"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return [
        rel
        for rel in out.stdout.splitlines()
        if rel and not rel.startswith(SKIP_PREFIXES) and rel.endswith(SOURCE_EXTS)
    ]


def read(rel):
    try:
        return open(os.path.join(ROOT, rel), encoding="utf-8").read()
    except (OSError, UnicodeDecodeError):
        return ""


def served_routes():
    """Every route either runner registers, read from their source."""
    routes = set()
    ios = os.path.join(ROOT, "swift-bridge/Sources/SmixRunnerCore")
    for name in sorted(os.listdir(ios)):
        if name.endswith(".swift"):
            routes |= set(SWIFT_ROUTE.findall(read(f"swift-bridge/Sources/SmixRunnerCore/{name}")))
    for rel in tracked_files():
        if rel.startswith("android-runner/app/src") and rel.endswith(".kt"):
            routes |= set(KOTLIN_ROUTE.findall(read(rel)))
    return routes


def excluded_area(rel):
    for prefix, _ in EXCLUDED_AREAS:
        if rel.startswith(prefix):
            return prefix
    return None


def main():
    verbose = "--verbose" in sys.argv
    served = served_routes()
    if len(served) < 30:
        print(
            f"route-conformance: read only {len(served)} routes from the runners — "
            "the registries moved and this check would pass by knowing nothing",
            file=sys.stderr,
        )
        return 2

    violations = {}
    owed = {prefix: 0 for prefix, _ in EXCLUDED_AREAS}
    for rel in tracked_files():
        area = excluded_area(rel)
        text = read(rel)
        for lineno, line in enumerate(text.splitlines(), 1):
            for match in ROUTE_LITERAL.finditer(line):
                route = match.group(1)
                if route in served:
                    continue
                if any(route.startswith(p) for p, _ in EXCLUDED_PREFIXES):
                    continue
                if area is not None:
                    owed[area] += 1
                    continue
                violations.setdefault(route, []).append(f"{rel}:{lineno}")

    for prefix, reason in EXCLUDED_AREAS:
        print(f"route-conformance: {owed[prefix]:4d} left in {prefix} — not checked ({reason})")

    if not violations:
        print(
            f"route-conformance: clean — every route reaches one of the "
            f"{len(served)} the runners serve"
        )
        return 0

    total = sum(len(v) for v in violations.values())
    print(
        f"\nroute-conformance: {len(violations)} route(s) no runner serves, "
        f"in {total} place(s)\n"
    )
    for route in sorted(violations):
        callers = violations[route]
        print(f"  {route}")
        for caller in callers if verbose else callers[:3]:
            print(f"      {caller}")
        if not verbose and len(callers) > 3:
            print(f"      … and {len(callers) - 3} more")
    print(
        "\nThe runners serve: " + ", ".join(sorted(served)),
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
