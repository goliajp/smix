#!/usr/bin/env python3
"""The two ways smix can see a screen must agree, or differ for a named reason.

smix has perceived through the accessibility tree since it existed. On a
Compose app that tree is a projection: the whole UI is one
`AndroidComposeView` and the nodes are synthesised from semantics. v10 adds
an in-process probe that reads the semantics tree itself, and two ways of
seeing one screen is two ways of being wrong about it.

So they are reconciled. Every node either appears on both sides, or its
absence falls under a rule that says why — and each rule has to point at a
real member, because a rule that excludes nothing still prints that it
considered something (§14.7; `fact-scan` carried four such for two majors).

The rules key on MECHANISM, not on names. "The tag starts with
compose_dialog_" would pass by recognising the fixture; "the node belongs
to a Compose root other than the one the opt-in was set on" is the actual
reason, and it holds in an app nobody here has seen.

Usage:
  two-paths-agree.py --device emulator-5554 [--port 22095] [--app dev.smix.fixture]
  two-paths-agree.py --a11y a11y.json --semantics probe.json
  two-paths-agree.py --prove-differences-exhibited ...   (also require each
                                                          rule to match)
"""

import argparse
import json
import os
import re
import subprocess
import time
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _probe_wire  # noqa: E402

problems = []


def sh(*cmd):
    r = subprocess.run(cmd, capture_output=True, text=True)
    return r.stdout if r.returncode == 0 else None


def fetch_a11y(binary, device, port):
    out = sh(binary, "tree", "--device", device, "--port", str(port), "--json")
    if out is None:
        return None
    body = "\n".join(l for l in out.splitlines() if not l.startswith("kevy:"))
    try:
        payload = json.loads(body)
    except json.JSONDecodeError:
        return None
    return unwrap(payload)


def unwrap(payload):
    """Take the tree out of the envelope smix now answers with.

    Since v10 `smix tree --json` emits `{"source": …, "root": …}`. A reader
    that walks the envelope as if it were the tree finds no `identifier`
    anywhere and reports every tag as missing from the accessibility side —
    which is what this did for the length of one checkpoint while its own
    self-test stayed green, because the payloads it was recorded from
    pre-dated the envelope. Recorded fixtures go stale in silence; the
    shape is asserted below rather than assumed.
    """
    if isinstance(payload, dict) and "root" in payload and "children" not in payload:
        return payload["root"]
    return payload


def fetch_semantics(device, app):
    # Through the shared reader: three gates each carried this parse, and
    # all three broke the day the probe added a field beside the tree.
    return _probe_wire.probe_tree(device, app)


def a11y_tags(tree):
    """Every resource id the accessibility side carries, with its node."""
    found = {}

    def walk(n):
        i = n.get("identifier")
        if i:
            found[str(i)] = n
        for c in n.get("children") or []:
            walk(c)

    walk(tree)
    return found


def compose_area(sem_roots):
    """The rectangles the app's Compose roots occupy.

    The accessibility tree is device-wide: the status bar, the navigation
    bar and every other app's window are in it, and the probe will never
    see any of them because it only knows this app's Compose. Comparing
    the two sets whole reports fifty system ids as missing from the probe,
    which is true and useless.

    So the subject is scoped by geometry rather than by name: what lies
    inside a Compose root is what both sides are supposed to be describing.
    A `status_bar` id sits outside every root; `compose_input` does not.
    """
    return [tuple(r["bounds"]) for r in sem_roots if "bounds" in r]


def contains_any(node, rects):
    """The node wraps a whole Compose root — it is a host, not a peer.

    A Compose root hangs inside real Views: the decor view, the activity's
    content frame. Those carry ids, are in the accessibility tree, and are
    not semantics nodes because they are not Compose. Told apart from a
    genuine disagreement by which way the containment runs: an ancestor
    holds the root, a missing sibling sits in it.
    """
    b = node.get("bounds") or {}
    try:
        x, y = float(b["x"]), float(b["y"])
        w, h = float(b["w"]), float(b["h"])
    except (KeyError, TypeError, ValueError):
        return False
    return any(x <= l and y <= t and x + w >= r and y + h >= bo
               for (l, t, r, bo) in rects)


def inside_any(node, rects):
    b = node.get("bounds") or {}
    try:
        x, y = float(b["x"]), float(b["y"])
        w, h = float(b["w"]), float(b["h"])
    except (KeyError, TypeError, ValueError):
        return False
    cx, cy = x + w / 2, y + h / 2
    return any(l <= cx <= r and t <= cy <= bo for (l, t, r, bo) in rects)


def semantics_tags(roots):
    """Every testTag the semantics side carries, with its node and root index."""
    found = {}

    def walk(n, root_index):
        t = n.get("testTag")
        if t:
            found[t] = (n, root_index)
        for c in n.get("children") or []:
            walk(c, root_index)

    for i, root in enumerate(roots):
        walk(root, i)
    return found


# ---------------------------------------------------------------------------
# The named differences. Each one says what it is about, and how to tell.
#
# `applies` decides from the DATA, not from a name — a rule that recognises
# the fixture is a rule that only holds here.
# ---------------------------------------------------------------------------

def in_secondary_root(tag, sem, a11y):
    """A Compose root other than the one the opt-in was set on.

    `Modifier.semantics { testTagsAsResourceId = true }` is a property of the
    subtree it is written on. A dialog, bottom sheet or popup composes into
    its OWN root, which that subtree does not reach — so its tags arrive on
    the semantics side and nowhere on the accessibility side. Measured:
    with the fixture's dialog open the probe reports two roots, and the
    button in root 0 is absent from the accessibility tree entirely.
    """
    node_root = sem.get(tag)
    return node_root is not None and len(set(i for _, i in sem.values())) > 1 \
        and node_root[1] != _primary_root(sem)


def _primary_root(sem):
    """The root holding the most tags — the screen, as opposed to a popup."""
    counts = {}
    for _, i in sem.values():
        counts[i] = counts.get(i, 0) + 1
    return max(counts, key=counts.get) if counts else 0


# Empty, on purpose, and this is the interesting part.
#
# The first draft carried a `secondary-compose-root` rule for dialogs. Driving
# it showed the rule could not be exhibited without also destroying what it
# was an exception TO: with a Compose dialog open, the accessibility path does
# not lose the dialog, it loses THE WHOLE APP — 16 compose ids one second,
# zero the next, while the probe still reports 17 and `smix find
# id:compose_submit` answers `exists=false` about a button plainly on screen.
#
# That is not a difference to be excused; it is the defect this version
# exists to close. So the rule is gone rather than kept as an exemption that
# excludes the empty set while printing that it considered something — which
# is what its own failure message told us to do.
#
# When a real, exhibitable difference turns up, it goes here WITH a screen
# that produces it.
RULES = []


def reconcile(a11y_tree, sem_roots, prove):
    if not isinstance(a11y_tree, dict) or "children" not in a11y_tree:
        problems.append(
            "the accessibility payload is not a tree — it has no `children`. "
            "If the wire grew an envelope again, `unwrap` is where that is "
            "known, and the recorded fixtures need re-recording with it"
        )
        return 0, {}
    a11y = a11y_tags(a11y_tree)
    sem = semantics_tags(sem_roots)

    both = set(a11y) & set(sem)
    only_sem = set(sem) - set(a11y)
    only_a11y = set(a11y) - set(sem)

    matched = {name: 0 for name, _, _ in RULES}
    for tag in sorted(only_sem):
        why = next((n for n, _, f in RULES if f(tag, sem, a11y)), None)
        if why is None:
            problems.append(
                f"`{tag}` is on the semantics side and not the accessibility "
                f"side, and no rule says why"
            )
        else:
            matched[why] += 1

    area = compose_area(sem_roots)
    outside = 0
    for tag in sorted(only_a11y):
        if not inside_any(a11y[tag], area) or contains_any(a11y[tag], area):
            outside += 1
            continue
        problems.append(
            f"`{tag}` is on the accessibility side, sits inside a Compose "
            f"root, and is not on the semantics side — the probe is the one "
            f"that should see more, not less"
        )
    # The presence half of the geometric scoping: if nothing landed outside,
    # the rectangles are wrong (the whole device is not inside one Compose
    # root) and the scoping is excusing by accident rather than by shape.
    if only_a11y and outside == 0:
        problems.append(
            "every accessibility id fell inside a Compose root — the system "
            "bars did too, so the root rectangles are not what they claim"
        )

    if prove:
        for name, why, _ in RULES:
            if matched[name] == 0:
                problems.append(
                    f"the rule `{name}` matched nothing on this screen — it "
                    f"excuses the empty set while printing that it considered "
                    f"something. Drive a screen that exhibits it, or drop the "
                    f"rule ({why})"
                )

    return len(both), matched


def superset(a11y_tree, sem_roots):
    """Whatever the accessibility path can see, the probe can see too.

    Holds on every screen state, which is what makes it worth asserting:
    with a Compose dialog open the accessibility path sees nothing of the
    app and this is trivially true; once that is fixed it is still true;
    and it goes red the day the probe starts missing something, which is
    the failure nobody would otherwise notice.
    """
    a11y = a11y_tags(a11y_tree)
    sem = set(semantics_tags(sem_roots))
    area = compose_area(sem_roots)
    # Scoped the same way as the reconciliation: system bars are not the
    # probe's to see.
    theirs = {
        t for t, n in a11y.items()
        if inside_any(n, area) and not contains_any(n, area)
    }
    missing = sorted(theirs - sem)
    for t in missing:
        problems.append(
            f"the accessibility path sees `{t}` inside a Compose root and the "
            f"probe does not — the probe reads the tree the other one is "
            f"projected FROM, so this is the probe being wrong"
        )
    if problems:
        report()
        return 1
    print(
        f"two-paths-agree: the probe sees all {len(theirs)} of the "
        f"accessibility path's in-root tags, and {len(sem) - len(theirs)} more"
    )
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--device")
    ap.add_argument("--port", default="22095")
    ap.add_argument("--app", default="dev.smix.fixture")
    ap.add_argument("--binary", default="./target/release/smix")
    ap.add_argument("--a11y")
    ap.add_argument("--semantics")
    ap.add_argument("--prove-differences-exhibited", action="store_true")
    ap.add_argument(
        "--min-both", type=int, default=1,
        help="how many tags must appear on BOTH sides. A count rather than "
             "'more than none': a screen that quietly stopped rendering half "
             "of itself still has some.",
    )
    ap.add_argument(
        "--superset-only", action="store_true",
        help="assert only that the probe sees everything the accessibility "
             "path sees. True on every screen state including the ones where "
             "the a11y path has gone blind, so it is the half that keeps "
             "meaning something after the blindness is fixed.",
    )
    args = ap.parse_args()

    # Put the subject on screen before reading it.
    #
    # The other v10 device gates do this and this one did not, so in the
    # ship it would go red on whatever the previous gate happened to leave
    # in front — "only 0 tags on both sides" is true and is about the wrong
    # thing. A gate that depends on the one before it having tidied up is a
    # gate that fails for reasons nobody can act on.
    if args.device and not (args.a11y or args.semantics):
        subprocess.run(["adb", "-s", args.device, "shell", "am", "force-stop", args.app],
                       capture_output=True, text=True)
        subprocess.run(["adb", "-s", args.device, "shell", "am", "start", "-n",
                        f"{args.app}/.ComposeActivity"], capture_output=True, text=True)
        time.sleep(2)

    if args.a11y and args.semantics:
        a11y_tree = unwrap(json.load(open(args.a11y)))
        sem_roots = json.load(open(args.semantics))
    elif args.device:
        a11y_tree = fetch_a11y(args.binary, args.device, args.port)
        sem_roots = fetch_semantics(args.device, args.app)
        # Named separately: "the runner is not up" and "the probe is not in
        # this build" want opposite fixes, and one message for both is the
        # failure this release spent a major on.
        if a11y_tree is None:
            problems.append(
                f"the accessibility tree did not come back — is a runner up on "
                f"port {args.port} for {args.device}?"
            )
        if sem_roots is None:
            problems.append(
                f"the probe did not answer on {args.device} — is "
                f"`debugImplementation(\"jp.golia.smix:smix-probe\")` in "
                f"{args.app}'s build, and is the app in the foreground?"
            )
        if problems:
            report()
            return 1
    else:
        ap.error("give --device, or both --a11y and --semantics")

    if args.superset_only:
        return superset(a11y_tree, sem_roots)
    both, matched = reconcile(a11y_tree, sem_roots, args.prove_differences_exhibited)
    # The presence half, and the ONLY one: a first draft also carried an
    # `if not both` check, which never fired on its own because this
    # count's default of 1 already covered it. Two predicates saying one
    # thing means one of them is never the reason for a red, and a
    # mutation sweep found exactly that.
    if both < args.min_both:
        problems.append(
            f"only {both} tags on both sides, expected at least "
            f"{args.min_both} — a reconciliation over an empty set agrees "
            f"with anything, and a screen half of which stopped answering "
            f"still has some. Is this the screen it was pointed at, and are "
            f"both readers answering?"
        )
    if problems:
        report()
        return 1
    named = ", ".join(f"{n}×{c}" for n, c in matched.items())
    print(f"two-paths-agree: {both} tags on both sides, differences all named ({named})")
    return 0


def report():
    print("two-paths-agree: FAIL")
    for p in problems:
        print(f"  - {p}")


if __name__ == "__main__":
    sys.exit(main())
