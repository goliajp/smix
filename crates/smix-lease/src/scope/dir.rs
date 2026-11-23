//! Where scopes are stored, and changing one under its lock.

use super::{Scope, valid_name};
use crate::ProcIdentity;
use crate::store::machine_root;
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

/// Where a machine's scopes live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeDir(PathBuf);

impl ScopeDir {
    /// This machine's, `None` when it has no home for one.
    #[must_use]
    pub fn machine() -> Option<Self> {
        machine_root().map(|r| Self(r.join("scopes")))
    }

    /// A directory named outright, for a test.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// [`Self::update`] for a scope that has been declared: one that covers
    /// no device is an error naming those that do, and is not created. A
    /// misspelt name would otherwise be a scope that protects nothing.
    pub fn update_declared<T>(
        &self,
        name: &str,
        f: impl FnOnce(&mut Scope) -> T,
    ) -> Result<T, String> {
        valid_name(name)?;
        let undeclared = || {
            let names = self.names();
            let others: Vec<&str> = names
                .iter()
                .map(String::as_str)
                .filter(|n| *n != name)
                .collect();
            let others = if others.is_empty() {
                "this machine has no other".to_string()
            } else {
                format!("this machine has {}", others.join(", "))
            };
            format!(
                "no scope {name} is declared ({others}); \
                 `smix lease scope {name} --device <device>` declares it"
            )
        };
        if !self.file(name).exists() {
            return Err(undeclared());
        }
        self.update(name, |s| (!s.devices.is_empty()).then(|| f(s)))?
            .ok_or_else(undeclared)
    }

    /// Delete a scope nobody holds or waits for. `Ok(Err(scope))` is one
    /// somebody does, left as it was.
    pub fn remove(
        &self,
        name: &str,
        now: DateTime<Utc>,
        alive: &dyn Fn(&ProcIdentity) -> bool,
    ) -> Result<Result<(), Scope>, String> {
        valid_name(name)?;
        let path = self.file(name);
        if !path.exists() {
            return Err(format!("no scope {name} on this machine"));
        }
        // under the scope's lock, so nobody takes it between the look and
        // the delete; the lock file stays, since deleting it would give
        // two processes two different locks
        self.locked(name, || {
            let mut s = self.read(name)?;
            s.settle(now, alive);
            if s.holder.is_some() || !s.queue.is_empty() {
                return Ok(Err(s));
            }
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(Ok(()))
        })
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(format!("{name}.json"))
    }

    /// Every scope's name, sorted.
    pub fn names(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.0) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(|e| {
                let p = e.ok()?.path();
                (p.extension()? == "json").then(|| p.file_stem()?.to_str().map(str::to_string))?
            })
            .collect();
        names.sort();
        names
    }

    /// Read `name`, let `f` change it, and write it back, all under the
    /// scope's lock so two processes never interleave.
    pub fn update<T>(&self, name: &str, f: impl FnOnce(&mut Scope) -> T) -> Result<T, String> {
        self.locked(name, || {
            let mut scope = self.read(name)?;
            let before = scope.clone();
            let out = f(&mut scope);
            if scope != before {
                let path = self.file(name);
                let tmp = self.0.join(format!(".{name}.{}.tmp", std::process::id()));
                let json = serde_json::to_vec_pretty(&scope).map_err(|e| e.to_string())?;
                std::fs::write(&tmp, json).map_err(|e| format!("{}: {e}", tmp.display()))?;
                std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
            }
            Ok(out)
        })
    }

    fn locked<T>(&self, name: &str, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        valid_name(name)?;
        std::fs::create_dir_all(&self.0).map_err(|e| format!("{}: {e}", self.0.display()))?;
        let lock_path = self.0.join(format!(".{name}.lock"));
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(|e| format!("{}: {e}", lock_path.display()))?;
        lock.lock()
            .map_err(|e| format!("{}: {e}", lock_path.display()))?;
        f()
    }

    fn read(&self, name: &str) -> Result<Scope, String> {
        let path = self.file(name);
        match std::fs::read(&path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Scope::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }
}
