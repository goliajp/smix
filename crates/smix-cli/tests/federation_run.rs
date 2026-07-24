//! `smix run --nodes` CLI surface: flag conflicts, roster errors, and
//! the local flow-existence fast-fail — all device-free, no network
//! dialed (the one test with a syntactically valid roster uses a
//! TEST-NET-1 host and a flow path that fails the local existence
//! check, which the lane runs before any ssh).
//!
//! `SMIX_UDID` / `SMIX_RUNNER_PORT` are removed on every invocation:
//! clap counts env-sourced values as present, so an exported SMIX_UDID
//! would trip the `--nodes` / `--device` conflict and shadow the
//! behaviour under test.

use std::process::Command;

fn smix() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_smix"));
    cmd.env_remove("SMIX_UDID").env_remove("SMIX_RUNNER_PORT");
    cmd
}

fn write_file(dir: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, content).expect("write file");
    p
}

fn write_flow(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    write_file(dir, name, "appId: smix.federation.test\n---\n- launchApp\n")
}

#[test]
fn nodes_conflicts_with_parallel() {
    let dir = tempfile::tempdir().expect("tempdir");
    let flow = write_flow(dir.path(), "f.yaml");
    let roster = write_file(dir.path(), "n.yaml", "nodes: []\n");

    let out = smix()
        .arg("run")
        .arg(&flow)
        .arg("--nodes")
        .arg(&roster)
        .args(["--parallel", "2"])
        .output()
        .expect("run smix");

    assert_eq!(out.status.code(), Some(2), "clap usage error exits 2");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "conflict must be named:\n{stderr}"
    );
}

#[test]
fn nodes_conflicts_with_device() {
    let dir = tempfile::tempdir().expect("tempdir");
    let flow = write_flow(dir.path(), "f.yaml");
    let roster = write_file(dir.path(), "n.yaml", "nodes: []\n");

    let out = smix()
        .arg("run")
        .arg(&flow)
        .arg("--nodes")
        .arg(&roster)
        .args(["--device", "FAKE-UDID-1"])
        .output()
        .expect("run smix");

    assert_eq!(out.status.code(), Some(2), "clap usage error exits 2");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "conflict must be named:\n{stderr}"
    );
}

#[test]
fn a_malformed_roster_is_named_and_exits_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let flow = write_flow(dir.path(), "f.yaml");
    let roster = write_file(dir.path(), "n.yaml", "nodes: []\n");

    let out = smix()
        .arg("run")
        .arg(&flow)
        .arg("--nodes")
        .arg(&roster)
        .output()
        .expect("run smix");

    assert_eq!(out.status.code(), Some(1), "roster error exits 1");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("lists no nodes"),
        "NodesError must surface:\n{stderr}"
    );
}

#[test]
fn a_flow_missing_locally_fails_before_any_ssh() {
    let dir = tempfile::tempdir().expect("tempdir");
    // TEST-NET-1 host: never dialed — the local flow-existence check
    // runs before the readiness gate, which this test pins.
    let roster = write_file(
        dir.path(),
        "n.yaml",
        "nodes:\n  - name: far\n    host: 192.0.2.1\n    repo: /repo\n    devices: [sim-1]\n",
    );
    let missing = dir.path().join("no-such-flow.yaml");

    let out = smix()
        .arg("run")
        .arg(&missing)
        .arg("--nodes")
        .arg(&roster)
        .output()
        .expect("run smix");

    assert_eq!(out.status.code(), Some(1), "missing flow exits 1");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(missing.to_str().expect("utf-8 path")),
        "the missing flow path must be named:\n{stderr}"
    );
}
