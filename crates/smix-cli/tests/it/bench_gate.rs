//! `smix bench` fails on an injected regression and passes without one.
//!
//! Driven through `--baseline-file` + `--current-file` so the gate is
//! exercised on fixed numbers, not on a live measurement: the verdict
//! logic is what this checks, and coupling it to the machine's timing
//! would make the test as flaky as the thing it guards. The live
//! measurement path has its own coverage in the module's unit tests
//! (the pure `compare`).

use std::process::Command;

fn write_json(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, body).expect("write fixture");
    p
}

fn run_bench(baseline: &std::path::Path, current: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_smix"))
        .args(["bench", "--baseline-file"])
        .arg(baseline)
        .arg("--current-file")
        .arg(current)
        .output()
        .expect("run smix bench")
}

#[test]
fn an_injected_regression_fails_the_gate_and_names_the_metric() {
    let dir = tempfile::tempdir().expect("tempdir");
    let base = write_json(dir.path(), "base.json", r#"{"metrics":{"m":100.0}}"#);
    // 110 is +10%, past the 5% tolerance.
    let cur = write_json(dir.path(), "cur.json", r#"{"metrics":{"m":110.0}}"#);

    let out = run_bench(&base, &cur);
    assert!(
        !out.status.success(),
        "a 10% regression must fail the gate; exit was {:?}",
        out.status.code()
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains('m'),
        "the report must name the metric:\n{stdout}"
    );
    assert!(
        stdout.contains("REGRESSION"),
        "the report must label it a regression:\n{stdout}"
    );
}

#[test]
fn a_change_within_tolerance_passes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let base = write_json(dir.path(), "base.json", r#"{"metrics":{"m":100.0}}"#);
    // 104 is +4%, inside tolerance.
    let cur = write_json(dir.path(), "cur.json", r#"{"metrics":{"m":104.0}}"#);

    let out = run_bench(&base, &cur);
    assert!(
        out.status.success(),
        "a 4% change is within tolerance and must pass; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A metric that stopped being measured must fail, not pass by absence.
#[test]
fn a_disappeared_metric_fails_the_gate() {
    let dir = tempfile::tempdir().expect("tempdir");
    let base = write_json(
        dir.path(),
        "base.json",
        r#"{"metrics":{"kept":100.0,"gone":100.0}}"#,
    );
    let cur = write_json(dir.path(), "cur.json", r#"{"metrics":{"kept":100.0}}"#);

    let out = run_bench(&base, &cur);
    assert!(
        !out.status.success(),
        "a disappeared metric must fail the gate; exit was {:?}",
        out.status.code()
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("MISSING"),
        "the report must say a metric went missing"
    );
}
