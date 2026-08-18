//! A record written while another smix holds the store lock.
//!
//! The persist path took the lock with `try_open` and treated "busy" as
//! "skip, the next attempt will persist". `smix run` is a one-shot
//! process: there is no next attempt, so a busy neighbour meant the
//! record was never written — and the gate that reads it back could not
//! tell that from a flow which never ran.
//!
//! One `#[test]` in this file on purpose, same reason as its sibling:
//! the persist path is a process-wide static.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::{Duration, Instant};

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

/// The union of both shapes — see the sibling file for why the gate
/// asks "did anything get lost" rather than "where is it kept".
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

/// The neighbour: takes the store lock, says so, holds it briefly, then
/// leaves. It releases unconditionally, so the process under test never
/// waits on anything that might not come.
#[test]
#[ignore = "helper: the lock holder, spawned by the test in this file"]
fn neighbour_holds_the_store_lock() {
    let root = std::env::var("SMIX_TEST_ROOT").expect("SMIX_TEST_ROOT");
    let held = std::env::var("SMIX_TEST_HELD").expect("SMIX_TEST_HELD");
    let _store = smix_store::Store::open(Path::new(&root)).expect("open store");
    std::fs::write(&held, b"held").expect("announce the lock is held");
    std::thread::sleep(Duration::from_millis(1200));
}

#[test]
fn a_record_written_under_a_held_lock_is_waited_for_not_skipped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("store");
    let held = dir.path().join("held");

    set_flow_attempts_persist_path(root.clone());
    // Load first, deliberately. Without this the process under test
    // blocks inside its own read while the neighbour holds the lock,
    // writes only afterwards, and the gate goes green having measured
    // "can wait" rather than "is not skipped".
    let _ = recent_flow_attempts();

    let exe = std::env::current_exe().expect("current exe");
    let mut child = std::process::Command::new(exe)
        .args([
            "neighbour_holds_the_store_lock",
            "--exact",
            "--ignored",
            "--nocapture",
        ])
        .env("SMIX_TEST_ROOT", &root)
        .env("SMIX_TEST_HELD", &held)
        .spawn()
        .expect("spawn the lock holder");

    let deadline = Instant::now() + Duration::from_secs(10);
    while !held.exists() {
        assert!(
            Instant::now() < deadline,
            "the neighbour never took the store lock — the gate measured nothing"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    record_flow_attempts("under-a-held-lock", &[Attempt]);

    child.wait().expect("the lock holder exits");

    let names = on_disk_flow_names(&root);
    assert!(
        names.contains("under-a-held-lock"),
        "the neighbour held the store lock, so this record was skipped instead of waited for: {names:?}"
    );
}
