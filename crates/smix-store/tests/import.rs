//! Users already have a `.smix/sims.json`. It has to keep working.
//!
//! An upgrade that silently empties someone's device registry is worse
//! than one that refuses to start: they run a flow, it says "no such
//! device", and nothing points at the store that just moved.
//!
//! Three properties matter more than convenience here. The original
//! file is never deleted — if this migration turns out badly, going
//! back to the old smix has to still work. The import never overwrites
//! what is already in the store — the store is the new source of truth
//! and the old file only fills gaps. And a file that cannot be parsed
//! is an error, not a skip, because "your registry is corrupt" and
//! "you have no devices" are different sentences.

use smix_store::{Store, import_legacy_records};

const LEGACY_SIMS: &str = r#"{
  "version": 1,
  "sims": {
    "dev": {
      "deviceName": "iPhone 16 Pro",
      "udid": "5D087114-ECB3-443C-8DDB-40EEF9CFB90C",
      "runtime": "iOS 26.5",
      "deviceType": "iPhone 16 Pro",
      "runnerPort": 22087
    },
    "spare": {
      "deviceName": "iPhone SE",
      "udid": "AAAA1111-2222-3333-4444-555566667777",
      "runtime": "iOS 26.5",
      "deviceType": "iPhone SE"
    }
  }
}"#;

fn temp_root(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("smix-store-import-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp root");
    dir
}

fn write_legacy(root: &std::path::Path) -> std::path::PathBuf {
    let path = root.join("sims.json");
    std::fs::write(&path, LEGACY_SIMS).expect("write legacy");
    path
}

#[test]
fn every_legacy_record_reaches_the_store() {
    let root = temp_root("basic");
    let legacy = write_legacy(&root);
    let store = Store::open(&root).expect("opens");

    let imported = import_legacy_records(&store.sims(), &legacy, "sims").expect("imports");
    assert_eq!(imported, 2);

    let mut ids = store.sims().list();
    ids.sort();
    assert_eq!(ids, vec!["dev".to_string(), "spare".to_string()]);

    let dev: serde_json::Value = store.sims().get_json("dev").expect("get").expect("present");
    assert_eq!(dev["udid"], "5D087114-ECB3-443C-8DDB-40EEF9CFB90C");
    assert_eq!(dev["runnerPort"], 22087);
}

#[test]
fn the_original_file_is_left_alone() {
    let root = temp_root("keeps-file");
    let legacy = write_legacy(&root);
    let store = Store::open(&root).expect("opens");
    import_legacy_records(&store.sims(), &legacy, "sims").expect("imports");
    assert!(
        legacy.exists(),
        "the legacy file must survive — downgrading has to remain possible"
    );
    assert_eq!(
        std::fs::read_to_string(&legacy).expect("read"),
        LEGACY_SIMS,
        "the legacy file must not be rewritten either"
    );
}

#[test]
fn importing_twice_changes_nothing() {
    let root = temp_root("idempotent");
    let legacy = write_legacy(&root);
    let store = Store::open(&root).expect("opens");
    assert_eq!(
        import_legacy_records(&store.sims(), &legacy, "sims").expect("first"),
        2
    );
    assert_eq!(
        import_legacy_records(&store.sims(), &legacy, "sims").expect("second"),
        0,
        "a second import must import nothing"
    );
    assert_eq!(store.sims().list().len(), 2);
}

#[test]
fn the_store_wins_over_the_legacy_file() {
    // The user registered `dev` again on the new version. The old file
    // still holds the previous UDID. Filling gaps is the job; undoing
    // the user's newer work is not.
    let root = temp_root("no-clobber");
    let legacy = write_legacy(&root);
    let store = Store::open(&root).expect("opens");
    store
        .sims()
        .put_json("dev", &serde_json::json!({ "udid": "NEWER-UDID" }))
        .expect("put");

    let imported = import_legacy_records(&store.sims(), &legacy, "sims").expect("imports");
    assert_eq!(imported, 1, "only the record the store lacked");

    let dev: serde_json::Value = store.sims().get_json("dev").expect("get").expect("present");
    assert_eq!(
        dev["udid"], "NEWER-UDID",
        "the legacy file overwrote newer state"
    );
}

#[test]
fn a_missing_legacy_file_is_not_an_error() {
    let root = temp_root("absent");
    let store = Store::open(&root).expect("opens");
    let imported =
        import_legacy_records(&store.sims(), &root.join("nope.json"), "sims").expect("no file");
    assert_eq!(imported, 0);
}

#[test]
fn a_corrupt_legacy_file_is_an_error_not_a_skip() {
    let root = temp_root("corrupt");
    let legacy = root.join("sims.json");
    std::fs::write(&legacy, b"{ this is not json").expect("write");
    let store = Store::open(&root).expect("opens");

    let err = import_legacy_records(&store.sims(), &legacy, "sims")
        .expect_err("a corrupt registry must be reported, not treated as empty");
    let msg = format!("{err}");
    assert!(
        msg.contains("sims.json"),
        "the error must name the file the user has to look at: {msg}"
    );
}
