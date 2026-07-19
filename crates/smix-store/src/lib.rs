//! The one place smix persists anything.
//!
//! Before this, persistence was three unrelated things: a valkey set
//! and a postgres table in `smix-server`, and hand-written JSON files
//! for everything the CLI remembers. The JSON half wrote through
//! `let _ = std::fs::write(...)` — a failed write was indistinguishable
//! from a successful one, and two `smix` processes could truncate each
//! other's registry.
//!
//! One store, one set of failure semantics, one way to look inside.
//!
//! # Which kevy
//!
//! Built against **crates.io `kevy-embedded` 3.18**, not the version in
//! a sibling checkout. The two differ — 4.0 has a `shutdown` that 3.18
//! does not — and code written against a local path dependency does not
//! compile for anyone else.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use kevy_embedded::{Config, Store as KevyStore};

mod import;
pub use import::import_legacy_records;

/// Key prefixes. One namespace per kind of thing smix remembers.
///
/// These strings are the on-disk contract: renaming one orphans every
/// record a user already has, so `schema.rs` asserts their exact shape
/// and a rename has to go through a test.
mod prefix {
    /// Registered simulators, keyed by UDID.
    pub const SIMS: &str = "sim:";
    /// Runner processes, keyed by device.
    pub const RUNNERS: &str = "runner:";
    /// Flow attempt records, keyed by flow identity.
    pub const ATTEMPTS: &str = "attempt:";
    /// Values that exist once: read whole, written whole.
    pub const SINGLETON: &str = "one:";
}

/// What can go wrong touching the store.
///
/// Every variant names the key or path involved. A bare `io::Error`
/// reaching a flow author says "something failed" and nothing else.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The store could not be opened at this path.
    #[error("smix store at {path} could not be opened: {source}")]
    Open {
        /// The directory smix tried to persist into.
        path: PathBuf,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// A read or write against a key failed.
    #[error("smix store: {op} on `{key}` failed: {source}")]
    Op {
        /// The operation attempted (`get`, `put`, `delete`, `list`).
        op: &'static str,
        /// The full key, prefix included.
        key: String,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// A stored value could not be understood.
    ///
    /// Deliberately not `None`: a value that exists but cannot be
    /// parsed is a different situation from a key that was never
    /// written, and collapsing the two is how corruption gets read as
    /// absence.
    #[error("smix store: the value at `{key}` is not valid {expected}: {detail}")]
    Corrupt {
        /// The key whose value could not be read.
        key: String,
        /// What the caller expected to find.
        expected: &'static str,
        /// Why it could not be read.
        detail: String,
    },
}

/// smix's persistent state.
///
/// # One process at a time
///
/// `kevy-embedded` is an in-process store: it assumes whoever opened
/// the directory owns it, and it takes no cross-process lock. smix
/// breaks that assumption routinely — a flow runs for minutes while
/// someone registers a simulator in another terminal — and two
/// instances appending to one AOF lose each other's writes. Measured,
/// not assumed: a concurrent-register test failed two runs in five
/// before this lock existed.
///
/// So opening the store takes an exclusive advisory lock on the root
/// and holds it until the `Store` is dropped. Callers open for an
/// operation and drop; a `Store` kept alive for the length of a flow
/// would block every other smix command on that directory.
///
/// `Debug` prints the root, never the contents: a store may hold a
/// user's device registry, and a panic message is not where that
/// belongs.
pub struct Store {
    inner: KevyStore,
    root: PathBuf,
    /// Held for the lifetime of the store. Dropping the file releases
    /// the lock, and the kernel does it too if this process dies —
    /// which a lockfile of our own would not.
    _lock: std::fs::File,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").field("root", &self.root).finish()
    }
}

impl Store {
    /// Open (or create) the store under `root`.
    ///
    /// `root` is the directory smix already owns — `.smix/` for
    /// project-local state, the user data dir for cross-project state.
    /// The store lives in `root/kv`.
    pub fn open(root: &Path) -> Result<Self, StoreError> {
        std::fs::create_dir_all(root).map_err(|source| StoreError::Open {
            path: root.to_path_buf(),
            source,
        })?;
        let lock_path = root.join("store.lock");
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|source| StoreError::Open {
                path: lock_path.clone(),
                source,
            })?;
        // Blocking, not try_lock: a second smix waits its turn rather
        // than failing, which is what a user expects from two commands
        // in two terminals. `File::lock` is std's own since 1.89 — an
        // added crate for this would have been a dependency for
        // something the standard library already does.
        lock.lock().map_err(|source| StoreError::Open {
            path: lock_path,
            source,
        })?;

        let dir = root.join("kv");
        let inner = KevyStore::open(Config::default().with_persist(&dir)).map_err(|source| {
            StoreError::Open {
                path: dir.clone(),
                source,
            }
        })?;
        Ok(Store {
            inner,
            root: root.to_path_buf(),
            _lock: lock,
        })
    }

    /// Where this store persists. Useful in errors and diagnostics.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Force the append-only log to disk.
    ///
    /// Writes already go to the AOF; this is for the moments smix wants
    /// them durable *now* — before spawning a runner that may outlive
    /// this process, or before exiting.
    ///
    /// **Not `flush`.** kevy's `Store::flush` is `FLUSHALL`: it empties
    /// the store. The name collides with `Write::flush`, which is what
    /// it looks like it should be, and the deprecation notice on it
    /// caught this method calling it while its own doc comment claimed
    /// to be pushing writes to disk. Nothing here calls `flush` or
    /// `flushall`; smix has no reason to erase a user's state.
    pub fn sync(&self) -> Result<(), StoreError> {
        self.inner.fsync_aof().map_err(|source| StoreError::Op {
            op: "sync",
            key: "<aof>".to_string(),
            source,
        })
    }

    /// Registered simulators.
    #[must_use]
    pub fn sims(&self) -> Namespace<'_> {
        Namespace {
            store: self,
            prefix: prefix::SIMS,
        }
    }

    /// Runner processes.
    #[must_use]
    pub fn runners(&self) -> Namespace<'_> {
        Namespace {
            store: self,
            prefix: prefix::RUNNERS,
        }
    }

    /// Flow attempt records.
    #[must_use]
    pub fn attempts(&self) -> Namespace<'_> {
        Namespace {
            store: self,
            prefix: prefix::ATTEMPTS,
        }
    }

    /// The whole store as JSON, for a human looking for the problem.
    ///
    /// Values that parse as JSON appear as JSON; values that do not
    /// appear as a hex string. A dump is what you run when something is
    /// already wrong, so one unreadable value must not be what stops
    /// you seeing the other forty.
    pub fn dump_json(&self) -> Result<String, StoreError> {
        let mut map = serde_json::Map::new();
        for key in self.raw_keys() {
            let raw = self
                .inner
                .get(key.as_bytes())
                .map_err(|source| StoreError::Op {
                    op: "dump",
                    key: key.clone(),
                    source,
                })?;
            let value = match raw {
                None => serde_json::Value::Null,
                Some(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| {
                    serde_json::Value::String(
                        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                    )
                }),
            };
            map.insert(key, value);
        }
        serde_json::to_string_pretty(&serde_json::Value::Object(map)).map_err(|e| {
            StoreError::Corrupt {
                key: "<dump>".to_string(),
                expected: "serializable dump",
                detail: e.to_string(),
            }
        })
    }

    /// A value there is exactly one of.
    ///
    /// The subprocess ring is a capped list read and written in full;
    /// the reset-app-data counters are a single pair. Neither is a
    /// record among others, and putting them in a [`Namespace`] would
    /// leave a fake id in `list()` for a caller to iterate.
    ///
    /// Trimming a capped list stays with the caller — the store has no
    /// business knowing whose list is capped at what.
    #[must_use]
    pub fn singleton(&self, name: &'static str) -> Singleton<'_> {
        Singleton { store: self, name }
    }

    /// Every key in the store, prefixes included. The schema test reads
    /// this to hold the on-disk contract still.
    #[must_use]
    pub fn raw_keys(&self) -> Vec<String> {
        self.inner
            .collect_keys(None, None)
            .into_iter()
            .map(|k| String::from_utf8_lossy(&k).into_owned())
            .collect()
    }
}

/// One kind of thing the store remembers, behind its key prefix.
///
/// Callers never spell a prefix themselves — a key built at the call
/// site is a key that drifts from the one the reader uses.
pub struct Namespace<'a> {
    store: &'a Store,
    prefix: &'static str,
}

impl Namespace<'_> {
    fn full(&self, id: &str) -> String {
        format!("{}{id}", self.prefix)
    }

    /// Read one record. `None` means never written — a value that
    /// exists but cannot be read is [`StoreError::Corrupt`], not
    /// silence.
    pub fn get(&self, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let key = self.full(id);
        self.store
            .inner
            .get(key.as_bytes())
            .map_err(|source| StoreError::Op {
                op: "get",
                key,
                source,
            })
    }

    /// Write one record, replacing any previous value.
    pub fn put(&self, id: &str, value: &[u8]) -> Result<(), StoreError> {
        let key = self.full(id);
        self.store
            .inner
            .set(key.as_bytes(), value)
            .map(|_| ())
            .map_err(|source| StoreError::Op {
                op: "put",
                key,
                source,
            })
    }

    /// Remove one record. Removing what is not there succeeds.
    pub fn delete(&self, id: &str) -> Result<(), StoreError> {
        let key = self.full(id);
        self.store
            .inner
            .del(&[key.as_bytes()])
            .map(|_| ())
            .map_err(|source| StoreError::Op {
                op: "delete",
                key,
                source,
            })
    }

    /// Read one record and parse it.
    ///
    /// Three outcomes, kept distinct on purpose:
    /// `Ok(None)` — never written.
    /// `Err(Corrupt)` — bytes are there but are not the expected shape.
    /// `Ok(Some(v))` — the value.
    ///
    /// Folding the middle case into the first is what turns corruption
    /// into "nothing here", and the next write erases the evidence.
    pub fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        id: &str,
    ) -> Result<Option<T>, StoreError> {
        let Some(bytes) = self.get(id)? else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| StoreError::Corrupt {
                key: self.full(id),
                expected: "JSON",
                detail: e.to_string(),
            })
    }

    /// Serialize and write one record.
    pub fn put_json<T: serde::Serialize>(&self, id: &str, value: &T) -> Result<(), StoreError> {
        let bytes = serde_json::to_vec(value).map_err(|e| StoreError::Corrupt {
            key: self.full(id),
            expected: "serializable value",
            detail: e.to_string(),
        })?;
        self.put(id, &bytes)
    }

    /// Every id in this namespace, prefix stripped.
    #[must_use]
    pub fn list(&self) -> Vec<String> {
        let pattern = format!("{}*", self.prefix);
        self.store
            .inner
            .collect_keys(Some(pattern.as_bytes()), None)
            .into_iter()
            .filter_map(|k| {
                let s = String::from_utf8_lossy(&k).into_owned();
                s.strip_prefix(self.prefix).map(str::to_string)
            })
            .collect()
    }
}

/// A value the store holds exactly one of.
pub struct Singleton<'a> {
    store: &'a Store,
    name: &'static str,
}

impl Singleton<'_> {
    fn key(&self) -> String {
        format!("{}{}", prefix::SINGLETON, self.name)
    }

    /// Read and parse it. `Ok(None)` means never written; bytes that do
    /// not parse are [`StoreError::Corrupt`], never silence.
    pub fn get_json<T: serde::de::DeserializeOwned>(&self) -> Result<Option<T>, StoreError> {
        let key = self.key();
        let raw = self
            .store
            .inner
            .get(key.as_bytes())
            .map_err(|source| StoreError::Op {
                op: "get",
                key: key.clone(),
                source,
            })?;
        let Some(bytes) = raw else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| StoreError::Corrupt {
                key,
                expected: "JSON",
                detail: e.to_string(),
            })
    }

    /// Replace it.
    pub fn put_json<T: serde::Serialize>(&self, value: &T) -> Result<(), StoreError> {
        let key = self.key();
        let bytes = serde_json::to_vec(value).map_err(|e| StoreError::Corrupt {
            key: key.clone(),
            expected: "serializable value",
            detail: e.to_string(),
        })?;
        self.store
            .inner
            .set(key.as_bytes(), &bytes)
            .map(|_| ())
            .map_err(|source| StoreError::Op {
                op: "put",
                key,
                source,
            })
    }

    /// Remove it. Removing what is not there succeeds.
    pub fn delete(&self) -> Result<(), StoreError> {
        let key = self.key();
        self.store
            .inner
            .del(&[key.as_bytes()])
            .map(|_| ())
            .map_err(|source| StoreError::Op {
                op: "delete",
                key,
                source,
            })
    }
}
