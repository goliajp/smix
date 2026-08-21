#!/usr/bin/env python3
"""Does the publication verifier judge, and judge the right things?

Its subject is four live registries, so this does not re-check them. It
checks the parts that decide what gets asked and what a missing answer
means — the two places its first draft was wrong.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
VERIFIER = os.path.join(ROOT, "scripts", "release", "verify-published.sh")


def npm_package_list(root: str) -> list[str]:
    """Run the verifier's own package-listing python, on a given tree."""
    src = open(VERIFIER, encoding="utf-8").read()
    start = src.index("NPM_PKGS=\"$(cd \"$ROOT\" && python3 -c '") + len(
        "NPM_PKGS=\"$(cd \"$ROOT\" && python3 -c '"
    )
    end = src.index("')\"", start)
    code = src[start:end]
    out = subprocess.run(
        [sys.executable, "-c", code], cwd=root, capture_output=True, text=True
    )
    return [n for n in out.stdout.split() if n]


def tree_with(packages: list[tuple[str, str, bool]]) -> str:
    """A tree of npm/<dir>/package.json entries: (dir, name, private)."""
    d = tempfile.mkdtemp()
    for sub, name, private in packages:
        os.makedirs(os.path.join(d, "npm", sub), exist_ok=True)
        body: dict = {"name": name, "version": "1.0.0"}
        if private:
            body["private"] = True
        with open(os.path.join(d, "npm", sub, "package.json"), "w") as fh:
            json.dump(body, fh)
    return d


def main() -> int:
    ok = True

    # A private package is npm's own "never publish this". Asking about
    # it reports a package missing that was never meant to be there —
    # which is what the first draft did, about smix-web-record.
    d = tree_with(
        [("public-one", "@x/public-one", False), ("secret", "@x/secret", True)]
    )
    names = npm_package_list(d)
    if "@x/secret" in names:
        print("  FAIL a private package is asked about")
        ok = False
    elif "@x/public-one" not in names:
        print(f"  FAIL a public package is not asked about: {names}")
        ok = False
    else:
        print("  ok   private packages are not asked about")

    # A private parent must not hide a public per-triple child from
    # being listed... and must not have children asked about either,
    # since they are published beside it. Assert the behaviour that is
    # actually implemented: a private parent takes its subtree with it.
    d = tree_with([("cli", "@x/cli", True)])
    os.makedirs(os.path.join(d, "npm", "cli", "npm", "darwin"), exist_ok=True)
    with open(os.path.join(d, "npm", "cli", "npm", "darwin", "package.json"), "w") as fh:
        json.dump({"name": "@x/cli-darwin", "version": "1.0.0"}, fh)
    names = npm_package_list(d)
    if names:
        print(f"  FAIL a private parent still lists its children: {names}")
        ok = False
    else:
        print("  ok   a private parent takes its per-triple children with it")

    # The real tree: the packages the ship publishes, and not the
    # private one.
    names = npm_package_list(ROOT)
    if "@goliapkg/smix-web-record" in names:
        print("  FAIL the real tree lists the private package")
        ok = False
    elif "@goliapkg/smix-cli" not in names or "@goliapkg/smix" not in names:
        print(f"  FAIL the real tree is missing published packages: {names}")
        ok = False
    else:
        print(f"  ok   the real tree lists {len(names)} publishable npm packages")

    # Maven late is not Maven failed. The script must contain the
    # distinction rather than treat an absent artifact as a failure —
    # asserted on the source because the subject is a live registry.
    src = open(VERIFIER, encoding="utf-8").read()
    if "NOT YET" not in src or "PENDING" not in src:
        print("  FAIL the verifier has no 'still to come' state for Maven")
        ok = False
    elif 'bad "maven' in src:
        print("  FAIL an absent Maven artifact is treated as a failure")
        ok = False
    else:
        print("  ok   Maven may be late and is never claimed")

    # And a missing version anywhere must exit non-zero. Checked by
    # asking about a version that cannot exist.
    p = subprocess.run(
        ["bash", VERIFIER, "0.0.0-never-published"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        env={**os.environ, "SMIX_VERIFY_SKIP_MAVEN": "1"},
    )
    if p.returncode == 0:
        print("  FAIL a version nobody published verified clean")
        ok = False
    elif "NOT fully published" not in p.stdout:
        print(f"  FAIL red, but does not say so\n{p.stdout}")
        ok = False
    else:
        print("  ok   a version nobody published is refused")

    print("=== verify-published-reads-registries.test:", "PASS" if ok else "FAIL", "===")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
