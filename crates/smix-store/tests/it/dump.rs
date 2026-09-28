//! Moving to a binary store must not cost the ability to look inside.
//!
//! `.smix/sims.json` could be `cat`-ed, and that mattered — this
//! session's own debugging leaned on reading `.smix/runner/state.json`
//! straight off disk. A KV file cannot be read that way, so the store
//! owes the same view through a command instead. Anything less is a
//! capability traded away quietly.

use smix_store::Store;

fn temp_root(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("smix-store-dump-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp root");
    dir
}

#[test]
fn dump_shows_every_namespace() {
    let root = temp_root("all");
    let store = Store::open(&root).expect("opens");
    store
        .sims()
        .put_json("5D08", &serde_json::json!({ "alias": "dev" }))
        .expect("put");
    store
        .runners()
        .put_json("5D08", &serde_json::json!({ "port": 22087 }))
        .expect("put");

    let dumped: serde_json::Value =
        serde_json::from_str(&store.dump_json().expect("dumps")).expect("dump is valid JSON");

    assert_eq!(dumped["sim:5D08"]["alias"], "dev");
    assert_eq!(dumped["runner:5D08"]["port"], 22087);
}

#[test]
fn an_empty_store_dumps_valid_json() {
    let root = temp_root("empty");
    let store = Store::open(&root).expect("opens");
    let dumped: serde_json::Value =
        serde_json::from_str(&store.dump_json().expect("dumps")).expect("valid JSON");
    assert!(dumped.as_object().expect("object").is_empty());
}

#[test]
fn one_unreadable_value_does_not_sink_the_whole_dump() {
    // Dump is what you reach for WHEN something is wrong, so a single
    // corrupt value must not be the thing that stops you seeing the
    // rest. The bad one is shown as bytes, and still shown.
    let root = temp_root("corrupt");
    let store = Store::open(&root).expect("opens");
    store.sims().put("good", b"{\"ok\":true}").expect("put");
    store.sims().put("bad", &[0xff, 0xfe, 0x00]).expect("put");

    let dumped: serde_json::Value =
        serde_json::from_str(&store.dump_json().expect("dumps anyway")).expect("valid JSON");
    assert_eq!(dumped["sim:good"]["ok"], true);
    assert!(
        dumped["sim:bad"].is_string(),
        "an unparseable value must still appear, as bytes: {dumped}"
    );
}
