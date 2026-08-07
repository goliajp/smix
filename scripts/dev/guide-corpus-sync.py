#!/usr/bin/env python3
"""Keep the guide corpus and the guides themselves in step.

`guide_gate` runs every yaml example the guides print, against the real
parser, the real Adapter and the real driver admissibility rule. It found
examples that had never worked — `pressKey` documenting BACK / POWER /
SCREEN_LOCK when `KeyName` has none of them, `assertTrue: ${output.userCount
> 0}` that cannot lex — and it is worth keeping exactly as it is.

What was wrong was where it read them from. It `include_str!`ed nine pages
of `docs/ai-guide/` into the crate, so the crate's test build depended on
documents, and a separate derivation had to be bolted onto preflight to map
a changed guide back to the crates whose tests compiled it. The arrow
pointed from documentation into code.

This turns it around. The blocks are extracted to
`crates/smix-cli/tests/guide-corpus/blocks/<page>/<NN>.yaml`, which is
ordinary test data owned by the crate; the gate reads those. This script is
what keeps them equal to what the guides print, and it is the only thing
that crosses the line.

  guide-corpus-sync.py            write the corpus from the guides
  guide-corpus-sync.py --check    fail if they differ (the gate)

Numbering is 1-based and counts EVERY yaml block on a page, including the
ones that are not flows — `smix script` files, node rosters, `sims.json`.
The gate skips those itself, and its KNOWN_BROKEN entries address blocks by
that number, so dropping non-flows here would silently renumber the corpus
and point every exemption at the wrong example.
"""

import os
import shutil
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
GUIDES = os.path.join(ROOT, "docs", "ai-guide")
CORPUS = os.path.join(ROOT, "crates", "smix-cli", "tests", "guide-corpus", "blocks")
DERIVED = os.path.join(ROOT, "crates", "smix-cli", "tests", "guide-corpus", "derived")

# Facts stated in a guide's prose rather than in a fenced block, which the
# gate still has to put in front of the runtime. `pressKey` is the one:
# 04-actions names the available keys in a sentence, and the sentence once
# listed BACK / POWER / SCREEN_LOCK when `KeyName` had none of them. Only
# the runtime can answer whether a key exists, so the sentence is turned
# into a flow here and run over there.
KEYS_PAGE = "04-actions"
KEYS_PREFIX = "- Available keys:"
MIN_KEYS = 6

# The pages the gate judges, in the order the guides number them. This is
# the same list the gate used to carry as `include_str!` arms; it lives
# here now because this script is the only thing that reads the guides.
PAGES = [
    "02-yaml-reference",
    "03-selectors",
    "04-actions",
    "05-cli",
    "06-fixtures",
    "07-errors",
    "08-cookbook",
    "10-ai-assertions",
    "12-authoring",
]

# A page that stops yielding blocks is an extraction that broke, not a page
# that got shorter. Checked per page rather than in total so one page
# emptying cannot hide behind the others.
MIN_BLOCKS_PER_PAGE = 1
MIN_BLOCKS_TOTAL = 70


def yaml_blocks(doc):
    """Every ```yaml fence on the page, in order.

    Deliberately the same shape as the reader the gate used to run: an
    opening fence is exactly "```yaml" and a closing one exactly "```",
    both after trailing whitespace is dropped.
    """
    out = []
    in_block = False
    cur = []
    for line in doc.splitlines():
        if in_block:
            if line.rstrip() == "```":
                out.append("".join(cur))
                cur = []
                in_block = False
            else:
                cur.append(line + "\n")
        elif line.rstrip() == "```yaml":
            in_block = True
    return out


def extract():
    """Page name → list of blocks, or None if a guide is unreadable."""
    corpus = {}
    total = 0
    for page in PAGES:
        path = os.path.join(GUIDES, f"{page}.md")
        try:
            with open(path, encoding="utf-8") as fh:
                doc = fh.read()
        except OSError as e:
            print(f"guide-corpus: cannot read {page}.md: {e}")
            return None
        blocks = yaml_blocks(doc)
        if len(blocks) < MIN_BLOCKS_PER_PAGE:
            print(
                f"guide-corpus: {page}.md yielded {len(blocks)} blocks — the "
                f"extraction stopped matching and writing this would empty "
                f"the corpus"
            )
            return None
        corpus[page] = blocks
        total += len(blocks)
    if total < MIN_BLOCKS_TOTAL:
        print(
            f"guide-corpus: {total} blocks across {len(PAGES)} pages, expected "
            f"at least {MIN_BLOCKS_TOTAL} — treating this as a broken parse "
            f"rather than a shorter corpus"
        )
        return None
    return corpus


def press_keys_flow():
    """The `pressKey` sentence in 04-actions, as a flow the runtime can take.

    Returns None with a printed reason if the sentence is gone or has
    changed shape — a list read as empty would let the gate pass by knowing
    nothing.
    """
    path = os.path.join(GUIDES, f"{KEYS_PAGE}.md")
    try:
        with open(path, encoding="utf-8") as fh:
            doc = fh.read()
    except OSError as e:
        print(f"guide-corpus: cannot read {KEYS_PAGE}.md: {e}")
        return None
    line = next((l for l in doc.splitlines() if l.startswith(KEYS_PREFIX)), None)
    if line is None:
        print(f"guide-corpus: {KEYS_PAGE}.md no longer lists the available keys")
        return None
    keys = [
        k.strip().rstrip(".")
        for k in line[len(KEYS_PREFIX):].split("/")
        if k.strip().rstrip(".")
    ]
    if len(keys) < MIN_KEYS:
        print(
            f"guide-corpus: only {len(keys)} keys read out of the sentence in "
            f"{KEYS_PAGE}.md — the list changed shape and writing this would "
            f"let the gate pass by knowing nothing"
        )
        return None
    return "".join(f"- pressKey: {k}\n" for k in keys)


def fixture_registry():
    """The jsonc registry 06-fixtures tells the reader to write.

    The `- fixture:` example one block below it names an id out of this
    registry, so the two are only meaningful together: a registry written
    by hand in the gate would be a second copy that nothing keeps in step,
    and the example would go on resolving after the guide renamed it.
    """
    path = os.path.join(GUIDES, "06-fixtures.md")
    try:
        with open(path, encoding="utf-8") as fh:
            doc = fh.read()
    except OSError as e:
        print(f"guide-corpus: cannot read 06-fixtures.md: {e}")
        return None
    block, cur = None, None
    for line in doc.splitlines():
        if cur is not None:
            if line.rstrip() == "```":
                block = "".join(cur)
                cur = None
            else:
                cur.append(line + "\n")
        elif line.rstrip() == "```jsonc":
            cur = []
    if block is None:
        print("guide-corpus: 06-fixtures.md no longer prints a jsonc registry block")
        return None
    return block


DERIVED_FILES = {
    "press-keys.yaml": press_keys_flow,
    "fixture-registry.json": fixture_registry,
}


def block_path(page, index):
    return os.path.join(CORPUS, page, f"{index:02d}.yaml")


def write(corpus):
    # Rewritten wholesale: a block deleted from a guide has to disappear
    # from the corpus too, and reconciling that incrementally is how a
    # stale example survives a rename.
    if os.path.isdir(CORPUS):
        shutil.rmtree(CORPUS)
    written = 0
    for page, blocks in corpus.items():
        os.makedirs(os.path.join(CORPUS, page), exist_ok=True)
        for i, block in enumerate(blocks, start=1):
            with open(block_path(page, i), "w", encoding="utf-8") as fh:
                fh.write(block)
            written += 1

    os.makedirs(DERIVED, exist_ok=True)
    for name, build in DERIVED_FILES.items():
        body = build()
        if body is None:
            return None
        with open(os.path.join(DERIVED, name), "w", encoding="utf-8") as fh:
            fh.write(body)
        written += 1
    return written


def check(corpus):
    problems = []
    on_disk = set()
    if os.path.isdir(CORPUS):
        for dirpath, _, names in os.walk(CORPUS):
            for n in names:
                if n.endswith(".yaml"):
                    on_disk.add(os.path.join(dirpath, n))

    expected = set()
    for page, blocks in corpus.items():
        for i, block in enumerate(blocks, start=1):
            path = block_path(page, i)
            expected.add(path)
            rel = os.path.relpath(path, ROOT)
            try:
                with open(path, encoding="utf-8") as fh:
                    have = fh.read()
            except OSError:
                problems.append(
                    f"{page} block #{i}: in the guide, absent from the corpus "
                    f"({rel})"
                )
                continue
            if have != block:
                problems.append(
                    f"{page} block #{i}: the guide and {rel} differ — the gate "
                    f"is running an example the page no longer prints"
                )
    for path in sorted(on_disk - expected):
        rel = os.path.relpath(path, ROOT)
        problems.append(f"{rel}: in the corpus, no longer in any guide")

    for name, build in DERIVED_FILES.items():
        body = build()
        path = os.path.join(DERIVED, name)
        rel = os.path.relpath(path, ROOT)
        if body is None:
            problems.append(f"{rel}: cannot be derived from the guide any more")
            continue
        try:
            with open(path, encoding="utf-8") as fh:
                have = fh.read()
        except OSError:
            problems.append(f"{rel}: derived from the guide, absent from the corpus")
            continue
        if have != body:
            problems.append(
                f"{rel}: the guide's prose and this file differ — the gate is "
                f"checking something the page no longer says"
            )
    return problems


def main():
    corpus = extract()
    if corpus is None:
        return 2
    total = sum(len(b) for b in corpus.values())

    if "--check" in sys.argv[1:]:
        problems = check(corpus)
        if problems:
            print(f"guide-corpus: FAIL — {len(problems)} differences")
            for p in problems:
                print(f"  - {p}")
            print(
                "\n  Run `python3 scripts/dev/guide-corpus-sync.py` to bring "
                "the corpus up to what the guides print,\n  then re-run the "
                "gate: an example that changed may no longer reach a route."
            )
            return 1
        print(f"guide-corpus: in step — {total} blocks across {len(corpus)} pages")
        return 0

    written = write(corpus)
    if written is None:
        return 2
    print(
        f"guide-corpus: wrote {written} files — {total} blocks across "
        f"{len(corpus)} pages, {len(DERIVED_FILES)} derived from prose"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
