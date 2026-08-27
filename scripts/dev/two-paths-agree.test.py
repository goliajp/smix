#!/usr/bin/env python3
"""Self-test for two-paths-agree, on payloads recorded from a real device.

Recorded rather than hand-written: the shapes here are what the two readers
actually produced on the fixture (screen coordinates, system bars, the
activity's content frame around the Compose root), and a hand-written
payload would encode what the author believed instead. 9.0.0 spent three
rounds on gate scanners whose fixtures were built differently from the
thing they scanned.
"""

import copy
import json
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
FIX = HERE / "fixtures" / "two-paths"
GATE = HERE / "two-paths-agree.py"

failures = []


def run(a11y, semantics, *extra, tmp=None):
    a = tmp / "a.json"
    s = tmp / "s.json"
    a.write_text(json.dumps(a11y))
    s.write_text(json.dumps(semantics))
    r = subprocess.run(
        [sys.executable, str(GATE), "--a11y", str(a), "--semantics", str(s), *extra],
        capture_output=True, text=True,
    )
    return r.returncode, r.stdout + r.stderr


def load(name):
    return json.loads((FIX / name).read_text())


def check(label, cond, detail=""):
    if not cond:
        failures.append(f"{label}{': ' + detail if detail else ''}")


def find_tag(roots, tag):
    for r in roots:
        stack = [r]
        while stack:
            n = stack.pop()
            if n.get("testTag") == tag:
                return n
            stack.extend(n.get("children") or [])
    return None


def main():
    import tempfile
    tmp = pathlib.Path(tempfile.mkdtemp())

    base_a, base_s = load("base-a11y.json"), load("base-semantics.json")
    dlg_a, dlg_s = load("dialog-a11y.json"), load("dialog-semantics.json")

    # 1. The recorded base screen is what the gate was built to pass.
    rc, out = run(base_a, base_s, "--min-both", "16", tmp=tmp)
    check("base screen should reconcile", rc == 0, out.strip()[:160])

    # 2. Take a tag away from the probe's side. The gate has to notice a
    #    node the accessibility path can see and the probe cannot — that
    #    direction is the probe being wrong, and it is the one that would
    #    otherwise pass by looking like "the probe sees more".
    holed = copy.deepcopy(base_s)
    node = find_tag(holed, "compose_submit")
    check("fixture should carry compose_submit", node is not None)
    if node:
        node["testTag"] = None
    rc, out = run(base_a, holed, "--min-both", "15", tmp=tmp)
    check("a tag missing from the probe should red", rc != 0, out.strip()[:160])
    check("and should name it", "compose_submit" in out, out.strip()[:160])

    # 3. The count is exact for a reason: a screen half of which stopped
    #    answering still has some tags on both sides.
    rc, out = run(base_a, base_s, "--min-both", "99", tmp=tmp)
    check("a too-low match count should red", rc != 0, out.strip()[:160])

    # 4. An empty screen must not reconcile perfectly with another empty
    #    screen. This is the check that stops the whole gate passing when
    #    one of the two readers has silently returned nothing.
    rc, out = run({"children": []}, [], tmp=tmp)
    check("two empty trees must not agree", rc != 0, out.strip()[:160])

    # 5. The rule list is empty today. Proving differences must therefore
    #    not become vacuous — it still has to require real agreement.
    rc, out = run({"children": []}, [], "--prove-differences-exhibited", tmp=tmp)
    check("prove-mode on nothing must still red", rc != 0, out.strip()[:160])

    # 6. The recorded dialog state: this is the defect, stated. The
    #    accessibility path sees none of the app; the probe sees all of it.
    rc, out = run(dlg_a, dlg_s, "--superset-only", tmp=tmp)
    check("probe should be a superset with the dialog open", rc == 0, out.strip()[:160])
    check(
        "and should say how much more it saw",
        "more" in out,
        out.strip()[:160],
    )
    rc, out = run(dlg_a, dlg_s, "--min-both", "16", tmp=tmp)
    check(
        "reconciliation must NOT pass with the dialog open — that state is "
        "the defect, not an agreed difference",
        rc != 0,
        out.strip()[:160],
    )

    # 6b. A tag the probe sees and the accessibility path does not, on a
    #     screen where they otherwise agree, and with no rule to excuse it.
    #     The rule list is empty today, so this is the check that says so
    #     out loud instead of passing.
    extra = copy.deepcopy(base_s)
    holder = find_tag(extra, "compose_submit")
    if holder:
        holder["children"] = (holder.get("children") or []) + [{
            "id": 9901, "testTag": "invented_by_the_test",
            "bounds": holder["bounds"], "focused": False, "enabled": True,
            "actions": [], "children": [],
        }]
    rc, out = run(base_a, extra, "--min-both", "16", tmp=tmp)
    check("an unexplained semantics-only tag should red", rc != 0, out.strip()[:160])
    check("and should name it", "invented_by_the_test" in out, out.strip()[:160])

    # 6c. The geometric scoping has to be checkable too. If a root's
    #     rectangle covered the whole device, every system id would fall
    #     "inside a Compose root" and the scoping would be excusing by
    #     accident. Nothing on a real screen exercises that, so it is done
    #     here.
    huge = copy.deepcopy(base_s)
    for r in huge:
        r["bounds"] = [-1, -1, 100000, 100000]
    rc, out = run(base_a, huge, "--min-both", "16", tmp=tmp)
    check(
        "a root rectangle covering the device should red the scoping",
        rc != 0 and "not what they claim" in out,
        out.strip()[:200],
    )

    # 7. The superset half must bite when the probe really does miss
    #    something that the accessibility path found inside a Compose root.
    holed2 = copy.deepcopy(base_s)
    n2 = find_tag(holed2, "compose_input")
    if n2:
        n2["testTag"] = None
    rc, out = run(base_a, holed2, "--superset-only", tmp=tmp)
    check("superset should red when the probe drops a node", rc != 0, out.strip()[:160])
    check("superset should name the dropped node", "compose_input" in out, out.strip()[:160])

    if failures:
        print("two-paths-agree.test: FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1
    print("two-paths-agree.test: clean — 13 assertions over 4 recorded payloads")
    return 0


if __name__ == "__main__":
    sys.exit(main())
