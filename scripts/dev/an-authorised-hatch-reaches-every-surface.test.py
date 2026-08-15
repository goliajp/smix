#!/usr/bin/env python3
"""Every rule in the hatch-surface gate can still refuse something.

Fixtures rather than the repository, because the gate's subject is a
tree and half these cases need a tree that does not exist here — one
where the missing surface is a different one each time. A gate that has
only ever seen the real repository has only ever been asked one
question.

Both directions are pinned. The presence half must refuse a surface that
lost its hatch, and the absence half must refuse an unauthorised
coordinate API appearing beside the two the charter names — that half is
what stops this gate from reading as a licence. A complete tree must
pass, or the gate is a wall.

Each mutation asserts its edit landed before the verdict is read: a
substitution matching nothing looks exactly like a rule carrying no
weight, and an earlier harness in this repository reported three
weightless rules of which one was a mutation that never applied.
"""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "an-authorised-hatch-reaches-every-surface.py")

# A tree where every hatch reaches every surface. The gate's own
# patterns are what these have to satisfy, so the fixtures are written
# in the shapes each real surface uses.
COMPLETE = {
    "crates/smix-cli/src/main.rs": '''
        /// Where to start the swipe.
        #[arg(long = "from")]
        swipe_from: Option<String>,
        /// A coordinate selector, written "point:50%,80%".
        const POINT: &str = "point";
    ''',
    "crates/smix-mcp/src/main.rs": '''
        struct SwipeParams { direction: Option<String>, swipe_from: Option<String> }
        struct SelectorParams { point: Option<String> }
    ''',
    "npm/smix-rn/src/App.ts": '''
        async tapAtCoord(nx: number, ny: number): Promise<void> {}
        async swipeAtCoord(from: [number, number], to: [number, number]): Promise<void> {}
    ''',
    "crates/smix-node/src/lib.rs": '''
        pub async fn tap_at_coord(&self, nx: f64, ny: f64) -> napi::Result<String> {}
        pub async fn swipe_at_coord(&self, from: (f64, f64), to: (f64, f64)) -> napi::Result<String> {}
    ''',
}

# (name, file, before, after, must_refuse)
CASES = [
    (
        "the CLI loses the swipe hatch",
        "crates/smix-cli/src/main.rs",
        'long = "from"',
        'long = "direction"',
        True,
    ),
    (
        "the MCP loses the swipe hatch",
        "crates/smix-mcp/src/main.rs",
        "swipe_from: Option<String>",
        "scope: Option<String>",
        True,
    ),
    (
        "the TS SDK loses the swipe hatch",
        "npm/smix-rn/src/App.ts",
        "async swipeAtCoord",
        "async swipeDirection",
        True,
    ),
    (
        "napi loses the swipe hatch",
        "crates/smix-node/src/lib.rs",
        "pub async fn swipe_at_coord",
        "pub async fn swipe_direction",
        True,
    ),
    (
        "a surface loses the tap hatch",
        "crates/smix-node/src/lib.rs",
        "pub async fn tap_at_coord",
        "pub async fn tap_by_id",
        True,
    ),
    (
        "an unauthorised coordinate API appears",
        "npm/smix-rn/src/App.ts",
        "async tapAtCoord",
        "async fill_at_coord(x: number) {}\n        async tapAtCoord",
        True,
    ),
    # The other direction. A gate that refuses a complete tree is a wall,
    # and a gate that only ever saw a broken tree has never been shown to
    # allow anything.
    ("the complete tree", "crates/smix-cli/src/main.rs", "", "", False),
]


def write_tree(root, files):
    for rel, body in files.items():
        path = os.path.join(root, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(body)


def main() -> int:
    failures = []

    for name, rel, before, after, must_refuse in CASES:
        files = dict(COMPLETE)
        if before:
            if before not in files[rel]:
                failures.append(f"{name}: the search string is not in the fixture — this case tested nothing")
                continue
            files[rel] = files[rel].replace(before, after, 1)
            if files[rel] == COMPLETE[rel]:
                failures.append(f"{name}: the fixture came back unchanged")
                continue

        with tempfile.TemporaryDirectory() as root:
            write_tree(root, files)
            result = subprocess.run(
                [sys.executable, GATE, "--root", root], capture_output=True, text=True
            )

        if must_refuse and result.returncode == 0:
            failures.append(f"{name}: the gate stayed green — nothing depends on that rule")
        elif not must_refuse and result.returncode != 0:
            failures.append(f"{name}: the gate refused a complete tree\n{result.stdout}")
        else:
            print(f"  {name} → {'refused' if result.returncode else 'allowed'}")

    if failures:
        print("an-authorised-hatch-reaches-every-surface.test: FAILED")
        for f in failures:
            print(f"  - {f}")
        return 1

    print(f"an-authorised-hatch-reaches-every-surface.test: {len(CASES)} cases pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
