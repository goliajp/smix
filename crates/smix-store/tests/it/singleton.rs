//! Not everything smix remembers is a keyed record.
//!
//! The subprocess ring is a capped list read and written whole; the
//! reset-app-data counters are one pair of numbers. Forcing either into
//! a `Namespace` would put a fake id in `list()` and invite a caller to
//! iterate something that has exactly one member forever.

use smix_store::Store;

fn temp_root(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("smix-store-single-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp root");
    dir
}

#[test]
fn a_singleton_round_trips() {
    let root = temp_root("roundtrip");
    let store = Store::open(&root).expect("opens");
    let ring = serde_json::json!([{ "argv": ["xcrun", "simctl"], "wall_ms": 12 }]);
    store
        .singleton("subprocess-ring")
        .put_json(&ring)
        .expect("put");
    let back: serde_json::Value = store
        .singleton("subprocess-ring")
        .get_json()
        .expect("get")
        .expect("present");
    assert_eq!(back, ring);
}

#[test]
fn an_unwritten_singleton_is_none() {
    let root = temp_root("absent");
    let store = Store::open(&root).expect("opens");
    let v: Option<serde_json::Value> = store.singleton("never").get_json().expect("no error");
    assert!(v.is_none());
}

#[test]
fn singletons_use_their_own_prefix() {
    // Same on-disk-contract treatment as the three namespaces: a
    // renamed prefix strands whatever a user already has.
    let root = temp_root("prefix");
    let store = Store::open(&root).expect("opens");
    store.singleton("counters").put_json(&1u32).expect("put");
    assert_eq!(store.raw_keys(), vec!["one:counters".to_string()]);
}

#[test]
fn a_singleton_is_not_visible_as_a_namespace_record() {
    let root = temp_root("isolation");
    let store = Store::open(&root).expect("opens");
    store.singleton("counters").put_json(&1u32).expect("put");
    assert!(
        store.sims().list().is_empty(),
        "a singleton must not surface as a record in any namespace"
    );
}

#[test]
fn a_corrupt_singleton_says_corrupt() {
    let root = temp_root("corrupt");
    let store = Store::open(&root).expect("opens");
    store.singleton("ring").put_json(&"a string").expect("put");
    let err = store
        .singleton("ring")
        .get_json::<Vec<u32>>()
        .expect_err("a string is not a list");
    assert!(
        format!("{err}").contains("ring"),
        "the error must name the singleton: {err}"
    );
}

#[test]
fn deleting_a_singleton_leaves_no_key() {
    let root = temp_root("delete");
    let store = Store::open(&root).expect("opens");
    store.singleton("ring").put_json(&1u32).expect("put");
    store.singleton("ring").delete().expect("delete");
    assert!(store.raw_keys().is_empty());
}

#[test]
fn a_set_holds_membership_not_records() {
    let root = temp_root("set");
    let store = Store::open(&root).expect("opens");
    let capturing = store.set("capturing");
    capturing.add("UDID-A").expect("add");
    capturing.add("UDID-B").expect("add");
    capturing.add("UDID-A").expect("adding twice is once");

    let mut members = capturing.members().expect("members");
    members.sort();
    assert_eq!(members, vec!["UDID-A".to_string(), "UDID-B".to_string()]);

    capturing.remove("UDID-A").expect("remove");
    capturing
        .remove("never-there")
        .expect("removing an absent member succeeds");
    assert_eq!(
        capturing.members().expect("members"),
        vec!["UDID-B".to_string()]
    );
}

#[test]
fn an_empty_set_is_empty_not_an_error() {
    let root = temp_root("set-empty");
    let store = Store::open(&root).expect("opens");
    assert!(
        store
            .set("capturing")
            .members()
            .expect("no error")
            .is_empty()
    );
}

#[test]
fn a_set_survives_reopening() {
    let root = temp_root("set-persist");
    {
        let store = Store::open(&root).expect("opens");
        store.set("capturing").add("UDID-A").expect("add");
        store.sync().expect("sync");
    }
    let store = Store::open(&root).expect("reopens");
    assert_eq!(
        store.set("capturing").members().expect("members"),
        vec!["UDID-A".to_string()]
    );
}
