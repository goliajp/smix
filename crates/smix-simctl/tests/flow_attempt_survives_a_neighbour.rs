//! Two smix processes on one machine, each recording its own flow.
//!
//! The record a corpus gate reads to decide whether its instrument is
//! working lived in one machine-global blob, rewritten whole. A process
//! that had read the blob before its neighbour wrote would put its own
//! snapshot back on top, and the neighbour's flow was simply gone — not
//! corrupted, not reported, gone. That is how a flow which ran green
//! came back as `left no attempt record` mid-ship.
//!
//! One `#[test]` in this file on purpose: the persist path is a
//! process-wide static, so a second case in the same binary would be
//! reading the first one's root.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use smix_simctl::{
    FlowAttemptShape, recent_flow_attempts, record_flow_attempts, set_flow_attempts_persist_path,
};

struct Attempt;

impl FlowAttemptShape for Attempt {
    fn attempt_index(&self) -> u32 {
        0
    }
    fn status(&self) -> &str {
        "ok"
    }
    fn error_class(&self) -> Option<&str> {
        None
    }
    fn ips_generated(&self) -> Option<&str> {
        None
    }
    fn wall_ms(&self) -> u64 {
        1
    }
}

/// Which flows are on disk, whichever shape holds them.
///
/// The union of the old whole-blob singleton and the per-flow records,
/// because the question this gate asks is "did anything get lost", not
/// "where is it kept" — so it reads the same before and after the
/// change, and its failure is a judgment rather than a compile error.
fn on_disk_flow_names(root: &Path) -> BTreeSet<String> {
    let store = smix_store::Store::open(root).expect("open store");
    let mut names: BTreeSet<String> = store.attempts().list().into_iter().collect();
    if let Ok(Some(value)) = store
        .singleton("flow-attempts")
        .get_json::<serde_json::Value>()
        && let Some(records) = value.as_array()
    {
        for record in records {
            if let Some(name) = record.get("flow_name").and_then(serde_json::Value::as_str) {
                names.insert(name.to_string());
            }
        }
    }
    names
}

/// The neighbour: a separate process recording one flow into the same
/// store, then leaving. Reached by re-executing this test binary rather
/// than by adding a bin target — the product surface does not grow a
/// hole for a test to reach through.
#[test]
#[ignore = "helper: the neighbour process, spawned by the test in this file"]
fn neighbour_records_its_own_flow() {
    let root = std::env::var("SMIX_TEST_ROOT").expect("SMIX_TEST_ROOT");
    let flow = std::env::var("SMIX_TEST_FLOW").expect("SMIX_TEST_FLOW");
    set_flow_attempts_persist_path(PathBuf::from(root));
    record_flow_attempts(&flow, &[Attempt]);
}

#[test]
fn a_neighbours_record_survives_this_process_recording_its_own() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();

    set_flow_attempts_persist_path(root.clone());
    // The real window: `smix run` has already read this state once
    // before its own flow lands.
    let _ = recent_flow_attempts();

    let exe = std::env::current_exe().expect("current exe");
    let status = std::process::Command::new(exe)
        .args([
            "neighbour_records_its_own_flow",
            "--exact",
            "--ignored",
            "--nocapture",
        ])
        .env("SMIX_TEST_ROOT", &root)
        .env("SMIX_TEST_FLOW", "neighbour-flow")
        .status()
        .expect("spawn the neighbour");
    assert!(
        status.success(),
        "the neighbour process must succeed, or this gate measured nothing"
    );

    // Absence needs presence: without this, the assertion below would
    // also hold when the neighbour never wrote at all.
    let before = on_disk_flow_names(&root);
    assert!(
        before.contains("neighbour-flow"),
        "the neighbour must be on disk before this gate can say anything about losing it: {before:?}"
    );

    record_flow_attempts("mine-flow", &[Attempt]);

    let after = on_disk_flow_names(&root);
    assert!(
        after.contains("neighbour-flow"),
        "neighbour-flow was on disk before this process recorded its own flow and is gone after: {after:?}"
    );
    assert!(
        after.contains("mine-flow"),
        "this process's own flow must be recorded: {after:?}"
    );
}
