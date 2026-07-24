//! Amend-loop mechanism contract: the device-free half of C4. The full loop —
//! `parse_flow_yaml` → `propose_from_bundle` (stub claude) → `apply` →
//! `emit_flow_yaml` → back to `parse_flow_yaml` — must close, swapping a typo'd
//! selector back to the correct one, and a failing CLI must surface as an
//! `AmendError`, never a silent empty amend. No device, no real claude.

use std::os::unix::fs::PermissionsExt;

use smix_adapter_maestro::{Step, parse_flow_yaml};
use smix_authoring_propose::{AmendError, propose_and_amend};
use smix_selector::Selector;

fn rt() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("test runtime")
    })
}

fn stub_cli(dir: &std::path::Path, body: &str) -> smix_ai_tier::AiTierConfig {
    let bin = dir.join("claude-stub");
    std::fs::write(&bin, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    smix_ai_tier::AiTierConfig {
        claude_bin: bin.to_string_lossy().into_owned(),
        timeout_secs: 120,
    }
}

/// Write a typo'd flow plus a real on-disk bundle (`run-summary.json` +
/// `failure.json` carrying a `suggestions` naming the correct selector).
/// Returns the flow path.
fn write_typo_bundle(dir: &std::path::Path) -> std::path::PathBuf {
    let flow = dir.join("corrupt.yaml");
    std::fs::write(
        &flow,
        "appId: com.example.app\n---\n- launchApp:\n    clearState: true\n- assertVisible:\n    id: search_action_barX\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("run-summary.json"),
        r#"{"runOutcome":"failure","steps":[{"n":2,"verb":"assertvisible","summary":"assert search_action_barX","verdict":"failed","failure_kind":"ELEMENT_NOT_FOUND","failure_message":"no element matched"}]}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("failure.json"),
        r#"{"ok":false,"code":"ELEMENT_NOT_FOUND","message":"no element matched id search_action_barX","selector":{"id":"search_action_barX"},"suggestions":["Did you mean \"search_action_bar\"? (similarity 0.94, field name)"],"visibleCount":12,"smixVersion":"2.0.0"}"#,
    )
    .unwrap();
    flow
}

const CANNED_SWAP: &str = r#"{"edits":[{"op":"replaceSelector","step_index":1,"new_selector":{"id":"search_action_bar"}}]}"#;

#[test]
fn stub_loop_closes_and_swaps() {
    rt().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let flow = write_typo_bundle(dir.path());
        let cfg = stub_cli(dir.path(), &format!("printf '%s' '{CANNED_SWAP}'"));

        let yaml = propose_and_amend(&flow, dir.path(), &cfg)
            .await
            .expect("stub loop yields amended yaml");

        let flow2 = parse_flow_yaml(&yaml).expect("amended yaml parses back");
        match &flow2.steps[1] {
            Step::AssertVisible {
                selector: Selector::Id { id, .. },
            } => assert_eq!(id, "search_action_bar", "typo swapped back to the suggestion"),
            other => panic!("step 1 not a fixed assertVisible: {other:?}"),
        }
    });
}

#[test]
fn stub_cli_failure_surfaces_not_silent() {
    rt().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let flow = write_typo_bundle(dir.path());
        let cfg = stub_cli(dir.path(), "echo 'not logged in' >&2\nexit 1");

        let err = propose_and_amend(&flow, dir.path(), &cfg)
            .await
            .expect_err("a failing CLI must surface, not collapse to empty yaml");
        assert!(
            matches!(err, AmendError::Propose(_)),
            "driver error propagates through propose, got {err:?}"
        );
    });
}
