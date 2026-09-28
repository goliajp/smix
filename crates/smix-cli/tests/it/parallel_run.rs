//! `smix run --parallel N` shards across sims and aggregates.
//!
//! Exercised without a device: two fake UDIDs with no runner make each
//! child `smix run` fail fast on connect, so the test checks the
//! orchestration — that the parallel path is taken, that both shards are
//! spawned, and that a failed shard fails the whole run — not the flow
//! execution (which the single-sim path already covers). `--parallel 1`
//! must NOT take the parallel path, keeping the default byte-identical.

use std::process::Command;

fn write_flow(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, "appId: smix.parallel.test\n---\n- launchApp\n").expect("write flow");
    p
}

#[test]
fn parallel_two_shards_across_two_sims_and_a_failed_shard_fails_the_run() {
    let dir = tempfile::tempdir().expect("tempdir");
    let f1 = write_flow(dir.path(), "a.yaml");
    let f2 = write_flow(dir.path(), "b.yaml");

    let out = Command::new(env!("CARGO_BIN_EXE_smix"))
        .args(["run"])
        .arg(&f1)
        .arg(&f2)
        .args([
            "--parallel",
            "2",
            "--device",
            "FAKE-UDID-1",
            "--also-device",
            "FAKE-UDID-2",
            // no runner on either → children fail fast on connect
            "--retry",
            "1",
        ])
        .output()
        .expect("run smix");

    let stderr = String::from_utf8_lossy(&out.stderr);
    // The parallel path was taken and both sims were dispatched.
    assert!(
        stderr.contains("smix run --parallel: sim FAKE-UDID-1")
            && stderr.contains("smix run --parallel: sim FAKE-UDID-2"),
        "both shards must be dispatched:\n{stderr}"
    );
    // A shard that could not complete fails the whole run.
    assert!(
        !out.status.success(),
        "a failed shard must fail the batch; exit was {:?}",
        out.status.code()
    );
}

/// `--parallel 1` stays on the single-sim path — no sharding, no
/// subprocess fan-out — so the default is byte-identical.
#[test]
fn parallel_one_does_not_take_the_parallel_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let f1 = write_flow(dir.path(), "a.yaml");

    let out = Command::new(env!("CARGO_BIN_EXE_smix"))
        .args(["run"])
        .arg(&f1)
        .args(["--parallel", "1", "--device", "FAKE-UDID-1", "--retry", "1"])
        .output()
        .expect("run smix");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("smix run --parallel:"),
        "--parallel 1 must not fan out to the parallel path:\n{stderr}"
    );
}
