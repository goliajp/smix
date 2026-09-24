#!/usr/bin/env python3
"""What `a-script-drives-this-tree.py` must answer.

Its subject: a script that reaches smix through the PATH — a lookup
(`command -v smix`, `which smix`, `shutil.which("smix")`) or a bare
`smix <verb>` with no `smix()` function that runs `$SMIX`. The PATH
holds the installed release, so such a script judges the wrong binary.

It must not flag a script that wraps `smix()` around `$SMIX`, a name
that merely starts with `smix`, prose in comments, quoted text, or a
heredoc body — and it must still see a bare call inside `"$( … )"`,
and a lookup inside a string that spans lines is prose, not code.
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
GATE = os.path.join(ROOT, "scripts", "dev", "a-script-drives-this-tree.py")

MUST_FLAG = {
    "a PATH default for SMIX_BIN": ('SMIX_BIN="${SMIX_BIN:-$(command -v smix)}"\n', "sh"),
    "which smix": ('BIN=$(which smix)\n', "sh"),
    "shutil.which in Python": ('import shutil\nBIN = shutil.which("smix")\n', "py"),
    "a bare verb at the start of a line": ('smix runner up "$U"\n', "sh"),
    "a bare verb after then": ('if [ x ]; then smix sim shutdown "$U"; fi\n', "sh"),
    "a bare verb inside a quoted substitution": ('said="$(smix runner down 2>&1)"\n', "sh"),
    "a bare verb after ||": ('true || smix sim boot "$U"\n', "sh"),
    "a wrapper that does not run $SMIX": ('smix() { command smix "$@"; }\nsmix sim boot x\n', "sh"),
    # A build of this tree chosen as the script's own default (M2): the
    # release one here, the debug one next door.
    "a default to the release build": ('SMIX="${SMIX_BIN:-$ROOT/target/release/smix}"\n', "sh"),
    "a default to the debug build": ('SMIX="${SMIX_BIN:-$ROOT/target/debug/smix}"\n', "sh"),
    "an unconditional release build": ('SMIX="$ROOT/target/release/smix"\n', "sh"),
    "a default used in place": ('"${SMIX_BIN:-$ROOT/target/debug/smix}" tree --json\n', "sh"),
    "a Python default": (
        'import argparse\nap = argparse.ArgumentParser()\n'
        'ap.add_argument("--binary", default="./target/release/smix")\n',
        "py",
    ),
    "a Python path join": (
        'import os\nB = os.path.join(ROOT, "target/release/smix")\n',
        "py",
    ),
}

MUST_PASS = {
    "a wrapper around $SMIX": ('smix() { "$SMIX" "$@" 2>&1; }\nsmix sim boot x\n', "sh"),
    "the resolved binary": ('"$SMIX" runner up "$U"\n', "sh"),
    "a longer name": ('command -v smix-mcp >/dev/null\nsmix-mcp --help\n', "sh"),
    "a comment": ('# smix runner up, then command -v smix\n', "sh"),
    "a trailing comment": ('true # then smix runner down\n', "sh"),
    "quoted prose": ('log "run smix runner up first"\n', "sh"),
    "prose spanning lines": ('fail "register one first:\n  smix sim register a --udid x"\n', "sh"),
    "a heredoc body": ("cat <<EOF\nsmix runner up\nEOF\necho done\n", "sh"),
    "a quoted heredoc body": ("cat <<'FLOW'\nsmix sim boot\nFLOW\n", "sh"),
    "a herestring is not a heredoc": ('read -r a <<<"$x"\n"$SMIX" tree\n', "sh"),
    "Python naming SMIX_BIN": ('import os\nBIN = os.environ["SMIX_BIN"]\n', "py"),
    "naming a binary for a child": ('SMIX_BIN="$ROOT/target/release/smix" bash gate.sh\n', "sh"),
    "naming the binary it built, for what follows": ('SMIX_BIN="$ROOT/target/release/smix"\n', "sh"),
    "exporting the binary a ship built": ('export SMIX_BIN="$ROOT/target/release/smix"\n', "sh"),
    "another host's binary over ssh": (
        'rssh "cd \'$REMOTE_REPO\' && target/release/smix sim list"\n',
        "sh",
    ),
    "prose about a path in Python": (
        '"""It was ./target/release/smix, relative to the caller."""\n',
        "py",
    ),
}

problems: list[str] = []


def verdict(body: str, ext: str) -> tuple[int, str]:
    with tempfile.TemporaryDirectory() as t:
        os.makedirs(os.path.join(t, "scripts", "dev"))
        with open(os.path.join(t, "scripts", "dev", f"subject.{ext}"), "w") as fh:
            fh.write(("#!/usr/bin/env bash\n" if ext == "sh" else "") + body)
        with open(os.path.join(t, "scripts", "dev", "other.sh"), "w") as fh:
            fh.write("#!/usr/bin/env bash\necho hi\n")
        r = subprocess.run(
            [sys.executable, GATE, "--root", t, "--min-scripts", "2"],
            capture_output=True,
            text=True,
            check=False,
        )
        return r.returncode, r.stdout + r.stderr


for name, (body, ext) in MUST_FLAG.items():
    code, out = verdict(body, ext)
    if code == 0:
        problems.append(f"{name}: not flagged\n{out}")
    elif "Traceback" in out:
        problems.append(f"{name}: red by crashing, not by verdict\n{out}")
    elif f"subject.{ext}:" not in out:
        problems.append(f"{name}: red without naming the line\n{out}")

for name, (body, ext) in MUST_PASS.items():
    code, out = verdict(body, ext)
    if code != 0:
        problems.append(f"{name}: flagged, and it is not the shape\n{out}")

# The resolver is the one file whose default is a build path; the same
# line anywhere else is a script picking its own binary.
RESOLVER_LINE = 'SMIX="${SMIX_BIN:-${ROOT}/target/debug/smix}"\n'
with tempfile.TemporaryDirectory() as t:
    os.makedirs(os.path.join(t, "scripts", "lib"))
    for name in ("e2e-binary.sh", "elsewhere.sh"):
        with open(os.path.join(t, "scripts", "lib", name), "w") as fh:
            fh.write("#!/usr/bin/env bash\n" + RESOLVER_LINE)
    r = subprocess.run(
        [sys.executable, GATE, "--root", t, "--min-scripts", "2"], capture_output=True, text=True
    )
    if "elsewhere.sh:2" not in r.stdout or "lib/e2e-binary.sh:" in r.stdout:
        problems.append(f"the resolver's exemption is wrong in one direction:\n{r.stdout}")

# Fewer scripts than the floor is a scan that lost its subject.
with tempfile.TemporaryDirectory() as t:
    os.makedirs(os.path.join(t, "scripts"))
    r = subprocess.run([sys.executable, GATE, "--root", t], capture_output=True, text=True)
    if r.returncode == 0 or "fewer than" not in r.stdout:
        problems.append(f"an empty scripts/ was not refused by name:\n{r.stdout}")

if problems:
    print("a-script-drives-this-tree.test: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)
print(
    f"a-script-drives-this-tree.test: {len(MUST_FLAG)} shapes flagged, "
    f"{len(MUST_PASS)} near-misses passed, an empty tree refused"
)
