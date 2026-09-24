//! Opening the store prints nothing unless something is wrong with it.
//!
//! The embedded store announces every replay of its append-only log on
//! stderr: `kevy: AOF … replayed N commands from M bytes in K ms (clean)`.
//! For a server that is a boot banner, once. smix opens the store on every
//! command, so every command printed it — into terminals, CI logs and AI
//! session transcripts. The repository's own scripts grew forty-one
//! `grep -v '^kevy:'` filters to cope, and the status read after those
//! filters was the filter's rather than smix's.
//!
//! A replay that lost bytes is different: that is the store telling you it
//! recovered less than the file held, and it must still be said. kevy says
//! it with a WARN that the quiet switch never silences; the second case
//! pins that, so a kevy upgrade that started silencing it would go red here
//! rather than in somebody's lost data.
//!
//! The line goes to the process's own stderr, so each case opens the store
//! in a child — this test binary re-run with one test selected and an
//! environment variable saying what the child is to do.

use std::path::Path;
use std::process::Command;

const CHILD: &str = "SMIX_STORE_OPENING_CHILD";

/// In the child: open the store at the given root and write one key, so
/// the next open has a log to replay. In the parent: nothing.
fn child_opens(test: &str) -> bool {
    let Ok(dir) = std::env::var(CHILD) else {
        return false;
    };
    let s = smix_store::Store::open(Path::new(&dir)).expect("open in child");
    s.sims()
        .put(&format!("probe-{test}"), b"x")
        .expect("put in child");
    s.sync().expect("sync in child");
    true
}

fn open_in_child(test: &str, root: &Path) -> String {
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(CHILD, root)
        .output()
        .expect("re-run this test binary as the child");
    assert!(
        out.status.success(),
        "the child open failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn a_store_with_history_opens_without_a_word() {
    if child_opens("a_store_with_history_opens_without_a_word") {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    open_in_child("a_store_with_history_opens_without_a_word", root.path());
    let second = open_in_child("a_store_with_history_opens_without_a_word", root.path());
    let chatter: Vec<&str> = second.lines().filter(|l| l.starts_with("kevy:")).collect();
    assert!(
        chatter.is_empty(),
        "opening a store with a clean log printed the replay banner: {chatter:?}"
    );
}

#[test]
fn a_replay_that_lost_bytes_is_still_said() {
    if child_opens("a_replay_that_lost_bytes_is_still_said") {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    open_in_child("a_replay_that_lost_bytes_is_still_said", root.path());
    // A partial last write: the frame header promises more than follows.
    let aof = std::fs::read_dir(root.path().join("kv"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "aof"))
        .expect("the first open left an append-only log");
    let mut bytes = std::fs::read(&aof).unwrap();
    bytes.extend_from_slice(b"*3\r\n$3\r\nSET\r\n$9\r\nincompl");
    std::fs::write(&aof, bytes).unwrap();

    let second = open_in_child("a_replay_that_lost_bytes_is_still_said", root.path());
    assert!(
        second
            .lines()
            .any(|l| l.starts_with("kevy WARN:") && l.contains("dropped")),
        "a replay that dropped a partial frame said nothing:\n{second}"
    );
}
