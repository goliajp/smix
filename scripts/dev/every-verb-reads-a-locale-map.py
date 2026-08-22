#!/usr/bin/env python3
"""A locale map is a selector spelling, so every verb has to read it.

`localizedText:` is rewritten to a plain `Text` selector by
`desugar_localized_text` — a pure function of the current locale, with
no device work and no new capability behind it. Three of the twelve
verbs that take a selector called it. The other nine handed the locale
map straight to the resolver, which matches nothing against it, so the
same element was found by `assertVisible` and not found by
`longPressOn` — measured on emulator-5554 with both flows naming the
same string.

That is the shape this whole release is about: a rewrite that only some
call sites perform. A comment cannot hold the line and neither can a
list; the arms are re-derived here on every run.

A verb that genuinely must not desugar says so in EXEMPT with a reason,
and the reason is checked from the other side: an exempt arm that calls
the desugar anyway is a stale exemption, not a safe one.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
RUNTIME = os.path.join(ROOT, "crates", "smix-adapter-maestro", "src", "runtime.rs")

# Arms that take a selector and must not desugar, with why.
EXEMPT = {
    # Its selector is the anchor of a coordinate shift, resolved by the
    # arm itself through find_norm_coord after the tap dispatcher has
    # already desugared what it was given.
    "Repeat": "the selector is a loop condition evaluated by check_selector_visible, "
    "which desugars for itself",
    "RepeatTap": "same probe path as Repeat",
    # Its `when.visible` / `when.notVisible` are gate selectors, evaluated
    # by evaluate_run_flow_gate → check_selector_visible, which desugars
    # before it looks at anything.
    "RunFlowInline": "its gate selectors go through check_selector_visible, "
    "which desugars for itself",
    "RunFlowConditional": "same gate path as RunFlowInline",
}


def arms(src: str) -> dict[str, str]:
    """Every `Step::X` arm of run_step, by name, with its body."""
    lines = src.split("\n")
    start = next(i for i, l in enumerate(lines) if "async fn run_step(" in l)
    end = next(i for i in range(start + 1, len(lines)) if lines[i] == "    }")
    found: dict[str, list[str]] = {}
    cur = None
    for i in range(start, end):
        m = re.match(r"            Step::([A-Za-z]+)", lines[i])
        if m:
            cur = m.group(1)
            found.setdefault(cur, [])
        if cur:
            found[cur].append(lines[i])
    return {k: "\n".join(v) for k, v in found.items()}


def main() -> int:
    src = open(RUNTIME, encoding="utf-8").read()
    all_arms = arms(src)
    if len(all_arms) < 20:
        print(
            f"every-verb-reads-a-locale-map: CANNOT RUN — found {len(all_arms)} arms, "
            f"which is not run_step. The parser has drifted from the file.",
            file=sys.stderr,
        )
        return 1

    problems: list[str] = []
    checked = 0

    # Four arms are exempt because their selectors go through
    # `check_selector_visible`, which desugars before it looks at
    # anything. That sentence is the whole of their protection, so it is
    # checked rather than trusted: delete the desugar there and this
    # says so, instead of four arms quietly losing cover while the gate
    # stays green.
    probe = re.search(
        r"async fn check_selector_visible.*?\n    \}", src, re.S
    )
    if probe is None:
        problems.append(
            "check_selector_visible is gone, and four arms are exempt on the "
            "grounds that it desugars for them. Re-verify those exemptions."
        )
    elif "desugar_localized_text" not in probe.group(0):
        problems.append(
            "check_selector_visible no longer desugars, and four arms are exempt "
            "on the grounds that it does — a gate selector with `localizedText:` "
            "now matches nothing, so `when.notVisible` fires because the question "
            "went unasked."
        )
    for name, body in sorted(all_arms.items()):
        if "selector" not in body:
            continue
        checked += 1
        # Per call site, not per arm. Asking whether the arm mentions the
        # desugar anywhere passes an arm that desugars one of its two
        # calls — which is how a partial fix comes to read like a whole
        # one, the very thing this release is about.
        # `\s*` around the dots on purpose: rustfmt breaks a long call as
        # `self.app` then `.method(...)` on the next line, and a pattern
        # wanting them contiguous would pass such an arm by failing to see
        # it — a check that reads nothing agrees with everything.
        bare = re.findall(r"self\s*\.\s*app\s*\.\s*([a-z_]+)\s*\(\s*&?selector\b", body)
        desugars = "desugar_localized_text" in body
        if name in EXEMPT:
            if desugars:
                problems.append(
                    f"{name} is exempt on the grounds that {EXEMPT[name]} — but it "
                    f"calls the desugar. The exemption has expired; drop it."
                )
            continue
        for call in bare:
            problems.append(
                f"{name} passes the selector to `App::{call}` without desugaring it, "
                f"so `localizedText:` reaches the resolver as a locale map and matches "
                f"nothing. The same element is found by a verb that does desugar."
            )
        if not bare and not desugars:
            problems.append(
                f"{name} takes a selector and neither desugars it nor passes it to an "
                f"App call this can see. Either it desugars, or it belongs in EXEMPT "
                f"with a reason."
            )

    if problems:
        print("every-verb-reads-a-locale-map: FAIL")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(
        f"every-verb-reads-a-locale-map: clean — {checked} selector-taking arms, "
        f"{len(EXEMPT)} exempt and each still unable to desugar"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
