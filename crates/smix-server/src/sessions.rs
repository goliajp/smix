//! Which simulators have a live stream on disk.
//!
//! This was a postgres table — one table, one primary key, one upsert.
//! That is a key-value access pattern wearing a relational schema, and
//! it cost the server a database that had to be running, migrated and
//! reachable before it could answer a single request.
//!
//! What SQL did give for free was `ORDER BY started_at DESC`. A KV store
//! has no order, so the sort is explicit below. Losing it silently would
//! reorder the live view without any test noticing.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use smix_store::{Store, StoreError};

/// One simulator whose stream is being served.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimEntry {
    /// The device.
    pub udid: String,
    /// Its human-readable name.
    pub device_name: String,
    /// The runtime it boots.
    pub runtime: String,
    /// HLS directory, relative to `SMIX_STREAM_ROOT`.
    pub stream_path: String,
    /// When the most recent capture started.
    pub started_at: DateTime<Utc>,
    /// Whether this device is capturing *right now*.
    ///
    /// Answered from the capturing set at read time, never stored — so
    /// it is skipped when *writing* a record and present when the entry
    /// is serialized into a response. `#[serde(skip)]` does both
    /// directions and dropped it from the live view entirely, which the
    /// wiring suite caught the moment it could actually run.
    ///
    /// A stopped capture leaves its playlist watchable on disk, so the
    /// record outliving the capture is the point — this field is what
    /// separates "there is a stream" from "it is live".
    #[serde(skip_deserializing, default)]
    pub capturing: bool,
}

/// Record a simulator as having a live stream.
///
/// Keyed on udid, so re-capturing a device refreshes its record rather
/// than accumulating one per run — the same thing the table's
/// `ON CONFLICT (udid) DO UPDATE` did.
///
/// `started_at` now comes from this process rather than from postgres's
/// `now()`. Same meaning, different clock.
pub fn register(
    store: &Store,
    udid: &str,
    device_name: &str,
    runtime: &str,
    stream_path: &str,
) -> Result<(), StoreError> {
    let entry = SimEntry {
        udid: udid.to_string(),
        device_name: device_name.to_string(),
        runtime: runtime.to_string(),
        stream_path: stream_path.to_string(),
        started_at: Utc::now(),
        capturing: false,
    };
    store.sessions().put_json(udid, &entry)?;
    store.sync()
}

/// Every recorded stream, newest first.
pub fn list(store: &Store) -> Result<Vec<SimEntry>, StoreError> {
    let mut entries = Vec::new();
    for udid in store.sessions().list() {
        let Some(entry) = store.sessions().get_json::<SimEntry>(&udid)? else {
            // Listed and then gone: nothing else writes this namespace,
            // so this would mean a concurrent delete that does not
            // exist. Skipping is safe and keeps the view usable.
            continue;
        };
        entries.push(entry);
    }
    // Explicit: the store returns no order, and the live view has always
    // shown the most recent capture first.
    entries.sort_by_key(|e| std::cmp::Reverse(e.started_at));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(name: &str) -> (Store, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("smix-sessions-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp root");
        let store = Store::open(&dir).expect("opens");
        (store, dir)
    }

    #[test]
    fn a_registered_session_is_listed() {
        let (store, _d) = temp_store("basic");
        register(&store, "UDID-A", "iPhone 16", "iOS 26.5", "a/index.m3u8").expect("register");
        let all = list(&store).expect("list");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].udid, "UDID-A");
        assert_eq!(all[0].stream_path, "a/index.m3u8");
    }

    #[test]
    fn registering_the_same_device_twice_refreshes_one_record() {
        let (store, _d) = temp_store("upsert");
        register(&store, "UDID-A", "old name", "iOS 26.4", "old/path").expect("first");
        register(&store, "UDID-A", "new name", "iOS 26.5", "new/path").expect("second");
        let all = list(&store).expect("list");
        assert_eq!(all.len(), 1, "re-capturing accumulated a second record");
        assert_eq!(all[0].device_name, "new name");
        assert_eq!(all[0].runtime, "iOS 26.5");
        assert_eq!(all[0].stream_path, "new/path");
    }

    #[test]
    fn the_newest_capture_comes_first() {
        // The one thing SQL was doing for free. A KV store hands them
        // back in whatever order it likes.
        let (store, _d) = temp_store("order");
        register(&store, "OLD", "a", "iOS", "a").expect("first");
        std::thread::sleep(std::time::Duration::from_millis(5));
        register(&store, "NEW", "b", "iOS", "b").expect("second");
        let all = list(&store).expect("list");
        assert_eq!(
            all.iter().map(|e| e.udid.as_str()).collect::<Vec<_>>(),
            vec!["NEW", "OLD"]
        );
    }

    #[test]
    fn no_sessions_is_an_empty_list_not_an_error() {
        let (store, _d) = temp_store("empty");
        assert!(list(&store).expect("no error").is_empty());
    }

    #[test]
    fn a_corrupt_record_is_reported_not_skipped() {
        let (store, _d) = temp_store("corrupt");
        store.sessions().put("BAD", b"{not a session").expect("put");
        let err = list(&store).expect_err("a damaged record must not vanish from the view");
        assert!(
            format!("{err}").contains("BAD"),
            "must name the record: {err}"
        );
    }
}

#[cfg(test)]
mod capturing_field_tests {
    use super::*;

    /// `capturing` is computed, not stored — but it must still reach
    /// the client. `#[serde(skip)]` removed it from both directions and
    /// the live view lost the field that says whether a stream is live.
    #[test]
    fn capturing_is_serialized_but_never_persisted() {
        let entry = SimEntry {
            udid: "U".into(),
            device_name: "d".into(),
            runtime: "r".into(),
            stream_path: "p".into(),
            started_at: Utc::now(),
            capturing: true,
        };
        let json = serde_json::to_value(&entry).expect("serializes");
        assert_eq!(
            json["capturing"], true,
            "the live view lost the field that says a stream is live"
        );

        // Round-tripping a stored record must not carry it back in.
        let stored: SimEntry = serde_json::from_value(json).expect("deserializes");
        assert!(
            !stored.capturing,
            "capturing came back from storage instead of from the set"
        );
    }
}
