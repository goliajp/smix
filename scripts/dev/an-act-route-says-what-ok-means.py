#!/usr/bin/env python3
"""Every POST route on either runner says what its `ok` means, and that
value reaches the host.

Two defects sit behind this gate, and they are different from each
other.

The first is answering the wrong question. `/back` answered
`UiDevice.pressBack()`, whose bytecode is
`sendKeyAndWaitForEvent(KEYCODE_BACK, 0, TYPE_WINDOW_CONTENT_CHANGED,
1000)` — "somebody's window changed within a second", which the status
bar's clock satisfies and a slow navigation does not. A consumer got
`ok:false` twice with the screenshot showing the screen had gone back.

The second is quieter: `/tap-at-norm-coord`, `/swipe-at-norm-coord` and
`/swipe-once` computed the right boolean and then wrote it into a
`status` string that nothing on the host reads. `OkEnvelope` looks for
`ok` and treats its absence as success, so an injection that never
happened arrived as a passing tap. **The answer existed and was thrown
away.**

So each POST handler declares, in one line,

    // OK MEANS: <kind> — <sentence>

with kind one of:

  injected          the input events were dispatched; whether the app
                    reacted is the caller's next assertion
  outcome           something was read back afterwards and it is what
                    the caller asked about
  action-performed  an accessibility action reported that it ran
  reading           the route answers a question about the screen; it
                    acts on nothing
  bookkeeping       the route keeps the runner's own records; there is
                    no device act

and a handler declaring `injected` or `outcome` must build its response
through a `*Body` builder that actually puts `ok` on the wire.

Not a style rule. A new route cannot be added without someone writing
down which of those five its `ok` is, and the wire check is the half
that would have caught the three routes above.

The iOS runner answers the same questions through FlyingFox's
`appendRoute`, and for a version this gate could not see it: it parsed
Kotlin, so half the product was outside what it judged — a gate covering
one of two platforms while reading as covering the thing. The Swift half
below reads the same declaration, with the same five kinds, from the
same file the routes are registered in.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_ROUTES = ROOT / "android-runner/app/src/androidTest/kotlin/dev/smix/runner/RunnerTest.kt"
DEFAULT_WIRE = ROOT / "android-runner/app/src/main/kotlin/dev/smix/runner/RunnerWire.kt"
DEFAULT_SWIFT = ROOT / "swift-bridge/Sources/SmixRunnerCore/SmixRunnerServer.swift"

KINDS = ("injected", "outcome", "action-performed", "reading", "bookkeeping")
# The kinds whose value is an answer about the device, and so has to
# reach the host rather than stopping at a `status` string.
KINDS_NEEDING_OK = ("injected", "outcome")

TABLE = re.compile(
    r'uri == "(?P<uri>[^"]+)"\s*&&\s*session\.method == Method\.(?P<method>GET|POST)'
    r"\s*->\s*(?P<fn>serve\w+)"
)
DEFN = re.compile(r"private fun (serve\w+)\s*\(")
ELSE_BRANCH = re.compile(r"else -> (serve\w+)")
DECL = re.compile(r"//\s*OK MEANS:\s*(?P<kind>[a-z-]+)\s*(?P<sep>[—-])\s*(?P<why>.+)")
BODY_CALL = re.compile(r"RunnerWire\.(\w*Body)\s*\(")


def fail(problems):
    print("an-act-route-says-what-ok-means: NOT CLEAN", file=sys.stderr)
    for p in problems:
        print(f"  - {p}", file=sys.stderr)
    return 1


def comment_block_start(source, pos):
    """Walk back over the comment lines written above a declaration.

    A handler with an expression body (`private fun x(): Response =
    …`) has no brace to put a line inside, so its declaration goes
    above it — and a slice that started at `private fun` would file
    that line under the PREVIOUS handler, which is how a gate comes to
    read one route's note as another's.
    """
    start = source.rfind("\n", 0, pos) + 1
    while start > 0:
        prev_end = start - 1
        prev_start = source.rfind("\n", 0, prev_end) + 1
        line = source[prev_start:prev_end].strip()
        if line.startswith("//") or line.startswith("*") or line.startswith("/*"):
            start = prev_start
            continue
        break
    return start


def function_bodies(source):
    """Each `private fun name(...)` mapped to its text.

    Bounded by the next declaration at the same indentation, which is
    how this file is laid out throughout, and extended backwards over
    the comment block that belongs to it.
    """
    bodies = {}
    starts = [(comment_block_start(source, m.start()), m.group(1)) for m in DEFN.finditer(source)]
    for i, (pos, name) in enumerate(starts):
        end = starts[i + 1][0] if i + 1 < len(starts) else len(source)
        bodies[name] = source[pos:end]
    return bodies


def wire_builders(source):
    """Each `fun nameBody(...)` in RunnerWire mapped to its text."""
    out = {}
    starts = [(m.start(), m.group(1)) for m in re.finditer(r"fun (\w+)\s*\(", source)]
    for i, (pos, name) in enumerate(starts):
        end = starts[i + 1][0] if i + 1 < len(starts) else len(source)
        out[name] = source[pos:end]
    return out


SWIFT_ROUTE = re.compile(r'appendRoute\(\s*"(?P<method>GET|POST) (?P<uri>/[^"]*)"')
# What a Swift route answers with. Two shapes count as a decided
# answer: a `success(ok: …)` carrying the value, and a route that picks
# between two different responses (`success()` or `notFound(…)`) — on
# this runner the verdict is often which object comes back rather than
# what is inside it. What does not count is a single `success()` with
# nothing in it and no alternative: that is `{"ok":true}` written in.
SWIFT_RESPONSE = re.compile(r"(\w+Route)\.(?P<kind>\w+)\((?P<args>[^)]*)\)")


def swift_routes(source):
    """Each `appendRoute("METHOD /uri")` with the comment block above it
    and the closure body below, to the next route."""
    out = []
    marks = [(m.start(), m.group("method"), m.group("uri")) for m in SWIFT_ROUTE.finditer(source)]
    starts = [comment_block_start(source, pos) for pos, _m, _u in marks]
    for i, (_pos, method, uri) in enumerate(marks):
        # Up to the NEXT route's comment block, not to its registration:
        # the line above a route belongs to that route, and a slice that
        # ran to `appendRoute` swallowed it — every route then carried
        # two declarations, its own and its neighbour's.
        end = starts[i + 1] if i + 1 < len(starts) else len(source)
        out.append((method, uri, source[starts[i]:end]))
    return out


def judge_swift(source):
    """The same rule, read off the Swift runner."""
    problems = []
    routes = swift_routes(source)
    if not routes:
        return ["no `appendRoute(\"METHOD /uri\")` found — the Swift reader is blind"], 0, {}
    kinds = {}
    checked = 0
    for method, uri, body in routes:
        if method != "POST":
            continue
        decls = DECL.findall(body)
        if not decls:
            problems.append(
                f"POST {uri} (swift): no `// OK MEANS: <kind> — …` line. "
                f"Say which of {', '.join(KINDS)} its `ok` is."
            )
            continue
        if len(decls) > 1:
            problems.append(f"POST {uri} (swift): {len(decls)} `OK MEANS` lines; exactly one")
            continue
        kind, _sep, why = decls[0]
        if kind not in KINDS:
            problems.append(
                f"POST {uri} (swift): kind `{kind}` is not one of {', '.join(KINDS)}"
            )
            continue
        if not why.strip():
            problems.append(f"POST {uri} (swift): the kind is there and the sentence is not")
            continue
        kinds.setdefault(kind, []).append(uri)
        if kind in KINDS_NEEDING_OK:
            calls = [(m.group("kind"), m.group("args")) for m in SWIFT_RESPONSE.finditer(body)]
            answers = {k for k, _ in calls if k not in {"decode", "DecodeError"}}
            if not calls:
                problems.append(
                    f"POST {uri} (swift): declares `{kind}` and builds no `…Route.…(…)` "
                    f"response, so there is nowhere for that answer to be"
                )
                continue
            checked += 1
            # A response built with something in it carries a verdict —
            # `success(ok: …)`, `outcome(await handler())`,
            # `response(rebound: …)`. `badRequest` is about the request
            # and says nothing about the device, so it neither carries
            # nor counts as an alternative.
            carries = any(
                k != "badRequest" and args.strip() for k, args in calls
            )
            picks = len({k for k in answers if k not in {"badRequest"}}) > 1
            if not carries and not picks:
                problems.append(
                    f"POST {uri} (swift): declares `{kind}`, but it always answers the same "
                    f"way — an `ok` written in rather than one this route decided"
                )
    return problems, checked, kinds


def main(argv):
    # The two files, so the self-test next door can drive this against
    # a mutated copy instead of a second implementation of the rules.
    routes = Path(argv[0]) if len(argv) > 0 else DEFAULT_ROUTES
    wire = Path(argv[1]) if len(argv) > 1 else DEFAULT_WIRE
    problems = []
    routes_src = routes.read_text()
    wire_src = wire.read_text()

    table = [(m.group("uri"), m.group("method"), m.group("fn")) for m in TABLE.finditer(routes_src)]
    defined = {m.group(1) for m in DEFN.finditer(routes_src)}
    fallback = {m.group(1) for m in ELSE_BRANCH.finditer(routes_src)}

    # Anti-vacuity, first half: each reader must have read something.
    # A parse that silently matches nothing looks exactly like a file
    # with nothing wrong in it.
    if not table:
        return fail(["the route table parsed to nothing — this gate read no routes at all"])
    if not defined:
        return fail(["no `private fun serve…` definitions found — the handler reader is blind"])

    # Anti-vacuity, second half: two independent readings of the same
    # file must agree. If either drifts, they stop matching — no
    # hand-copied count of routes to go stale (`§14.8`).
    routed = {fn for _, _, fn in table}
    orphan_handlers = defined - routed - fallback
    dangling = routed - defined
    for fn in sorted(orphan_handlers):
        problems.append(f"`{fn}` is defined but no route reaches it — the two readings disagree")
    for fn in sorted(dangling):
        problems.append(f"the route table names `{fn}`, which is not defined here")

    bodies = function_bodies(routes_src)
    builders = wire_builders(wire_src)

    post = [(uri, fn) for uri, method, fn in table if method == "POST"]
    declared_kinds = {}
    checked_for_ok = 0

    for uri, fn in post:
        body = bodies.get(fn)
        if body is None:
            continue  # already reported as dangling
        decls = DECL.findall(body)
        if not decls:
            problems.append(
                f"POST {uri} → {fn}: no `// OK MEANS: <kind> — …` line. "
                f"Say which of {', '.join(KINDS)} its `ok` is."
            )
            continue
        if len(decls) > 1:
            problems.append(f"POST {uri} → {fn}: {len(decls)} `OK MEANS` lines; exactly one")
            continue
        kind, _sep, why = decls[0]
        if kind not in KINDS:
            problems.append(f"POST {uri} → {fn}: kind `{kind}` is not one of {', '.join(KINDS)}")
            continue
        if not why.strip():
            problems.append(f"POST {uri} → {fn}: the kind is there and the sentence is not")
            continue
        declared_kinds.setdefault(kind, []).append(uri)

        if kind in KINDS_NEEDING_OK:
            called = set(BODY_CALL.findall(body))
            if not called:
                problems.append(
                    f"POST {uri} → {fn}: declares `{kind}` but builds no `*Body` response, "
                    f"so there is nowhere for that answer to be"
                )
                continue
            for builder in sorted(called):
                text = builders.get(builder)
                if text is None:
                    problems.append(f"POST {uri} → {fn}: `RunnerWire.{builder}` is not defined")
                    continue
                checked_for_ok += 1
                # The value has to be handed over, not written in. A
                # builder that hardcodes `ok: true` satisfies a reader
                # looking for the field while answering nothing.
                if not re.search(r"\bok:\s*Boolean", text):
                    problems.append(
                        f"POST {uri} → {fn}: declares `{kind}`, but `RunnerWire.{builder}` "
                        f"takes no `ok: Boolean` — whatever it puts on the wire is not "
                        f"something this route decided"
                    )
                    continue
                if '.put("ok"' not in text:
                    problems.append(
                        f"POST {uri} → {fn}: declares `{kind}`, but `RunnerWire.{builder}` "
                        f'never puts `ok` on the wire — the host reads `ok` and nothing reads '
                        f"`status`, so that answer is computed and thrown away"
                    )

    # A kind that has left the building entirely is worth a word: it
    # means either a whole class of route is gone, or the declarations
    # have drifted into one bucket.
    for kind in KINDS:
        if kind not in declared_kinds:
            problems.append(f"no POST route declares `{kind}` — has a whole class of route left?")

    # Declarations are only meaningful where they are read.
    for fn, body in bodies.items():
        if fn in {f for _, f in post}:
            continue
        if DECL.search(body):
            problems.append(
                f"`{fn}` carries an `OK MEANS` line but serves no POST route — "
                f"a declaration nothing checks"
            )

    swift = Path(argv[2]) if len(argv) > 2 else DEFAULT_SWIFT
    swift_problems, swift_checked, swift_kinds = judge_swift(swift.read_text())
    problems += swift_problems

    if checked_for_ok == 0:
        problems.append(
            "no handler was checked for the `ok` field — the wire half of this gate did nothing"
        )

    if problems:
        return fail(problems)

    kinds = ", ".join(f"{k}:{len(v)}" for k, v in sorted(declared_kinds.items()))
    swift_total = sum(len(v) for v in swift_kinds.values())
    print(
        f"an-act-route-says-what-ok-means: clean — {len(post)} POST route(s) on Android "
        f"declare what their ok means ({kinds}); {checked_for_ok} response builder(s) "
        f"checked for the field the host reads; {swift_total} POST route(s) on iOS "
        f"declare theirs, {swift_checked} of them building a decided answer"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
