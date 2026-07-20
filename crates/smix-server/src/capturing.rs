//! Which simulators are capturing right now.
//!
//! This was a valkey set — one external process, held up for four
//! commands: PING, SADD, SREM, SMEMBERS. It is a set in the embedded
//! store now, and smix-server has one less thing that must already be
//! running before it can start.
//!
//! The key keeps its name. A server upgraded in place finds the
//! membership it had; renaming it would quietly show an empty live view
//! while captures were in fact running.

use smix_store::Store;

/// The set's name, unchanged from the valkey era.
pub const CAPTURING_SET: &str = "smix:capturing";

/// Mark a simulator as capturing.
pub fn add(store: &Store, udid: &str) -> Result<(), smix_store::StoreError> {
    store.set(CAPTURING_SET).add(udid)?;
    store.sync()
}

/// Mark a simulator as no longer capturing.
pub fn remove(store: &Store, udid: &str) -> Result<(), smix_store::StoreError> {
    store.set(CAPTURING_SET).remove(udid)?;
    store.sync()
}

/// Every capturing simulator. Unordered.
pub fn members(store: &Store) -> Result<Vec<String>, smix_store::StoreError> {
    store.set(CAPTURING_SET).members()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(name: &str) -> (Store, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("smix-capturing-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp root");
        let store = Store::open(&dir).expect("opens");
        (store, dir)
    }

    #[test]
    fn a_capturing_sim_shows_up_and_goes_away() {
        let (store, _dir) = temp_store("basic");
        add(&store, "UDID-A").expect("add");
        add(&store, "UDID-B").expect("add");
        let mut m = members(&store).expect("members");
        m.sort();
        assert_eq!(m, vec!["UDID-A".to_string(), "UDID-B".to_string()]);

        remove(&store, "UDID-A").expect("remove");
        assert_eq!(
            members(&store).expect("members"),
            vec!["UDID-B".to_string()]
        );
    }

    #[test]
    fn adding_the_same_sim_twice_leaves_one() {
        let (store, _dir) = temp_store("dedup");
        add(&store, "UDID-A").expect("add");
        add(&store, "UDID-A").expect("add again");
        assert_eq!(members(&store).expect("members").len(), 1);
    }

    #[test]
    fn nothing_capturing_is_an_empty_list_not_an_error() {
        let (store, _dir) = temp_store("empty");
        assert!(members(&store).expect("no error").is_empty());
    }

    #[test]
    fn membership_survives_a_restart() {
        let dir = std::env::temp_dir().join("smix-capturing-restart");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp root");
        {
            let store = Store::open(&dir).expect("opens");
            add(&store, "UDID-A").expect("add");
        }
        let store = Store::open(&dir).expect("reopens");
        assert_eq!(
            members(&store).expect("members"),
            vec!["UDID-A".to_string()]
        );
    }
}
