#!/usr/bin/env python3
"""Self-test for `an-act-route-says-what-ok-means`.

Each case mutates a copy of the two real files and requires the gate to
go red on that mutation, naming it. An assertion that has never been
red is an assertion nobody has verified, and a gate is the worst place
to keep one.

The baseline case is the other half: the gate must be green on the
files as they are, or every red below proves nothing.
"""

import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GATE = ROOT / "scripts/dev/an-act-route-says-what-ok-means.py"
ROUTES = ROOT / "android-runner/app/src/androidTest/kotlin/dev/smix/runner/RunnerTest.kt"
WIRE = ROOT / "android-runner/app/src/main/kotlin/dev/smix/runner/RunnerWire.kt"


def run(routes_text, wire_text):
    with tempfile.TemporaryDirectory() as d:
        r = Path(d) / "RunnerTest.kt"
        w = Path(d) / "RunnerWire.kt"
        r.write_text(routes_text)
        w.write_text(wire_text)
        p = subprocess.run(
            [sys.executable, str(GATE), str(r), str(w)],
            capture_output=True,
            text=True,
        )
        return p.returncode, p.stdout + p.stderr


def main():
    routes = ROUTES.read_text()
    wire = WIRE.read_text()
    failures = []

    def case(name, routes_text, wire_text, expect_clean, expect_phrase=None):
        code, out = run(routes_text, wire_text)
        clean = code == 0
        if clean != expect_clean:
            failures.append(f"{name}: expected {'clean' if expect_clean else 'red'}, got:\n{out}")
            return
        if expect_phrase and expect_phrase not in out:
            failures.append(f"{name}: red for the wrong reason — no '{expect_phrase}' in:\n{out}")

    # The files as they are. Without this the mutations below are
    # measuring nothing.
    case("the tree as it stands", routes, wire, expect_clean=True)

    # A route with no declaration at all.
    stripped = routes.replace(
        "        // OK MEANS: injected — `UiDevice.click` is `clickNoSync`, which", "", 1
    )
    stripped = re.sub(
        r"^        // is `touchDown`/`touchUp`.*\n(        // .*\n)*", "", stripped, count=1, flags=re.M
    )
    case(
        "a POST route with no OK MEANS line",
        stripped,
        wire,
        expect_clean=False,
        expect_phrase="/tap-at-norm-coord",
    )

    # A kind nobody defined.
    case(
        "a kind that is not one of the five",
        routes.replace("// OK MEANS: injected — `UiDevice.click`", "// OK MEANS: injekted — `UiDevice.click`", 1),
        wire,
        expect_clean=False,
        expect_phrase="is not one of",
    )

    # The defect this gate was built for: the value is computed and
    # then written where nothing reads it.
    case(
        "a body builder that stops putting ok on the wire",
        routes,
        wire.replace(
            '''            .put("ok", ok)
            .put("status", if (ok) "ok" else "click_returned_false")''',
            '''            .put("status", if (ok) "ok" else "click_returned_false")''',
            1,
        ),
        expect_clean=False,
        expect_phrase="thrown away",
    )

    # A builder that writes a constant instead of an answer.
    case(
        "a body builder that hardcodes ok instead of being handed it",
        routes,
        wire.replace(
            "fun swipeOnceBody(ok: Boolean, direction: String, q: SwipeQuad): String = JSONObject()\n"
            '        .put("ok", ok)',
            "fun swipeOnceBody(direction: String, q: SwipeQuad): String = JSONObject()\n"
            '        .put("ok", true)',
            1,
        ),
        expect_clean=False,
        expect_phrase="takes no `ok: Boolean`",
    )

    # The instrument's own blindness: a file it cannot parse must be
    # red, not quietly clean.
    case("a routes file with nothing in it", "", wire, expect_clean=False, expect_phrase="read no routes")

    # Two readings of one file that no longer agree.
    case(
        "a handler the route table no longer reaches",
        routes.replace('uri == "/back" && session.method == Method.POST -> serveBack()', "", 1),
        wire,
        expect_clean=False,
        expect_phrase="serveBack",
    )

    # A declaration where nothing checks it.
    case(
        "an OK MEANS line on a route that serves no POST",
        routes.replace(
            "    private fun serveTree(session: IHTTPSession): Response {",
            "    private fun serveTree(session: IHTTPSession): Response {\n        // OK MEANS: outcome — nothing checks this line.",
            1,
        ),
        wire,
        expect_clean=False,
        expect_phrase="a declaration nothing checks",
    )

    # A whole class of route disappearing.
    case(
        "every injected route reclassified",
        routes.replace("// OK MEANS: injected —", "// OK MEANS: bookkeeping —"),
        wire,
        expect_clean=False,
        expect_phrase="has a whole class of route left",
    )

    if failures:
        print("an-act-route-says-what-ok-means.test: NOT CLEAN", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print(
        "an-act-route-says-what-ok-means.test: a missing declaration, an unknown kind, "
        "an answer left off the wire, a hardcoded ok, an unreadable file, a handler nothing "
        "routes to, a declaration nothing checks, and a vanished class are each judged as "
        "they should be"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
