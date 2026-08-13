#!/usr/bin/env python3
"""The probes-name-the-app gate can still go red.

Six fixture trees, each one thing away from a complete one. Five have to
be refused and one has to pass — a gate that cannot be made to fail is
indistinguishable from one that always passes.

Two of them were written because something was already wrong. The last
case carries a call the formatter split across lines, and the gate's
first draft read one line at a time and called it unnamed — wrapping a
call must not be how it becomes exempt, in either direction. The
almost-empty tree was added after removing the floor left this harness
green: a floor with no case is not a floor.
"""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "probes-name-the-app.py")

RUNNER_RS = """\
pub fn probe_session(port: u16) -> SessionProbe {
    // unnamed-probe: this is the unnamed face itself.
    probe_session_for(port, None)
}

pub fn probe_session_for(port: u16, bundle: Option<&str>) -> SessionProbe {
    read_get(port, "/tree", bundle)
}

pub fn decide_after_timeout(target: RunnerTarget<'_>) -> AfterTimeout {
    AfterTimeout::GiveUp
}

pub fn up_on(port: u16, bundle: Option<&str>) -> Result<(), String> {
    let probe = probe_session_for(port, bundle);
    let ready = probe_session_for(port, bundle).usable();
    Ok(())
}
"""

MCP_RS = """\
async fn smix_use(port: u16) -> Result<(), String> {
    if let Some(why) = smix_capsule::runner::probe_session_for(
        port,
        params.bundle_id.as_deref(),
    )
    .unusable_because()
    {
        return Err(why.to_string());
    }
    Ok(())
}
"""

TREE = {
    "smix-capsule/src/runner.rs": RUNNER_RS,
    "smix-mcp/src/main.rs": MCP_RS,
}


def build(root, edits):
    for rel, body in TREE.items():
        for before, after in edits.get(rel, []):
            assert before in body, f"fixture edit no longer applies: {before!r}"
            body = body.replace(before, after, 1)
        path = os.path.join(root, "crates", rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(body)


def run(edits):
    with tempfile.TemporaryDirectory() as root:
        build(root, edits)
        return subprocess.run(
            [sys.executable, GATE, root], capture_output=True, text=True
        )


CASES = [
    (
        "an unnamed probe with nothing said about it",
        {
            "smix-capsule/src/runner.rs": [
                ("    // unnamed-probe: this is the unnamed face itself.\n", "")
            ]
        },
        False,
        "Silence must never be how",
    ),
    (
        "an unnamed probe declared with no reason",
        {
            "smix-capsule/src/runner.rs": [
                (
                    "    // unnamed-probe: this is the unnamed face itself.",
                    "    // unnamed-probe:",
                )
            ]
        },
        False,
        "the declaration is the reason",
    ),
    (
        "named with None, which is not a name",
        {
            "smix-capsule/src/runner.rs": [
                ("    let probe = probe_session_for(port, bundle);", "    let probe = probe_session_for(port, None);")
            ]
        },
        False,
        "Silence must never be how",
    ),
    (
        "the subject renamed out from under the gate",
        {
            "smix-capsule/src/runner.rs": [
                ("pub fn decide_after_timeout", "pub fn decide_what_next")
            ]
        },
        False,
        "is not defined in",
    ),
    (
        "a tree with almost no probes in it",
        {
            # One named site left, so MIN_NAMED is satisfied and the only
            # thing that can refuse this is the floor on how many sites
            # there are. Without that pairing the floor would be a rule
            # with no case, which is how a floor stops being one.
            "smix-capsule/src/runner.rs": [
                ("    // unnamed-probe: this is the unnamed face itself.\n    probe_session_for(port, None)", "    todo!()"),
                ("    let probe = probe_session_for(port, bundle);\n", ""),
            ],
            "smix-mcp/src/main.rs": [
                ("smix_capsule::runner::probe_session_for(\n        port,\n        params.bundle_id.as_deref(),\n    )\n    .unusable_because()", "None"),
            ],
        },
        False,
        "reading air",
    ),
    ("the complete tree, wrapped call and all", {}, True, "clean"),
]

failures = []
for name, edits, want_clean, marker in CASES:
    result = run(edits)
    clean = result.returncode == 0
    output = result.stdout + result.stderr
    if clean != want_clean:
        failures.append(
            f"{name}: expected {'clean' if want_clean else 'FAIL'}, got "
            f"{'clean' if clean else 'FAIL'}\n{output}"
        )
    elif marker not in output:
        failures.append(f"{name}: verdict is right and says nothing about why\n{output}")
    if "Traceback" in output:
        failures.append(f"{name}: the gate crashed rather than judging\n{output}")

# The wrapped call in the complete tree has to have counted as named, or
# case 5 passes for the wrong reason — it would be clean because the gate
# saw fewer sites, not because it read them right.
clean_output = run({}).stdout
if "3 named" not in clean_output:
    failures.append(
        "the complete tree does not report three named call sites, so the "
        f"wrapped one was not read: {clean_output}"
    )

if failures:
    print("probes-name-the-app.test: FAILED")
    for failure in failures:
        print(f"  - {failure}")
    sys.exit(1)

print(f"probes-name-the-app.test: {len(CASES)} cases pass")
