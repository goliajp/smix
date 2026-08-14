#!/usr/bin/env python3
"""Every rule in the aim-not-arrival gate can still refuse something.

Both directions are pinned. A gate over wording is one careless regex
away from refusing correct prose, and one missing half away from being
satisfied by silence — which is why the required-presence cases are
here at all: a file with nothing to say about taps passes a ban on
phrasings, and says nothing true.

Each mutation asserts that its edit landed before the verdict is read.
A substitution matching nothing produces the same green as a rule
carrying no weight.
"""

import pathlib
import subprocess
import sys

GATE = ["python3", "scripts/dev/a-tap-proves-aim-not-arrival.py"]

CLI = pathlib.Path("crates/smix-cli/src/act.rs")
DRIVER = pathlib.Path("crates/smix-driver/src/lib.rs")
ACTIONS = pathlib.Path("docs/ai-guide/04-actions.md")
AUTHORING = pathlib.Path("docs/ai-guide/12-authoring.md")

# (name, file, before, after, must_refuse)
CASES = [
    ("the CLI line goes back to 'landed'", CLI, 'aimed inside: {}', 'landed inside: {}', True),
    (
        "the CLI line says neither",
        CLI,
        'println!("  aimed inside: {}", inside.join(" < "));',
        'println!("  inside: {}", inside.join(" < "));',
        True,
    ),
    (
        "a guide claims the touch reached the element",
        AUTHORING,
        'A green `tapOn` means "the point aimed at was inside the element named"',
        'A green `tapOn` means "a touch reached the element it aimed at"',
        True,
    ),
    (
        "a guide sends the reader into their own app",
        AUTHORING,
        "and it does not mean the app is where an unchanged screen came\nfrom",
        "and if the screen did not change, the no-op is downstream in the app",
        True,
    ),
    (
        "the actions guide asserts the touch landed inside",
        ACTIONS,
        "means **the aim was inside your target**",
        "means the touch landed inside your target",
        True,
    ),
    (
        "the driver's verdict doc says the touch landed",
        DRIVER,
        "/// The point aimed at was inside the element aimed at, as the",
        "/// The touch landed inside the element that was aimed at, as the",
        True,
    ),
    # The other direction. A gate that only bans words is satisfied by a
    # file that says nothing, and a gate that refuses correct prose is
    # worse than none.
    (
        "a guide that stops mentioning aim at all",
        AUTHORING,
        "the point aimed at was inside the element named",
        "the point touched was inside the element named",
        True,
    ),
    (
        "prose that describes the limit without claiming arrival",
        ACTIONS,
        "a weaker one than \"your element was\ntouched\"",
        "a weaker one than any claim about what the element received",
        False,
    ),
]


def main() -> int:
    originals = {p: p.read_text(encoding="utf-8") for p in {CLI, DRIVER, ACTIONS, AUTHORING}}
    failures = []

    for name, path, before, after, must_refuse in CASES:
        original = originals[path]
        if before not in original:
            failures.append(f"{name}: the search string is not in {path} — this case tested nothing")
            continue
        path.write_text(original.replace(before, after, 1), encoding="utf-8")
        changed = path.read_text(encoding="utf-8") != original
        result = subprocess.run(GATE, capture_output=True, text=True)
        path.write_text(original, encoding="utf-8")

        if not changed:
            failures.append(f"{name}: the file came back unchanged")
        elif must_refuse and result.returncode == 0:
            failures.append(f"{name}: the gate stayed green — nothing depends on that rule")
        elif not must_refuse and result.returncode != 0:
            failures.append(f"{name}: the gate refused correct prose\n{result.stdout}")
        else:
            verdict = "refused" if result.returncode else "allowed"
            print(f"  {name} → {verdict}")

    if failures:
        print("a-tap-proves-aim-not-arrival.test: FAILED")
        for f in failures:
            print(f"  - {f}")
        return 1

    print(f"a-tap-proves-aim-not-arrival.test: {len(CASES)} cases pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
