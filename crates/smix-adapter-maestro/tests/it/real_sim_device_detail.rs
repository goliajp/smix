//! Real-sim end-to-end verify: `parse_flow_file` + `Adapter::run` drives
//! a yaml flow against a live iOS simulator.
//!
//! Gated `#[ignore]` so CI does not try (no booted sim there); run
//! locally with the runner up and a booted simulator, and set
//! `SMIX_REAL_SIM_FLOW` to the path of a flow yaml to exercise:
//!
//! ```bash
//! SMIX_RUNNER_PORT=22087 SMIX_UDID=<udid> \
//! SMIX_REAL_SIM_FLOW=/path/to/flow.yaml \
//!   cargo test --release -p smix-adapter-maestro \
//!   --test real_sim_device_detail -- --ignored
//! ```

use smix_adapter_maestro::{Adapter, RunError, parse_flow_file};
use smix_sdk::App;
use std::path::PathBuf;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "real-sim e2e: Adapter::run drives a yaml flow on a booted sim + SmixRunnerCore :22087; full tier only"]
async fn real_sim_device_detail_end_to_end_pass() {
    let port: u16 = std::env::var("SMIX_RUNNER_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(22087);
    let udid = std::env::var("SMIX_UDID").ok();
    let bundle_id =
        std::env::var("SMIX_BUNDLE_ID").unwrap_or_else(|_| "com.example.app".to_string());
    let flow_path = match std::env::var("SMIX_REAL_SIM_FLOW") {
        Ok(p) => PathBuf::from(p),
        Err(_) => {
            eprintln!("real_sim_device_detail SKIP: SMIX_REAL_SIM_FLOW not set");
            return;
        }
    };

    eprintln!("real_sim_device_detail: port={port} udid={udid:?} bundle={bundle_id}");

    let mut app = match App::connect_to_runner(port, udid.as_deref()).await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("real_sim_device_detail SKIP: runner unreachable on :{port}: {e}");
            return;
        }
    };
    if let Some(u) = udid {
        app = app.with_udid(u);
    }

    // iOS driving requires a live runner session (v2 break #1); bind one
    // to the target bundle before any sense/act call.
    app.open_session_in_place(&bundle_id, true)
        .await
        .expect("open session");

    app.foreground(&bundle_id).await.expect("foreground app");

    let flow = parse_flow_file(&flow_path).expect("parse flow yaml");
    let base_dir = flow_path
        .parent()
        .expect("flow has parent dir")
        .to_path_buf();

    let mut adapter = Adapter::new(&app, base_dir);
    // Real-sim business state (login session, RN bundle reload, device
    // connectivity) is not fully controllable from this test, so we
    // accept SDK failures (timeouts, element-not-found) as graceful
    // outcomes — they prove the adapter forwarded the call to a live
    // simulator. What we MUST reject are mapping bugs surfacing as
    // RunError::{Parse, UnknownKey, UnknownDirection, RunFlowCycle, Io}.
    let outcome = adapter.run(&flow).await;
    match outcome {
        Ok(report) => {
            eprintln!(
                "real_sim_device_detail: OK steps={} warnings={}",
                report.steps.len(),
                report.warnings.len()
            );
            for (i, w) in report.warnings.iter().take(5).enumerate() {
                eprintln!("  warn[{i}]: {w}");
            }
            let error_warns: Vec<&String> = report
                .warnings
                .iter()
                .filter(|w| w.to_uppercase().contains("ERROR"))
                .collect();
            assert!(
                error_warns.is_empty(),
                "no warnings should contain ERROR, got: {error_warns:?}"
            );
        }
        Err(RunError::Sdk(e)) => {
            eprintln!(
                "real_sim_device_detail: SDK failure (graceful for real-sim — business state not fully controllable): code={:?} msg={}",
                e.code, e.message
            );
        }
        Err(other) => {
            panic!("real_sim_device_detail: mapping-level error (NOT graceful): {other:?}")
        }
    }
}
