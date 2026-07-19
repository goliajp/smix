//! The store must actually reach disk.
//!
//! smix's persistence was hand-written JSON with unchecked writes
//! (`let _ = std::fs::write(...)`), so "it worked" meant "the process
//! that wrote it could still see it". The test that matters is the one
//! a second process would run: close the store, open the same directory
//! again, and find the value.

use smix_store::Store;

fn temp_root(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("smix-store-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp root");
    dir
}

#[test]
fn a_value_written_is_a_value_read() {
    let root = temp_root("roundtrip");
    let store = Store::open(&root).expect("opens");
    store
        .sims()
        .put("5D08", b"{\"alias\":\"dev\"}")
        .expect("put");
    assert_eq!(
        store.sims().get("5D08").expect("get").as_deref(),
        Some(&b"{\"alias\":\"dev\"}"[..])
    );
}

#[test]
fn a_value_survives_reopening_the_store() {
    let root = temp_root("reopen");
    {
        let store = Store::open(&root).expect("opens");
        store.sims().put("5D08", b"booted").expect("put");
        store.sync().expect("sync");
    }
    let store = Store::open(&root).expect("reopens");
    assert_eq!(
        store.sims().get("5D08").expect("get").as_deref(),
        Some(&b"booted"[..]),
        "the value did not reach disk — a second process would not see it"
    );
}

#[test]
fn a_missing_key_is_none_not_an_error() {
    let root = temp_root("missing");
    let store = Store::open(&root).expect("opens");
    assert!(store.sims().get("nope").expect("get succeeds").is_none());
}

#[test]
fn opening_an_unwritable_root_names_the_path() {
    // A regular file where a directory has to go: portable across
    // platforms, and unlike an absolute `/proc/...` literal it does not
    // read as an HTTP route to the source gates.
    let blocker = temp_root("unwritable").join("not-a-dir");
    std::fs::write(&blocker, b"i am a file").expect("make the blocker");
    let err = Store::open(&blocker).expect_err("cannot persist under a file");
    let msg = format!("{err}");
    assert!(
        msg.contains("not-a-dir"),
        "the error must name the path it failed on: {msg}"
    );
}

/// kevy's `flush` is FLUSHALL — it empties the store. smix must never
/// reach it, and `sync` (the method that sounds like the safe one) must
/// be the durability call rather than the destructive one.
#[test]
fn sync_makes_durable_and_does_not_erase() {
    let root = temp_root("sync-keeps");
    let store = Store::open(&root).expect("opens");
    store.sims().put("a", b"1").expect("put");
    store.sims().put("b", b"2").expect("put");
    store.sync().expect("sync");
    assert_eq!(
        store.sims().list().len(),
        2,
        "sync must not empty the store"
    );
}
