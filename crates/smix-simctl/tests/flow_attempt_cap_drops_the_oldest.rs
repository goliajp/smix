//! The cap evicts the oldest record, not an arbitrary one.
//!
//! Records are kept bounded so a dump stays cheap to serialize. Which
//! one goes matters: the gate that reads these back asks about the flow
//! that just ran, so evicting anything other than the oldest can drop
//! the record the caller is about to look for.
//!
//! One `#[test]` in this file on purpose, same as its siblings: the
//! persist path is a process-wide static.

use std::path::PathBuf;
use std::time::Duration;

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

#[test]
fn the_cap_evicts_the_oldest_and_keeps_the_rest_in_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root: PathBuf = dir.path().to_path_buf();
    set_flow_attempts_persist_path(root);

    // One more than the cap, so exactly one has to go — and the sleep
    // keeps the timestamps strictly increasing, which is what "oldest"
    // is decided on.
    for i in 0..=32 {
        record_flow_attempts(&format!("flow-{i:02}"), &[Attempt]);
        std::thread::sleep(Duration::from_millis(2));
    }

    let kept = recent_flow_attempts();
    let names: Vec<String> = kept.iter().map(|f| f.flow_name.clone()).collect();

    assert_eq!(
        names.len(),
        32,
        "the cap must drop the oldest record, not keep everything: {names:?}"
    );
    assert!(
        !names.iter().any(|n| n == "flow-00"),
        "the cap must drop the oldest record, and flow-00 is it: {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "flow-32"),
        "the newest record must survive its own write: {names:?}"
    );

    let expected: Vec<String> = (1..=32).map(|i| format!("flow-{i:02}")).collect();
    assert_eq!(
        names, expected,
        "the survivors must come back oldest first: {names:?}"
    );
}
