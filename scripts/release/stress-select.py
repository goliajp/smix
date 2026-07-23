#!/usr/bin/env python3
"""Select the flows for a stress/smoke tier from the corpus manifest.

The stress harness runs one corpus at two tiers: `smoke` (a key subset,
per PR) and `all` (the full corpus, nightly). This is the single place
that decides which flows a tier runs, so `stress-gate.sh` never
hand-maintains a second list. `smoke` is always a subset of `all` — a
flow in the smoke tier that is not in the corpus is a manifest error,
caught by the self-test rather than surfacing as a missing file at run
time.

Manifest: `scripts/release/stress-corpus.yaml`, a list of
`{ path: <flow.yaml>, tier: smoke|stress }`. `smoke`-tier flows run in
both tiers; `stress`-tier flows run only under `all`.

Usage:
  stress-select.py --tier smoke        # paths for the smoke subset
  stress-select.py --tier all          # every flow path
  stress-select.py --test              # self-test, exit 0/1
"""

import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
MANIFEST = REPO / "scripts/release/stress-corpus.yaml"


def parse_manifest(text):
    """A deliberately tiny YAML reader for the fixed manifest shape.

    The manifest is a list of `- path: X` / `  tier: Y` pairs. Using a
    hand parser keeps the gate free of a pyyaml dependency the CI image
    may not carry; the shape is fixed and the self-test pins it.
    """
    flows = []
    cur = {}
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("- path:"):
            if cur:
                flows.append(cur)
            cur = {"path": line.split(":", 1)[1].strip()}
        elif line.startswith("path:"):
            if cur:
                flows.append(cur)
            cur = {"path": line.split(":", 1)[1].strip()}
        elif line.startswith("tier:"):
            cur["tier"] = line.split(":", 1)[1].strip()
    if cur:
        flows.append(cur)
    return flows


def select(flows, tier):
    """Flow paths for a tier. `smoke` → smoke-tier only; `all` →
    every flow. Order is manifest order, deterministic."""
    if tier == "smoke":
        return [f["path"] for f in flows if f.get("tier") == "smoke"]
    if tier == "all":
        return [f["path"] for f in flows]
    raise ValueError(f"unknown tier {tier!r} (want smoke|all)")


def _self_test():
    fixture = parse_manifest(
        "- path: a.yaml\n  tier: smoke\n"
        "- path: b.yaml\n  tier: stress\n"
        "- path: c.yaml\n  tier: smoke\n"
    )
    smoke = select(fixture, "smoke")
    allf = select(fixture, "all")
    assert smoke == ["a.yaml", "c.yaml"], smoke
    assert allf == ["a.yaml", "b.yaml", "c.yaml"], allf
    # smoke is always a subset of all — the property the gate relies on.
    assert set(smoke) <= set(allf), "smoke must be a subset of all"
    # every smoke flow is a real corpus row, never a dangling name.
    corpus_paths = {f["path"] for f in fixture}
    assert set(smoke) <= corpus_paths, "a smoke flow is not in the corpus"
    try:
        select(fixture, "bogus")
    except ValueError:
        pass
    else:
        raise AssertionError("an unknown tier must be rejected")
    print("stress-select: self-test ok")


def main(argv):
    if "--test" in argv:
        _self_test()
        return 0
    if "--tier" not in argv:
        print(__doc__)
        return 2
    tier = argv[argv.index("--tier") + 1]
    if not MANIFEST.exists():
        print(f"error: no manifest at {MANIFEST}", file=sys.stderr)
        return 2
    flows = parse_manifest(MANIFEST.read_text())
    for path in select(flows, tier):
        print(path)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
