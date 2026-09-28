//! Real-sim e2e: spawn the cargo-built `smix-maestro` bin against a yaml
//! flow on a booted simulator + SmixRunnerCore reachable on :22087.
//!
//! `#[ignore]` by default — requires a booted simulator, SmixRunnerCore
//! reachable on :22087, and the target app installed. Run manually with
//! `SMIX_REAL_SIM_FLOW`, `SMIX_UDID`, and `SMIX_BUNDLE_ID` set:
//!   `cargo test --release -p smix-adapter-maestro --test real_sim_cli_bin -- --ignored`

#![allow(clippy::unwrap_used)]

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "real-sim e2e: cargo-built smix-maestro bin needs a booted sim + SmixRunnerCore :22087; full tier only"]
async fn real_sim_cli_bin_device_detail_runs_and_exits_zero_or_three() {
    let bin = env!("CARGO_BIN_EXE_smix-maestro");
    let flow = match std::env::var("SMIX_REAL_SIM_FLOW") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("real_sim_cli_bin SKIP: SMIX_REAL_SIM_FLOW not set");
            return;
        }
    };
    let udid = std::env::var("SMIX_UDID").unwrap_or_default();
    let bundle_id =
        std::env::var("SMIX_BUNDLE_ID").unwrap_or_else(|_| "com.example.app".to_string());
    let output = std::process::Command::new(bin)
        .args([
            "test",
            &flow,
            "--runner-port",
            "22087",
            "--udid",
            &udid,
            "--bundle-id",
            &bundle_id,
        ])
        .output()
        .expect("spawn smix-maestro");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    eprintln!("--- smix-maestro bin exit={code} ---");
    eprintln!("--- stdout ({} bytes) ---", stdout.len());
    eprintln!("{stdout}");
    eprintln!("--- stderr ({} bytes) ---", stderr.len());
    eprintln!("{stderr}");

    // exit code ∈ {0, 3} — real-sim app-state may legitimately fail a
    // runtime SDK step; accept the graceful path.
    assert!(
        code == 0 || code == 3,
        "expected exit code 0 or 3, got {code}\nstdout={stdout}\nstderr={stderr}"
    );

    // stderr emits progress lines (`STEP N/M: ...`).
    assert!(
        stderr.contains("STEP 1/"),
        "stderr missing 'STEP 1/' progress\nstderr={stderr}"
    );

    // On success, stdout carries the JSON summary `{"ok":true,...}`.
    if code == 0 {
        assert!(
            stdout.contains("\"ok\":true"),
            "exit 0 but stdout missing '\"ok\":true'\nstdout={stdout}"
        );
    }

    // yaml parsed (not exit 2).
    assert_ne!(code, 2, "yaml parse failed unexpectedly");

    // command body valid (not exit 4).
    assert_ne!(code, 4, "UnknownKey / UnknownDirection unexpected");

    // reference chain ok (not exit 5).
    assert_ne!(code, 5, "RunFlowCycle / Io unexpected");

    // runner reachable (not exit 6).
    assert_ne!(code, 6, "runner unreachable — check :22087 health");
}
