//! Bringing a user's existing JSON state into the store.
//!
//! Every machine running smix today has state on disk in files this
//! crate replaces. The migration has to be invisible: the same devices
//! resolve, the same runner is found, nothing is announced.
//!
//! Three rules, each protecting against a different way to lose data.
//!
//! **The legacy file is never touched.** Not deleted, not rewritten,
//! not renamed. If this store turns out to have a problem, the previous
//! smix has to still find its registry — a migration that burns the
//! bridge behind it is one nobody can retreat across.
//!
//! **The store wins.** Import fills gaps only. Once someone has
//! registered a device on the new version, an older file must not
//! reach forward and undo it.
//!
//! **A file that will not parse is an error.** "Your registry is
//! corrupt" and "you have no devices" lead to different actions, and
//! only one of them is true. Skipping the file quietly turns the first
//! into the second, and the next write finishes the job.

use std::path::Path;

use crate::{Namespace, StoreError};

/// Copy records from a legacy JSON file into a namespace.
///
/// The file is expected to hold `{ "<container>": { "<id>": <value> } }`
/// — the shape every one of smix's registry files uses. Returns how
/// many records were newly written; already-present ids are left as
/// they are.
///
/// # Errors
///
/// [`StoreError::Corrupt`] when the file exists but cannot be read as
/// that shape, naming the path so the user knows what to look at. A
/// file that is simply absent is not an error and imports nothing.
pub fn import_legacy_records(
    into: &Namespace<'_>,
    legacy: &Path,
    container: &str,
) -> Result<usize, StoreError> {
    let bytes = match std::fs::read(legacy) {
        Ok(b) => b,
        // Nothing to migrate is the common case, on every run after the
        // first and on every fresh install.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(source) => {
            return Err(StoreError::Op {
                op: "import",
                key: legacy.display().to_string(),
                source,
            });
        }
    };

    let doc: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| StoreError::Corrupt {
            key: legacy.display().to_string(),
            expected: "a smix JSON state file",
            detail: e.to_string(),
        })?;

    let records = doc
        .get(container)
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| StoreError::Corrupt {
            key: legacy.display().to_string(),
            expected: "a JSON object",
            detail: format!("no `{container}` object at the top level"),
        })?;

    let mut written = 0usize;
    for (id, value) in records {
        // Read-then-write, not an upsert: the point is to not clobber.
        if into.get(id)?.is_some() {
            continue;
        }
        into.put_json(id, value)?;
        written += 1;
    }
    Ok(written)
}
