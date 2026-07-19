//! The key schema is the on-disk contract.
//!
//! A user who upgrades smix keeps the store they already have. Renaming
//! a prefix orphans every record behind it — the registry looks empty,
//! the device list comes back blank, and nothing errors. So the exact
//! key shapes are asserted here: changing one has to come through this
//! file, deliberately.

use smix_store::{Store, StoreError};

fn temp_root(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("smix-store-schema-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp root");
    dir
}

#[test]
fn each_namespace_writes_its_documented_prefix() {
    let root = temp_root("prefixes");
    let store = Store::open(&root).expect("opens");
    store.sims().put("UDID-1", b"{}").expect("put");
    store.runners().put("UDID-1", b"{}").expect("put");
    store.attempts().put("flow-abc", b"{}").expect("put");

    let mut keys = store.raw_keys();
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "attempt:flow-abc".to_string(),
            "runner:UDID-1".to_string(),
            "sim:UDID-1".to_string(),
        ],
        "the on-disk key shapes changed — every existing record behind \
         the old prefix becomes unreachable, silently"
    );
}

#[test]
fn namespaces_do_not_see_each_other() {
    // sim:UDID-1 and runner:UDID-1 share an id. Reading one must never
    // return the other.
    let root = temp_root("isolation");
    let store = Store::open(&root).expect("opens");
    store.sims().put("shared", b"i-am-a-sim").expect("put");
    store
        .runners()
        .put("shared", b"i-am-a-runner")
        .expect("put");

    assert_eq!(
        store.sims().get("shared").expect("get").as_deref(),
        Some(&b"i-am-a-sim"[..])
    );
    assert_eq!(store.sims().list(), vec!["shared".to_string()]);
    assert_eq!(store.runners().list(), vec!["shared".to_string()]);
}

#[test]
fn list_returns_ids_without_the_prefix() {
    let root = temp_root("list");
    let store = Store::open(&root).expect("opens");
    for id in ["a", "b", "c"] {
        store.sims().put(id, b"{}").expect("put");
    }
    let mut ids = store.sims().list();
    ids.sort();
    assert_eq!(ids, vec!["a", "b", "c"]);
}

#[test]
fn delete_removes_only_its_own_record() {
    let root = temp_root("delete");
    let store = Store::open(&root).expect("opens");
    store.sims().put("keep", b"1").expect("put");
    store.sims().put("drop", b"2").expect("put");
    store.sims().delete("drop").expect("delete");
    assert_eq!(store.sims().list(), vec!["keep".to_string()]);
    store.sims().delete("never-existed").expect("idempotent");
}

#[test]
fn a_typed_read_of_a_corrupt_value_says_corrupt_not_missing() {
    // The difference that matters: a key that was never written is
    // None, and a key holding bytes that are not the expected shape is
    // an error naming the key. Collapsing the second into the first is
    // how corruption reads as "nothing here" and gets overwritten.
    let root = temp_root("corrupt");
    let store = Store::open(&root).expect("opens");
    store.sims().put("bad", b"{not json").expect("put");

    let missing: Option<serde_json::Value> = store
        .sims()
        .get_json("never-written")
        .expect("absent is not an error");
    assert!(missing.is_none());

    let err = store
        .sims()
        .get_json::<serde_json::Value>("bad")
        .expect_err("unparseable bytes must not read as absent");
    assert!(
        matches!(&err, StoreError::Corrupt { key, .. } if key.contains("bad")),
        "the error must name the key: {err}"
    );
}

#[test]
fn a_typed_round_trip_keeps_the_value() {
    let root = temp_root("typed");
    let store = Store::open(&root).expect("opens");
    let value = serde_json::json!({ "alias": "dev", "udid": "5D08" });
    store.sims().put_json("5D08", &value).expect("put");
    let back: serde_json::Value = store
        .sims()
        .get_json("5D08")
        .expect("get")
        .expect("present");
    assert_eq!(back, value);
}
