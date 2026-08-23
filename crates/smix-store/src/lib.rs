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
//! `kevy-embedded` from crates.io, at whatever version `Cargo.toml` pins —
//! never the sibling checkout, because code written against a local path
//! dependency does not compile for anyone else. Which version that is does
//! not belong here: this paragraph named 3.18 across five bumps after it
//! stopped being true, and a comment cannot be the second place a fact
//! lives without eventually being the wrong one.
//!
//! What is worth stating is what the embed cannot reach. kevy's storage
//! work lands server-first: 5.4's packed row is a `[server]` config key,
//! and the embedded facade neither exposes it nor names the `kevy-store`
//! setter behind it, so a row here keeps the general representation
//! whatever the server's default becomes. It would take a declared table
//! as well, and this crate declares none.

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
    /// Capsule records, keyed by device UDID.
    pub const CAPSULES: &str = "capsule:";
    /// Sets — membership, not records.
    pub const SETS: &str = "set:";
    /// Live-stream session records, keyed by device UDID.
    pub const SESSIONS: &str = "session:";
    /// A project's default device: the alias a project resolves to when
    /// no --device is given, keyed by the project's path. The value is
    /// the alias string only — a pointer into the sims namespace, never a
    /// device fact (no UDID / kind / opt-in lives here).
    pub const PROJECT_DEVICES: &str = "project-device:";
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
        ///
        /// Deliberately `io::Error` and not kevy 4's `KevyError`, even
        /// though the engine now returns its own type. This variant also
        /// carries smix's own filesystem failures — the lock file, the
        /// directory — and making them wear an engine error would be a
        /// lie about where they came from. Nothing in or out of this
        /// workspace matches on the cause, so the structure would buy
        /// nothing while breaking this crate's published API; the
        /// engine's message text survives the conversion intact.
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
        /// The underlying failure. See `Open` for why this is
        /// `io::Error` rather than the engine's own type.
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

/// Compact the AOF when it has outgrown what it holds.
///
/// kevy compacts on its own, but the trigger rides the reaper's tick,
/// and smix is a one-shot CLI: the process is gone in milliseconds and
/// that tick never comes. So the log only ever grew. Measured on a
/// working install: 100 MB of append holding 3,783 live commands —
/// 26 KB of history per surviving fact, replayed in full at the start
/// of every command, twice, forever.
///
/// The check is a `stat`, so the common case costs nothing. The
/// rewrite is synchronous and holds the store lock this process
/// already owns; at this size it is milliseconds, and it happens on
/// roughly one command in a thousand.
///
/// Failure is not fatal and not silent: an uncompacted log is slower,
/// not wrong, and a caller who cannot compact should still get their
/// answer.
fn compact_if_overgrown(store: &KevyStore, dir: &Path) {
    /// Past this, the log is mostly history. Redis' own floor is 64
    /// MiB and it compacts on a live server's timer; smix has no timer,
    /// so it compacts lower and on open.
    const COMPACT_ABOVE_BYTES: u64 = 16 * 1024 * 1024;

    let Ok(meta) = std::fs::metadata(dir.join("aof-0.aof")) else {
        return;
    };
    if meta.len() < COMPACT_ABOVE_BYTES {
        return;
    }
    match store.rewrite_aof() {
        Ok(Some(stats)) => eprintln!(
            "smix: compacted the state log — {} bytes for {} keys (was {})",
            stats.bytes,
            stats.keys,
            meta.len()
        ),
        Ok(None) => {}
        Err(e) => eprintln!(
            "smix: the state log is {} bytes and could not be compacted: {e}",
            meta.len()
        ),
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
                source: std::io::Error::other(source),
            }
        })?;
        compact_if_overgrown(&inner, &dir);
        Ok(Store {
            inner,
            root: root.to_path_buf(),
            _lock: lock,
        })
    }

    /// Open the store only if nobody else holds it.
    ///
    /// [`Self::open`] blocks, which is right for a command the user is
    /// waiting on. It is wrong on a hot path: the subprocess ring
    /// persists after *every* `xcrun simctl` invocation, and blocking
    /// there would stall real work behind an unrelated smix process for
    /// the sake of a diagnostic record.
    ///
    /// `Ok(None)` means the store is busy. Callers who get it should do
    /// what they already do when a best-effort write fails — carry on,
    /// and let the next attempt persist.
    pub fn try_open(root: &Path) -> Result<Option<Self>, StoreError> {
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
        if lock.try_lock().is_err() {
            return Ok(None);
        }
        let dir = root.join("kv");
        let inner = KevyStore::open(Config::default().with_persist(&dir)).map_err(|source| {
            StoreError::Open {
                path: dir.clone(),
                source: std::io::Error::other(source),
            }
        })?;
        Ok(Some(Store {
            inner,
            root: root.to_path_buf(),
            _lock: lock,
        }))
    }

    /// Where this store persists. Useful in errors and diagnostics.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether a smix built against kevy 3 could still open this store.
    ///
    /// `Some(true)`: the log is still in the old record format, so going
    /// back is an install. `Some(false)`: a rewrite has upgraded it, and
    /// going back now needs a keyspace export through a client. `None`:
    /// there is no log to ask about — which is not the same as "no way
    /// back", and a caller that collapses the two reports a loss that
    /// never happened.
    ///
    /// Forwarded rather than reimplemented: this crate is the one place
    /// smix touches the engine, and the engine is what knows.
    #[must_use]
    pub fn downgradeable_to_kevy3(&self) -> Option<bool> {
        self.inner.downgradeable_to_v3()
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
            source: std::io::Error::other(source),
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

    /// A project's default device — its path keyed to an alias string.
    /// The value is a pointer into `sims`, not a device fact.
    #[must_use]
    pub fn project_devices(&self) -> Namespace<'_> {
        Namespace {
            store: self,
            prefix: prefix::PROJECT_DEVICES,
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
                    source: std::io::Error::other(source),
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

    /// Capsule records, one per device.
    ///
    /// Per-udid, unlike the runner record: two simulators under one
    /// workspace each hold their own capsule, and always have.
    #[must_use]
    pub fn capsules(&self) -> Namespace<'_> {
        Namespace {
            store: self,
            prefix: prefix::CAPSULES,
        }
    }

    /// Live-stream session records, one per device.
    #[must_use]
    pub fn sessions(&self) -> Namespace<'_> {
        Namespace {
            store: self,
            prefix: prefix::SESSIONS,
        }
    }

    /// A set of strings.
    ///
    /// Membership, not records: "which sims are capturing right now" is
    /// a question about the set, and asking a [`Namespace`] would mean
    /// storing a value per member that nobody reads.
    ///
    /// Reached through here rather than by handing callers the kevy
    /// handle: one crate opens kevy, and that stays true.
    #[must_use]
    pub fn set(&self, name: &'static str) -> Set<'_> {
        Set { store: self, name }
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
                source: std::io::Error::other(source),
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
                source: std::io::Error::other(source),
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
                source: std::io::Error::other(source),
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
                source: std::io::Error::other(source),
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
                source: std::io::Error::other(source),
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
                source: std::io::Error::other(source),
            })
    }
}

/// A set of strings in the store.
pub struct Set<'a> {
    store: &'a Store,
    name: &'static str,
}

impl Set<'_> {
    fn key(&self) -> String {
        format!("{}{}", prefix::SETS, self.name)
    }

    /// Add a member. Adding one twice leaves one.
    pub fn add(&self, member: &str) -> Result<(), StoreError> {
        let key = self.key();
        self.store
            .inner
            .sadd(key.as_bytes(), &[member.as_bytes()])
            .map(|_| ())
            .map_err(|source| StoreError::Op {
                op: "sadd",
                key,
                source: std::io::Error::other(source),
            })
    }

    /// Remove a member. Removing an absent one succeeds.
    pub fn remove(&self, member: &str) -> Result<(), StoreError> {
        let key = self.key();
        self.store
            .inner
            .srem(key.as_bytes(), &[member.as_bytes()])
            .map(|_| ())
            .map_err(|source| StoreError::Op {
                op: "srem",
                key,
                source: std::io::Error::other(source),
            })
    }

    /// Every member. Unordered — a set has no order, and a caller that
    /// depends on one is depending on an accident.
    pub fn members(&self) -> Result<Vec<String>, StoreError> {
        let key = self.key();
        let raw = self
            .store
            .inner
            .smembers(key.as_bytes())
            .map_err(|source| StoreError::Op {
                op: "smembers",
                key,
                source: std::io::Error::other(source),
            })?;
        Ok(raw
            .into_iter()
            .map(|m| String::from_utf8_lossy(&m).into_owned())
            .collect())
    }
}
