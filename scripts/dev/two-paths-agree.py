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
import _e2e_binary  # noqa: E402

problems = []


def sh(*cmd):
    r = subprocess.run(cmd, capture_output=True, text=True)
    return r.stdout if r.returncode == 0 else None


def fetch_a11y(binary, device, port):
    # `--reader a11y`, and the flag is load-bearing. Since I1 was closed
    # `smix tree` asks the probe first — it is what a flow does — so a
    # plain call on a probe-carrying app hands back the semantics tree,
    # and this gate would compare that tree with itself and find perfect
    # agreement on every screen forever.
    out = sh(binary, "tree", "--device", device, "--port", str(port), "--json",
             "--reader", "a11y")
    if out is None:
        return None
    body = out
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
    """Every name the semantics side carries, with its node and root index.

    Two kinds of name, and a flow cannot tell them apart: a Compose node
    has a `testTag`, and a View that Compose hosts inside an `AndroidView`
    has its own resource id. The second kind arrived in v10.2 — before it
    the probe could not see hosted Views at all, which is the defect this
    reader's own assertion was supposed to catch and could not, having
    never been shown a screen with one on it.
    """
    found = {}

    def walk(n, root_index):
        t = n.get("testTag") or n.get("resourceId")
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


def clipped_away(tag, sem, a11y):
    """The probe can see a node that is placed and entirely clipped.

    A row scrolled past the end of its viewport is still composed and
    still has a position; the accessibility path drops it, and the probe
    reports it with an empty rectangle and `visible: false`. Both are
    right, and the difference is one the probe is ALLOWED to have —
    unlike the reverse, which is the defect this gate exists for.

    Told from the data: the semantics side says nothing of it shows.
    Exhibited by `.InteropActivity`, whose scrolling column has five rows
    below its viewport.
    """
    node = sem.get(tag)
    if node is None:
        return False
    n = node[0]
    if n.get("visible") is False:
        return True
    b = n.get("bounds")
    return isinstance(b, list) and len(b) == 4 and (b[2] <= b[0] or b[3] <= b[1])


def hosts_a_view(tag, sem, a11y):
    """A Compose node whose content is a View Compose hosts.

    `AndroidView` does not project its own semantics node to
    accessibility — the hosted View's nodes are projected in its place.
    So the wrapper's testTag is on the semantics side and nowhere on the
    other, while everything inside it is on both.

    Told by geometry rather than by name: some hosted View node (one
    carrying a `resourceId`, which only a View has) lies inside this
    node's rectangle. Exhibited by `.InteropActivity`.
    """
    node = sem.get(tag)
    if node is None:
        return False
    outer = probe_rect(node[0])
    if outer is None:
        return False
    for other, (n, _) in sem.items():
        if other == tag or not n.get("resourceId"):
            continue
        inner = probe_rect(n)
        if inner is None:
            continue
        if (outer[0] <= inner[0] and outer[1] <= inner[1]
                and outer[2] >= inner[2] and outer[3] >= inner[3]):
            return True
    return False


# The named differences, each with a screen that produces it.
#
# The first draft of this list carried a `secondary-compose-root` rule for
# dialogs. Driving it showed the rule could not be exhibited without also
# destroying what it was an exception TO: with a Compose dialog open, the
# accessibility path does not lose the dialog, it loses THE WHOLE APP — 16
# compose ids one second, zero the next, while the probe still reported 17.
# That was not a difference to be excused; it was the defect v10 existed to
# close, so the rule went rather than being kept as an exemption that
# excludes the empty set while printing that it considered something.
#
# The two below are different: each is a case where the probe legitimately
# sees MORE than the accessibility path, which is the direction this gate
# allows, and `--prove-differences-exhibited` requires each to match on the
# screen it names.
RULES = [
    ("clipped-away", "a placed node with nothing of it showing", clipped_away),
    ("hosts-a-view", "an AndroidView wrapper, whose hosted View is projected instead",
     hosts_a_view),
]


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


def rect_of(node):
    """A node's rectangle as (left, top, right, bottom), or None."""
    b = node.get("bounds") or {}
    try:
        x, y = float(b["x"]), float(b["y"])
        w, h = float(b["w"]), float(b["h"])
    except (KeyError, TypeError, ValueError):
        return None
    return (x, y, x + w, y + h)


def probe_rect(node):
    """What the accessibility reader is answering about: the part that shows.

    The probe reports two rectangles — the node's own, and the part of it
    inside its scroll container and the screen. The accessibility path
    reports the second kind, so comparing it against the first would call
    the two readers wrong about every clipped row while they agree
    perfectly. `visibleBounds` when it is there; a probe too old to send
    it only ever had the one rectangle.
    """
    b = node.get("visibleBounds")
    if not isinstance(b, list) or len(b) != 4:
        b = node.get("bounds")
    if not isinstance(b, list) or len(b) != 4:
        return None
    try:
        return tuple(float(v) for v in b)
    except (TypeError, ValueError):
        return None


def centre(r):
    return ((r[0] + r[2]) / 2, (r[1] + r[3]) / 2)


def holds(r, point):
    return r[0] <= point[0] <= r[2] and r[1] <= point[1] <= r[3]


def bounds_agree(a11y_tree, sem_roots, min_compared):
    """The two readers put the same element in the same place.

    Presence was the only thing compared here, and the second half of what
    a consumer reported was entirely about position: a row reported at
    [0,533,1080,743] while the screen showed it nowhere. Both readers
    naming a thing and disagreeing about where it is, is the same class of
    fault as one of them not naming it at all — and it is the half that
    decides where a tap lands.

    Not compared edge by edge, and not with a tolerance. The two readers
    report different rectangles for a good reason: the accessibility side
    of Compose reports a node's TOUCH TARGET, padded out to the minimum
    48dp, while semantics reports the visual box. Measured on the recorded
    fixture payloads, `compose_open_dialog` is 132px tall to one reader and
    110px to the other — a real 11px disagreement that means nothing, and
    a threshold loose enough to forgive it would forgive a row-height
    error too.

    So the question asked is the one that has consequences: does each
    reader's centre — where a tap is aimed — fall inside the other's
    rectangle. Padding cannot break that, and being in the wrong place
    cannot satisfy it.
    """
    a11y = a11y_tags(a11y_tree)
    sem = semantics_tags(sem_roots)
    compared = 0
    for tag in sorted(set(a11y) & set(sem)):
        theirs = rect_of(a11y[tag])
        ours = probe_rect(sem[tag][0])
        if theirs is None or ours is None:
            continue
        compared += 1
        if not (holds(theirs, centre(ours)) and holds(ours, centre(theirs))):
            problems.append(
                f"`{tag}` is in two places: the accessibility path says "
                f"{tuple(int(v) for v in theirs)} and the probe says "
                f"{tuple(int(v) for v in ours)}, and neither contains the "
                f"other's centre. A tap goes where the probe says."
            )
    # Non-empty: a comparison over nothing agrees with everything, and the
    # two readers naming disjoint sets of things is itself the finding.
    if compared < min_compared:
        problems.append(
            f"only {compared} element(s) had a rectangle on both sides, "
            f"expected at least {min_compared} — either the screen is not "
            f"the one this was pointed at, or the two readers are naming "
            f"different things"
        )
    return compared


# What each reader says about a node's state, keyed the way each spells it.
# Two constants in the probe's View walk (`focused = false`, `enabled =
# true`) passed this gate for as long as it compared only presence and
# place — while `inputText` after `tapOn` on a View field found no focused
# field on every run, because the flow read the probe's constant.
STATE_KEYS = (("hasFocus", "focused", "focus"), ("enabled", "enabled", "enablement"))


def state_agrees(a11y_tree, sem_roots, focus_tag):
    """Both readers say the same about focus and enablement.

    Compared over every name the two share. A key missing on one side is
    reported rather than skipped: that is a wire that changed, and a
    comparison that quietly stops comparing reads exactly like agreement.

    `focus_tag` is the presence half. On a screen where nothing holds
    focus, "both say false" agrees with a reader that cannot say true, so
    when a field has been focused on purpose, both must say so about it.
    """
    a11y = a11y_tags(a11y_tree)
    sem = semantics_tags(sem_roots)
    compared = 0
    for tag in sorted(set(a11y) & set(sem)):
        theirs, ours = a11y[tag], sem[tag][0]
        for their_key, our_key, what in STATE_KEYS:
            if their_key not in theirs or our_key not in ours:
                problems.append(
                    f"`{tag}`: no `{their_key}` from the accessibility path or "
                    f"no `{our_key}` from the probe, so its {what} was not "
                    f"compared"
                )
                continue
            compared += 1
            if bool(theirs[their_key]) != bool(ours[our_key]):
                problems.append(
                    f"`{tag}`: the accessibility path says {what} is "
                    f"{bool(theirs[their_key])} and the probe says "
                    f"{bool(ours[our_key])}. A flow reads the probe."
                )
    if focus_tag is not None:
        a = a11y.get(focus_tag)
        m = sem.get(focus_tag)
        if not (a and a.get("hasFocus")) or not (m and m[0].get("focused")):
            problems.append(
                f"`{focus_tag}` was tapped to take focus, and the accessibility "
                f"path says {bool(a and a.get('hasFocus'))} while the probe says "
                f"{bool(m and m[0].get('focused'))} — both must say it holds "
                f"focus, or \"they agree\" is only agreement that nothing does"
            )
    return compared


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--device")
    ap.add_argument("--port", default="22095")
    ap.add_argument("--app", default="dev.smix.fixture")
    # Default: the one resolver's answer (this tree's debug build unless
    # SMIX_BIN names another). It was `./target/release/smix`, relative to
    # wherever the gate was started, while the e2e beside it drove debug.
    ap.add_argument("--binary")
    ap.add_argument("--a11y")
    ap.add_argument("--semantics")
    ap.add_argument("--prove-differences-exhibited", action="store_true")
    ap.add_argument(
        "--activity", default=".ComposeActivity",
        help="which screen to put in front before reading it. The reader "
             "drove one screen for two majors, and the defects it exists "
             "to catch were on the screens it never saw.",
    )
    ap.add_argument(
        "--min-bounds-compared", type=int, default=1,
        help="how many elements must have a rectangle on both sides.",
    )
    ap.add_argument(
        "--min-both", type=int, default=1,
        help="how many tags must appear on BOTH sides. A count rather than "
             "'more than none': a screen that quietly stopped rendering half "
             "of itself still has some.",
    )
    ap.add_argument(
        "--focus",
        help="a tag to tap before reading, so that one field holds focus and "
             "the two readers are asked about a focus that exists.",
    )
    ap.add_argument(
        "--superset-only", action="store_true",
        help="assert only that the probe sees everything the accessibility "
             "path sees. True on every screen state including the ones where "
             "the a11y path has gone blind, so it is the half that keeps "
             "meaning something after the blindness is fixed.",
    )
    args = ap.parse_args()
    if args.device and not args.binary:
        args.binary = _e2e_binary.this_tree_smix()

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
                        f"{args.app}/{args.activity}"], capture_output=True, text=True)
        # Wait for the screen, do not guess at it. This was `sleep(2)`,
        # which is enough on an idle machine and not enough after two and
        # a half hours of ship -- and then the accessibility side reads
        # the system status bar while the probe, which lives in the app's
        # process, answers with the app's tags whatever is in front. The
        # reconciliation compares two different screens and blames the
        # feature. Measured 2026-08-29: sixteen tags "on the semantics
        # side and not the accessibility side", and the app was not up.
        if _probe_wire.wait_for_front(args.device, args.app) is None:
            print("two-paths-agree: FAIL")
            print(f"  - {args.app} never came to the front on {args.device} "
                  f"within 30s, so the two trees would be of different "
                  f"screens. This is about the app starting, not about "
                  f"what the two readers see.")
            return 1
        # The resumed activity is set before Compose has projected itself
        # into the accessibility tree. Measured 2026-08-29 on this
        # emulator: at t=0 the tree is 40 nodes of system UI, at t=1 it is
        # 72 with the app's; the ids the readers share appear about three
        # seconds after a force-stop. What was here was `sleep(2)` --
        # right on that boundary, so it passed on an idle machine and
        # failed after two and a half hours of ship, and then the gate
        # said sixteen tags were "on the semantics side and not the
        # accessibility side": a sentence about the release's headline
        # feature, about a screen that had not arrived.
        #
        # Waiting for the tree to STOP CHANGING does not work either --
        # the system-only tree is perfectly stable too. A stable wrong
        # state looks exactly like a stable right one.
        #
        # So wait for the thing that actually distinguishes them: one id
        # present on BOTH sides, which is what "the two readers are
        # looking at the same screen" means. It is strictly weaker than
        # what this gate asserts (all of them, with every difference
        # named), and if it never happens the loop simply ends and the
        # reconciliation below runs and reports it exactly as before. The
        # wait removes the race; it does not remove the finding.
        for _ in range(60):
            early_a11y = fetch_a11y(args.binary, args.device, args.port)
            early_sem = fetch_semantics(args.device, args.app)
            if early_a11y is not None and early_sem is not None:
                shared = set(a11y_tags(early_a11y)) & set(semantics_tags(early_sem))
                if shared:
                    break
            time.sleep(0.5)

    if args.focus and args.device and not (args.a11y or args.semantics):
        subprocess.run([args.binary, "tap", "--device", args.device, "--port",
                        str(args.port), f"id:{args.focus}"],
                       capture_output=True, text=True)
        # The keyboard coming up is what focus looks like on the device,
        # and it takes a moment; read once both readers could have seen it.
        for _ in range(20):
            early_a11y = fetch_a11y(args.binary, args.device, args.port)
            node = a11y_tags(early_a11y or {}).get(args.focus) or {}
            if node.get("hasFocus"):
                break
            time.sleep(0.5)

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
    compared = bounds_agree(a11y_tree, sem_roots, args.min_bounds_compared)
    states = state_agrees(a11y_tree, sem_roots, args.focus)
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
    print(
        f"two-paths-agree: {both} tags on both sides, {compared} of them "
        f"in the same place, {states} focus/enablement readings the same"
        f"{f' (with `{args.focus}` focused)' if args.focus else ''}, "
        f"differences all named ({named})"
    )
    return 0


def report():
    print("two-paths-agree: FAIL")
    for p in problems:
        print(f"  - {p}")


if __name__ == "__main__":
    sys.exit(main())
