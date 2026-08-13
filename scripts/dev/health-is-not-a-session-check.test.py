#!/usr/bin/env python3
"""The health-is-not-a-session gate can still go red.

Six fixture trees, each differing from a complete one in a single way.
Five have to be refused and one has to pass; a gate that cannot be made
to fail is indistinguishable from one that always passes, and this
repository has shipped both kinds before noticing.

Case 5 is the one worth naming: a tree with no `health_ok` in it at all
must be **refused**, not waved through for having no violations. That is
the shape a renamed function leaves behind, and a scan that reads air
reports coverage it does not have.
"""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "health-is-not-a-session-check.py")

# A complete tree: ten call sites, two of them deciders that ask, one
# deferred with a reason, the rest declared as deciding nothing. The
# probe is defined where the gate expects it and called from two files.
RUNNER_RS = """\
pub fn health_ok(port: u16) -> bool {
    read_health_bytes(port, 64).is_ok()
}

pub fn probe_session(port: u16) -> SessionProbe {
    read_get(port, "/tree")
}

pub fn decide_already_serving(probe: &SessionProbe) -> AlreadyServing {
    AlreadyServing::ReportUp
}

pub fn up_on(root: &Path, port: u16) -> Result<(), String> {
    // health-decider: whether this port is already serving usefully.
    if health_ok(port) {
        let probe = probe_session(port);
        return Ok(());
    }
    // health-decider: whether the bring-up is finished.
    let ready = health_ok(port) && probe_session(port).usable();
    if ready {
        return Ok(());
    }
    Ok(())
}

fn wait_health_back(port: u16) -> bool {
    // health-not-a-decider: waits for the socket to come back.
    if health_ok(port) {
        return true;
    }
    false
}

fn down_with(port: u16) {
    // health-not-a-decider: waits for the port to go quiet.
    while health_ok(port) {}
    // health-not-a-decider: whether anything still answers afterwards.
    if health_ok(port) {
        eprintln!("still up");
    }
}

fn try_soft_cycle(port: u16) {
    // health-not-a-decider: whether there is a host to ask for a bounce.
    if !health_ok(port) {
        return;
    }
}
"""

ANDROID_RS = """\
use crate::runner::health_ok;

pub fn up(port: u16) -> Result<(), String> {
    // health-decider: whether this port is already serving — deferred:
    // the question to ask instead is below, and moving it here is a
    // claim about a device this was not run against.
    if health_ok(port) {
        return Ok(());
    }
    // health-decider: whether the bring-up is finished.
    if health_ok(port) {
        automation_sees_an_app(port)?;
        return Ok(());
    }
    Ok(())
}
"""

CLI_RS = """\
fn doctor(port: u16) {
    // health-not-a-decider: doctor prints whether a runner answers.
    let up = smix_capsule::runner::health_ok(port);
}
"""

MCP_RS = """\
async fn smix_use(port: u16) -> Result<(), String> {
    // health-decider: whether this server is already driving it.
    if smix_capsule::health_ok(port) {
        if let Some(why) = probe_session(port).unusable_because() {
            return Err(why.to_string());
        }
    }
    Ok(())
}
"""

TREE = {
    "smix-capsule/src/runner.rs": RUNNER_RS,
    "smix-capsule/src/runner_android.rs": ANDROID_RS,
    "smix-cli/src/main.rs": CLI_RS,
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
        "a decider that never asks the session",
        {
            "smix-capsule/src/runner.rs": [
                ("        let probe = probe_session(port);\n", "")
            ]
        },
        False,
        "never asks",
    ),
    (
        "a call site in neither list",
        {
            "smix-cli/src/main.rs": [
                ("    // health-not-a-decider: doctor prints whether a runner answers.\n", "")
            ]
        },
        False,
        "Not listed must never be the way",
    ),
    (
        "a deferral with no reason",
        {
            "smix-capsule/src/runner_android.rs": [
                (
                    "— deferred:\n    // the question to ask instead is below, and moving it here is a\n"
                    "    // claim about a device this was not run against.",
                    "— deferred:",
                )
            ]
        },
        False,
        "does not say why",
    ),
    (
        "the probe renamed out from under the gate",
        {
            "smix-capsule/src/runner.rs": [("pub fn probe_session", "pub fn ask_the_app")],
            "smix-mcp/src/main.rs": [("probe_session(port)", "ask_the_app(port)")],
        },
        False,
        "is not defined in",
    ),
    (
        "a tree with no health_ok in it at all",
        {
            rel: [("health_ok", "ping")] * body.count("health_ok")
            for rel, body in TREE.items()
        },
        False,
        "reading air",
    ),
    ("the complete tree", {}, True, "clean"),
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

if failures:
    print("health-is-not-a-session-check.test: FAILED")
    for failure in failures:
        print(f"  - {failure}")
    sys.exit(1)

print(f"health-is-not-a-session-check.test: {len(CASES)} cases pass")
