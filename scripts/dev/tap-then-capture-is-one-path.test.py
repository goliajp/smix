#!/usr/bin/env python3
"""The one-path gate can still go red.

Six fixture trees, each one thing away from a complete one. Five have to
be refused and one has to pass — a gate that cannot be made to fail is
indistinguishable from one that always passes, and this repository has
shipped both kinds before noticing.

Case 5 is the one that pairs with the floor: a tree where only one
surface reaches the combined action. Without it, `MIN_SURFACES` would be
a rule with no case, which is how a floor stops being one.
"""

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
GATE = os.path.join(HERE, "tap-then-capture-is-one-path.py")

SDK_RS = """\
pub struct CapturedAfterTap {
    pub png: Vec<u8>,
    pub via: &'static str,
    pub gap_ms: u64,
}

pub async fn tap_then_capture_with(
    driver: &dyn Driver,
    selector: &Selector,
) -> Result<(ActOutcome, CapturedAfterTap), ExpectationFailure> {
    let outcome = driver.tap(selector, None).await?;
    let tapped_at = std::time::Instant::now();
    let runner = driver.runner_client().ok_or_else(no_runner)?;
    let png = runner.screenshot().await.map_err(|e| {
        if screenshot_route_absent(&e) {
            return predates_the_route(e);
        }
        transport_to_failure(e)
    })?;
    Ok((outcome, CapturedAfterTap { png, via: "runner", gap_ms: 0 }))
}

fn screenshot_route_absent(err: &RunnerTransportError) -> bool {
    matches!(err, RunnerTransportError::NonSuccessStatus { status, .. } if *status == 501)
}

impl App {
    pub async fn tap_then_capture(&self, selector: &Selector) -> Result<(), ()> {
        tap_then_capture_with(self.driving()?, selector).await
    }
}
"""

CLI_RS = """\
pub async fn cmd_tap_then_screenshot(port: u16) -> Result<(), ActError> {
    let (outcome, captured) =
        smix_sdk::tap_then_capture_with(&d, Some(d.runner()), &selector).await?;
    Ok(())
}
"""

MCP_RS = """\
async fn smix_tap_then_screenshot(&self) -> Result<CallToolResult, McpError> {
    let (outcome, captured) = app.tap_then_capture(&target).await?;
    Ok(())
}
"""

TREE = {
    "smix-sdk/src/lib.rs": SDK_RS,
    "smix-cli/src/act.rs": CLI_RS,
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
        "the combined action renamed out from under the gate",
        {
            "smix-sdk/src/lib.rs": [
                ("pub async fn tap_then_capture_with(", "pub async fn tap_and_shoot(")
            ]
        },
        False,
        "is not defined in",
    ),
    (
        "the answer type is gone",
        {"smix-sdk/src/lib.rs": [("pub struct CapturedAfterTap {", "pub struct Shot {")]},
        False,
        "reading air",
    ),
    (
        "the frame no longer comes from the runner",
        {
            "smix-sdk/src/lib.rs": [
                ("let png = runner.screenshot().await", "let png = device_tooling().await")
            ]
        },
        False,
        "never asks the runner for a frame",
    ),
    (
        "the answer stops saying where the frame came from",
        {"smix-sdk/src/lib.rs": [('via: "runner"', "via: provenance()")]},
        False,
        "does not name",
    ),
    (
        "a second hand takes the picture again",
        {
            "smix-sdk/src/lib.rs": [
                ('Ok((outcome, CapturedAfterTap { png, via: "runner", gap_ms: 0 }))',
                 'let via = if png.is_empty() { "device-tooling" } else { "runner" };\n'
                 "    Ok((outcome, CapturedAfterTap { png, via, gap_ms: 0 }))")
            ]
        },
        False,
        "names \"device-tooling\" again",
    ),
    (
        "an old runner and a failed capture collapse into one answer",
        {"smix-sdk/src/lib.rs": [("if screenshot_route_absent(&e) {", "if false {")]},
        False,
        "does not ask screenshot_route_absent",
    ),
    (
        "an empty frame is reachable again",
        {
            "smix-sdk/src/lib.rs": [
                ("let png = runner.screenshot().await.map_err(|e| {",
                 "let png = Vec::new();\n    let _ = runner.screenshot().await.map_err(|e| {")
            ]
        },
        False,
        "empty frame",
    ),
    (
        "only one surface reaches it",
        {"smix-mcp/src/main.rs": [("app.tap_then_capture(&target)", "app.tap(&target)")]},
        False,
        "surface(s) reach the combined action",
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
    print("tap-then-capture-is-one-path.test: FAILED")
    for failure in failures:
        print(f"  - {failure}")
    sys.exit(1)

print(f"tap-then-capture-is-one-path.test: {len(CASES)} cases pass")
