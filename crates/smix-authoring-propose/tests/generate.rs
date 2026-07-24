//! Generation core contract: the bundle → local-claude → Proposal path,
//! exercised device-free against a stub `claude` binary (the same stub
//! pattern `smix-ai-tier` uses). A canned stub reply becomes an `Ok(Proposal)`;
//! a failing CLI surfaces as `Err`, never a silent empty proposal.

use std::os::unix::fs::PermissionsExt;

use smix_ai_tier::AiTierConfig;
use smix_authoring_propose::{ProposalEdit, parse_proposal_reply, propose_from_bundle};
use smix_error::FailureCode;

fn rt() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("test runtime")
    })
}

fn stub_cli(dir: &std::path::Path, body: &str) -> AiTierConfig {
    let bin = dir.join("claude-stub");
    std::fs::write(&bin, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    AiTierConfig {
        claude_bin: bin.to_string_lossy().into_owned(),
        timeout_secs: 120,
    }
}

const CANNED_PROPOSAL: &str =
    r#"{"edits":[{"op":"replaceSelector","step_index":0,"new_selector":{"id":"submit-btn"}}]}"#;

/// Write a minimal but real bundle directory: a `run-summary.json`
/// (StepDebugRecord-shaped steps), a `failure.json` (ExpectationFailure
/// shape), and the original flow file. The stub ignores them; they exist so
/// the fixture is a real on-disk bundle rather than an invented wire.
fn write_bundle(dir: &std::path::Path) -> std::path::PathBuf {
    let flow = dir.join("flow.yaml");
    std::fs::write(&flow, "appId: com.example.app\n---\n- tapOn:\n    id: submit-btn\n").unwrap();
    std::fs::write(
        dir.join("run-summary.json"),
        r#"{"runOutcome":"failure","steps":[{"n":1,"verb":"tapon","summary":"tap submit-btn","verdict":"failed","wall_ms":1200,"json_path":"step-1-tapon.json","tree_path":"step-1-tapon.fail.tree.json","failure_kind":"ELEMENT_NOT_FOUND","failure_message":"no element matched"}]}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("failure.json"),
        r#"{"ok":false,"code":"ELEMENT_NOT_FOUND","message":"no element matched id submit-btn","selector":{"id":"submit-btn"},"suggestions":["id: submit"],"visibleElements":[],"smixVersion":"2.0.0"}"#,
    )
    .unwrap();
    flow
}

#[test]
fn parse_proposal_reply_tolerates_prose() {
    let reply = format!("Sure! Here is the fix: {CANNED_PROPOSAL} — hope that helps!");
    let proposal = parse_proposal_reply(&reply).expect("prose-wrapped proposal parses");
    assert_eq!(proposal.edits.len(), 1);
    assert!(matches!(
        &proposal.edits[0],
        ProposalEdit::ReplaceSelector { step_index: 0, .. }
    ));
}

#[test]
fn propose_from_bundle_parses_stub_reply() {
    rt().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let flow = write_bundle(dir.path());
        let cfg = stub_cli(dir.path(), &format!("printf '%s' '{CANNED_PROPOSAL}'"));
        let proposal = propose_from_bundle(&flow, dir.path(), &cfg)
            .await
            .expect("stub reply yields a proposal");
        assert_eq!(proposal.edits.len(), 1);
        assert!(matches!(
            &proposal.edits[0],
            ProposalEdit::ReplaceSelector { step_index: 0, .. }
        ));
    });
}

#[test]
fn propose_from_bundle_surfaces_cli_failure() {
    rt().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let flow = write_bundle(dir.path());
        let cfg = stub_cli(dir.path(), "echo 'not logged in' >&2\nexit 1");
        let err = propose_from_bundle(&flow, dir.path(), &cfg)
            .await
            .unwrap_err();
        assert_eq!(err.code, FailureCode::DriverError);
    });
}
