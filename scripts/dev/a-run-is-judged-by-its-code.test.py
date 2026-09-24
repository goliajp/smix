#!/usr/bin/env python3
"""What `a-run-is-judged-by-its-code.py` must answer.

Its subject is one shape: a script running a flow straight through the
binary, so that whatever smix reported, the script's next line reads the
exit status as its own rule's failure. And the escape next to it: a
`# raw run:` note must give a reason and must sit on a raw run.

It must not flag the wrapper, `smix runner …`, prose that mentions
`smix run`, a heredoc that does, or a raw run that says why.
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
GATE = os.path.join(ROOT, "scripts", "dev", "a-run-is-judged-by-its-code.py")

HEAD = 'source "$ROOT/scripts/lib/e2e-binary.sh"\n'

MUST_FLAG = {
    "the binary's run inside a substitution": (
        HEAD + 'out="$(SMIX_RUNNER_PORT="$P" "$SMIX" run --device "$D" "$f" 2>&1)" || rc=$?\n'
    ),
    "SMIX_BIN's run behind env -u": (
        HEAD + 'env -u X SMIX_RUNNER_PORT="$P" "$SMIX_BIN" run --device "$D" "$f" >"$l" 2>&1 || rc=$?\n'
    ),
    "a run on a continuation line": (
        HEAD + 'SMIX_RUNNER_PORT="$P" with_deadline 120 \\\n  "$SMIX" run --device "$D" "$f" \\\n  >"$l" 2>&1 || fail "x"\n'
    ),
    "a bare smix run from PATH": (
        HEAD + 'if smix run "$Y" --device "$D"; then fail "x"; fi\n'
    ),
    "a raw run note with no reason": (
        HEAD + '# raw run:\n"$SMIX" run --device "$D" "$f" || rc=$?\n'
    ),
    "a raw run note with no raw run under it": (
        HEAD + '# raw run: the flow is refused before any step on purpose\n"$SMIX_RUN" --device "$D" "$f"\n'
    ),
    "the wrapper called without sourcing what defines it": (
        'out="$("$SMIX_RUN" --device "$D" "$f" 2>&1)" || rc=$?\n'
    ),
}

MUST_PASS = {
    "the wrapper": (
        HEAD + 'out="$(SMIX_RUNNER_PORT="$P" with_deadline 120 "$SMIX_RUN" --device "$D" "$f" 2>&1)" || rc=$?\n'
    ),
    "runner verbs": (
        HEAD + '"$SMIX" runner up "$D" --bundle "$B" >/dev/null 2>&1 || fail "up"\nsmix runner down --device "$D"\n'
    ),
    "prose that mentions smix run": (
        HEAD + 'fail "smix run did not answer within 120 s"\nlog \'smix run the recorded flow\'\n'
    ),
    "a comment that mentions it": (
        HEAD + '# "$SMIX" run --device "$D" is what this used to do\n'
    ),
    "a heredoc that mentions it": (
        HEAD + "cat >\"$W/notes\" <<'EOF'\n\"$SMIX\" run --device x flow.yaml\nsmix run flow.yaml\nEOF\n"
        '"$SMIX_RUN" --device "$D" "$f"\n'
    ),
    "a raw run that says why": (
        HEAD + '# raw run: this run must be refused before any step, and the refusal is judged\n'
        'OUT="$("$SMIX" run /nonexistent.yaml --device "$D" 2>&1)"\n'
    ),
    "a raw run with the reason on its own line": (
        HEAD + '"$SMIX" run --check "$f"  # raw run: a host-side parse, no device involved\n'
    ),
}

problems: list[str] = []


def verdict(body: str) -> tuple[int, str]:
    with tempfile.TemporaryDirectory() as t:
        os.makedirs(os.path.join(t, "scripts", "dev"))
        with open(os.path.join(t, "scripts", "dev", "subject.sh"), "w") as fh:
            fh.write("#!/usr/bin/env bash\n" + body)
        # One clean script that uses the wrapper, so neither an empty scan
        # nor a tree with no wrapper call is what decides.
        with open(os.path.join(t, "scripts", "dev", "other.sh"), "w") as fh:
            fh.write('#!/usr/bin/env bash\n' + HEAD + '"$SMIX_RUN" --device "$D" "$f"\n')
        r = subprocess.run(
            [sys.executable, GATE, "--root", t], capture_output=True, text=True, check=False
        )
        return r.returncode, r.stdout + r.stderr


for name, body in MUST_FLAG.items():
    rc, out = verdict(body)
    if rc == 0:
        problems.append(f"did not flag: {name}\n{out}")
    elif "subject.sh" not in out:
        problems.append(f"red for another reason than the subject: {name}\n{out}")

for name, body in MUST_PASS.items():
    rc, out = verdict(body)
    if rc != 0:
        problems.append(f"flagged: {name}\n{out}")

# Nothing to scan, and nothing using the wrapper, are both red.
with tempfile.TemporaryDirectory() as t:
    os.makedirs(os.path.join(t, "scripts"))
    r = subprocess.run([sys.executable, GATE, "--root", t], capture_output=True, text=True, check=False)
    if r.returncode == 0:
        problems.append("an empty tree passed")
with tempfile.TemporaryDirectory() as t:
    os.makedirs(os.path.join(t, "scripts", "dev"))
    with open(os.path.join(t, "scripts", "dev", "x.sh"), "w") as fh:
        fh.write("#!/usr/bin/env bash\necho hi\n")
    r = subprocess.run([sys.executable, GATE, "--root", t], capture_output=True, text=True, check=False)
    if r.returncode == 0:
        problems.append("a tree where nothing calls the wrapper passed")

for p in problems:
    print(p)
if problems:
    print(f"a-run-is-judged-by-its-code.test: {len(problems)} problem(s)")
    sys.exit(1)
print(
    f"a-run-is-judged-by-its-code.test: {len(MUST_FLAG)} shapes flagged, "
    f"{len(MUST_PASS)} left alone, empty and wrapper-less trees red"
)
