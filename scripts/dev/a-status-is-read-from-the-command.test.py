#!/usr/bin/env python3
"""What `a-status-is-read-from-the-command.py` must answer.

The gate's subject is one shape: an exit status read after a pipeline
whose last stage is a filter. The status is the filter's — `grep -v`
exits 1 when it selected nothing, `tail` exits 0 whatever came before —
so the script judges the wrong program. And one shape next to it: `$?`
read straight after `|| true`, which is always 0.

It must not flag a pipeline whose last stage IS the thing being judged
(`printf … | verdict_fn`), nor a status taken before the filtering.
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
GATE = os.path.join(ROOT, "scripts", "dev", "a-status-is-read-from-the-command.py")

MUST_FLAG = {
    "substitution ending in grep, status after": (
        'out="$("$SMIX" run --device "$D" "$flow" 2>&1 | grep -v \'^noise:\')" || rc=$?\n'
    ),
    "substitution ending in tail, status on the next line": (
        'out="$(with_deadline 240 "$SMIX" sim boot "$A" 2>&1 | tail -3)"\nrc=$?\n'
    ),
    "plain pipeline ending in head, status next": (
        '"$SMIX" tree --json | head -1\nstatus=$?\n'
    ),
    "status straight after || true": (
        '"$SMIX" runner down || true\nif [ $? -ne 0 ]; then echo x; fi\n'
    ),
    "the && rc=0 || rc=$? spelling": (
        'out="$(cmd arg | sed s/a/b/)" && rc=0 || rc=$?\n'
    ),
}

MUST_PASS = {
    "last stage is the verdict function": (
        'out="$(printf \'%s\\n\' "$input" | e2e_verdict)" && rc=0 || rc=$?\n'
    ),
    "status taken first, filtered after": (
        'out="$("$SMIX" run "$flow" 2>&1)" || rc=$?\nshown="$(printf \'%s\' "$out" | tail -3)"\n'
    ),
    "a filter whose status nobody reads": (
        'printf \'%s\' "$out" | grep -q done && echo yes\n'
    ),
    "|| true then something that is not a status read": (
        '"$SMIX" runner down || true\nrm -rf "$WORK"\n'
    ),
    "a || inside a quoted string": (
        'echo "a | b" \nrc=$?\n'
    ),
    "a comment mentioning the shape": (
        '# out="$(cmd | grep x)" || rc=$?\n'
    ),
}

problems: list[str] = []


def verdict(body: str) -> tuple[int, str]:
    with tempfile.TemporaryDirectory() as t:
        os.makedirs(os.path.join(t, "scripts", "dev"))
        with open(os.path.join(t, "scripts", "dev", "subject.sh"), "w") as fh:
            fh.write("#!/usr/bin/env bash\n" + body)
        # One unrelated clean script, so an empty scan is not what passes.
        with open(os.path.join(t, "scripts", "dev", "other.sh"), "w") as fh:
            fh.write("#!/usr/bin/env bash\necho hi\n")
        r = subprocess.run(
            [sys.executable, GATE, "--root", t], capture_output=True, text=True, check=False
        )
        return r.returncode, r.stdout + r.stderr


for name, body in MUST_FLAG.items():
    code, out = verdict(body)
    if code == 0:
        problems.append(f"{name}: not flagged\n{out}")
    elif "Traceback" in out:
        problems.append(f"{name}: red by crashing, not by verdict\n{out}")
    elif "subject.sh:" not in out:
        problems.append(f"{name}: red without naming the line\n{out}")

for name, body in MUST_PASS.items():
    code, out = verdict(body)
    if code != 0:
        problems.append(f"{name}: flagged, and it is not the shape\n{out}")

# A tree with no shell scripts at all is not a clean tree: it is a scan
# that read nothing.
with tempfile.TemporaryDirectory() as t:
    os.makedirs(os.path.join(t, "scripts"))
    r = subprocess.run([sys.executable, GATE, "--root", t], capture_output=True, text=True)
    if r.returncode == 0:
        problems.append(f"an empty scripts/ passed:\n{r.stdout}")

if problems:
    print("a-status-is-read-from-the-command.test: FAIL")
    for p in problems:
        print(f"  - {p}")
    sys.exit(1)
print(
    f"a-status-is-read-from-the-command.test: {len(MUST_FLAG)} shapes flagged, "
    f"{len(MUST_PASS)} near-misses passed, an empty tree refused"
)
