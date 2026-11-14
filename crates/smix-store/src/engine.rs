use std::path::Path;

use kevy_embedded::Store as KevyStore;

use crate::{StoreError, kevy_config};

/// Open the engine in `dir`, the store lock already held.
///
/// The engine claims its directory with a lock of its own, so holding
/// ours leaves one way for that claim to be taken: a child this process
/// is starting on another thread. It holds copies of both locks from
/// fork until exec closes them one at a time, and ours can come free
/// first. Waiting for the engine's lock here waits for that exec.
pub(crate) fn open_engine(dir: &Path) -> Result<KevyStore, StoreError> {
    let mut attempts = 0;
    loop {
        match KevyStore::open(kevy_config(dir)) {
            Err(kevy_embedded::KevyError::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock && attempts < 20 =>
            {
                attempts += 1;
                let claim = std::fs::File::open(dir.join("LOCK"))
                    .and_then(|f| f.lock().map(|()| f))
                    .map_err(|source| StoreError::Open {
                        path: dir.join("LOCK"),
                        source,
                    })?;
                drop(claim);
            }
            r => {
                return r.map_err(|source| StoreError::Open {
                    path: dir.to_path_buf(),
                    source: std::io::Error::other(source),
                });
            }
        }
    }
}
