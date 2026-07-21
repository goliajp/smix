//! The diagnostic singletons load on first use, not at startup.
//!
//! Every smix command used to set all three persist paths and read all
//! three values immediately. A backtrace probe on `Store::open` showed
//! `smix sim list` opening the store four times: three eager loads plus
//! the one write it needed. Two of the three were for state that
//! command cannot touch — it runs no flow and resets no app data — and
//! each open replays the AOF and takes a blocking advisory lock, so the
//! waste was also three extra chances to queue behind another smix.
//!
//! What has to keep working: a value written by an earlier process is
//! still visible to this one, just read later. And the deferral must
//! not become a cancellation — the flag may only latch once a path
//! exists, or a read that happens before the path is installed would
//! skip the load permanently rather than postponing it.

use std::path::Path;

use smix_simctl::{
    FlowAttemptShape, recent_flow_attempts, record_flow_attempts, set_flow_attempts_persist_path,
};

/// The caller supplies its own shape — `smix run` does the same with a
/// local newtype rather than a shared struct.
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

fn seed_flow_attempts(root: &Path, flow_name: &str) {
    let store = smix_store::Store::open(root).expect("open store");
    let value = serde_json::json!([{ "flow_name": flow_name, "attempts": [] }]);
    store
        .singleton("flow-attempts")
        .put_json(&value)
        .expect("write singleton");
}

/// A read that lands before the path is installed must postpone the
/// load, not cancel it. Latching the flag on that first read would mean
/// the value is never read at all.
///
/// Ordered deliberately: the read comes first, and only then the path.
/// One process, process-wide statics, so this is the whole test — a
/// second one touching the same singleton could not control the order.
#[test]
fn a_read_before_the_path_is_set_postpones_rather_than_cancels() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    seed_flow_attempts(&root, "written-by-an-earlier-process");

    // No path yet: nothing to read from, and nothing may be latched.
    let before = recent_flow_attempts();
    assert!(
        before.is_empty(),
        "no persist path is installed, so there is nothing to have loaded: {before:?}"
    );

    set_flow_attempts_persist_path(root.clone());

    // If the earlier read had latched the flag, this would still be
    // empty — the load would have been skipped forever rather than
    // deferred to here.
    let after = recent_flow_attempts();
    assert!(
        after
            .iter()
            .any(|f| f.flow_name == "written-by-an-earlier-process"),
        "the deferred load must happen on the first use after the path is set: {after:?}"
    );

    // A write still lands on top of what was read, rather than
    // replacing it — the earlier process's record survives.
    record_flow_attempts("added-now", &[Attempt]);
    let written = recent_flow_attempts();
    assert!(
        written
            .iter()
            .any(|f| f.flow_name == "written-by-an-earlier-process"),
        "the earlier flow must survive the write: {written:?}"
    );
    assert!(
        written.iter().any(|f| f.flow_name == "added-now"),
        "the new flow must be recorded: {written:?}"
    );
}
